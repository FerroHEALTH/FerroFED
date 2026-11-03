// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `OPTIONS {base}/`: the gateway's self-description (§7a.2, N30, CP-23),
//! and `OPTIONS` on a sub-path, answered with the methods the gateway serves
//! there.
//!
//! The body is built by [`crate::federation::describe::describe`] from the
//! running federation, with the national contact points the deployment
//! declares added ([`ContactPoints`]). No specification governs where it is
//! answered: our own design.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use openehr_its::rest::routes::{self, Lookup};

use crate::auth::contact_point::ContactPoints;
use crate::error::{self, Code};
use crate::facade::{QUERY_AQL, route, stored};
use crate::federation::describe::{DescribeError, describe};
use crate::request_id;
use crate::state::AppState;

/// The `Allow` value of `{base}/`, which answers `GET` (and so `HEAD`) and
/// `OPTIONS`.
const ROOT_ALLOW: &str = "GET, HEAD, OPTIONS";

/// `OPTIONS {base}/`: the self-description of the running federation
/// (§7a.2, N30).
///
/// Only a caller the gateway authenticated reaches it ([`crate::auth`]): the
/// endpoint list is subject to the gateway's normal authentication (§7a.2,
/// §13.1). Without a federation the gateway federates nothing and answers as
/// the unserved ITS-REST surface does.
pub async fn options_root(
    State(state): State<Arc<AppState>>,
    Extension(contact_points): Extension<Arc<ContactPoints>>,
    headers: HeaderMap,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    let Some(federation) = state.federation() else {
        return error::fixed(Code::NotImplemented, request_id);
    };
    let described = describe(&federation, state.definitions().is_some()).and_then(|body| {
        contact_points
            .declared_in(body)
            .map_err(DescribeError::from)
    });
    match described {
        Ok(body) => (
            StatusCode::OK,
            [(header::ALLOW, HeaderValue::from_static(ROOT_ALLOW))],
            Json(body),
        )
            .into_response(),
        Err(failure) => {
            tracing::error!(error = %crate::chain(&failure), "the self-description failed");
            error::fixed(Code::Internal, request_id)
        }
    }
}

/// Answers `OPTIONS` on `path`, relative to the ITS-REST base (§7a.2).
///
/// The answer is `204` with the methods the gateway serves for that resource
/// in `Allow`, or `501` for a resource it does not serve (N32; RFC 9110
/// §9.3.7, §10.2.1). The gateway answers itself: no node is asked, so nothing is dispatched.
#[must_use]
pub fn allow(state: &AppState, path: &str, request_id: &str) -> Response {
    let registry = state
        .definitions()
        .map(|definitions| definitions.is_read_only());
    let served = state
        .federation()
        .and_then(|federation| served(path, registry, federation.demographic_endpoint().is_some()));
    let Some(methods) = served else {
        return error::fixed(Code::NotImplemented, request_id);
    };
    let listed = methods
        .iter()
        .map(Method::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    match HeaderValue::try_from(listed) {
        Ok(value) => (StatusCode::NO_CONTENT, [(header::ALLOW, value)]).into_response(),
        Err(_invalid) => {
            tracing::error!("a method list is not a valid header value");
            error::fixed(Code::Internal, request_id)
        }
    }
}

/// The methods the gateway serves for `path`, `OPTIONS` last, or `None`
/// when it serves none there.
///
/// The federated query takes `GET` and `POST` (N1); an EHR resource under a path
/// `ehr_id` takes every method ITS-REST declares for it, because each is
/// routed to one node (§7a.1), and the EHR collection takes `GET`, the read
/// of an EHR by subject at the one member that resolves it (§5.2, N33), and
/// `POST`, the creation of an EHR at the one node the targeting headers name
/// (§12.4).
/// A definition resource takes every method ITS-REST declares for it, each
/// routed to the one node the targeting headers name (§12.6). Where the
/// stored-query `registry` is offered, a stored query takes `GET` and
/// `POST`, and a stored-query definition `GET` and `PUT` at the gateway, or
/// `GET` alone where the registry is read-only, its `Some(true)` (§12.7).
/// Where a `demographic` endpoint is configured, a DEMOGRAPHIC
/// resource takes every method
/// ITS-REST declares for it, each routed to that endpoint (§7a.1, N32).
fn served(path: &str, registry: Option<bool>, demographic: bool) -> Option<Vec<Method>> {
    let query = QUERY_AQL.strip_prefix(crate::ITS_REST_PREFIX.trim_end_matches('/'));
    let mut methods = if query == Some(path) {
        vec![Method::GET, Method::POST]
    } else {
        let Lookup::MethodNotAllowed { allowed } = routes::lookup(&Method::OPTIONS, path) else {
            return None;
        };
        // NOTE: RFC 9110 §9.1, a declared name that is no method token is no
        // method the gateway can serve, so it is absent from `Allow`.
        allowed
            .into_iter()
            .filter_map(|name| Method::from_bytes(name.as_bytes()).ok())
            .filter(|method| match routes::lookup(method, path) {
                Lookup::Matched(matched) => match registry {
                    Some(read_only) if stored::serves(&matched) => {
                        stored::accepts(&matched, read_only)
                    }
                    Some(_) | None => {
                        crate::route_area::in_ehr_area(&matched)
                            || crate::route_area::creates_ehr(&matched)
                            || route::in_definition_area(&matched)
                            || crate::route_area::reads_ehr_by_subject(&matched)
                            || (demographic && crate::route_area::in_demographic_area(&matched))
                    }
                },
                Lookup::MethodNotAllowed { .. } | Lookup::NotFound => false,
            })
            .collect()
    };
    if methods.is_empty() {
        return None;
    }
    methods.push(Method::OPTIONS);
    Some(methods)
}
