// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read-only operator surface on the client listener, under
//! `{base}/operator/`.
//!
//! It serves the integrity incidents, the `creating_system_id` routing
//! table, the stored-query registry, the last observed state of each
//! dependency, and how the process is deployed. Every route is admitted by
//! client authentication only for a caller whose token carries the operator
//! scope its issuer's entry names
//! ([`Requirement::Operator`](crate::auth::permission::Requirement::Operator)).
//! It answers routing ids, states, counts and stored definitions only, the
//! bodies of [`ferrofed_registry::operator`], [`ferrofed_registry::health`]
//! and the ITS-REST `StoredQuery`, never a patient identifier (§5.4.1, N33),
//! and none of it is part of the ITS-REST surface. The two listings answer one page at a time, `offset` and
//! `limit` in the query, at most [`MAX_PAGE`] items. Every answer names the
//! replica that gave it, in the `ferrofed-replica` header and in each
//! report's `replica` member, because the state is that process's own. No
//! specification governs the operator surface: our own design.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use ferrofed_registry::creating_system::LearnedMap;
use ferrofed_registry::operator::{
    DeploymentReport, IncidentReport, MAX_PAGE, Page, PageRequest, routing_table,
};
use ferrofed_registry::replica::{HEADER, Replica};
use http::{HeaderMap, HeaderName, HeaderValue};
use openehr_its::rest::generated::definition::StoredQuery;

use crate::base_path::BasePath;
use crate::error::{self, Code};
use crate::request_id;
use crate::state::AppState;

/// The prefix every operator route sits under, below `{base}`.
pub const PREFIX: &str = "/operator/";

/// The path of the incident report, below `{base}`.
pub const INCIDENTS: &str = "/operator/incidents";

/// The path of the `creating_system_id` routing table, below `{base}`.
pub const CREATING_SYSTEMS: &str = "/operator/creating-systems";

/// The path of the stored-query registry, below `{base}`.
pub const STORED_QUERIES: &str = "/operator/stored-queries";

/// The path of the dependency report, below `{base}`.
// NOTE: no specification governs this: our own design; the report names every member
// and which are down, so it is the operator's, behind the operator scope.
pub const DEPENDENCIES: &str = "/operator/dependencies";

/// The path of the deployment report, below `{base}`.
// NOTE: no specification governs this: our own design; the report names the profile, the
// development cross-reference, the version and the replica, and no setting value.
pub const DEPLOYMENT: &str = "/operator/deployment";

/// Whether `path` is the operator surface, or below it, under `base`.
#[must_use]
pub fn addresses(base: &BasePath, path: &str) -> bool {
    let operator = base.join(PREFIX.trim_end_matches('/'));
    path == operator
        || path
            .strip_prefix(operator.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Returns the operator routes, added to the client surface.
///
/// Every answer they give, the refusal of a page among them, names
/// `replica` in the [`HEADER`] response header, and each report names it in
/// its `replica` member as well.
pub fn routes(
    surface: axum::Router<Arc<AppState>>,
    replica: &Replica,
) -> axum::Router<Arc<AppState>> {
    // NOTE: no specification governs this: our own design; a replica name's form is a
    // header token's, so the value always builds, and one that did not would name nothing.
    let named = HeaderValue::from_str(replica.as_str()).ok();
    let operator = axum::Router::new()
        .route(INCIDENTS, axum::routing::get(incidents))
        .route(CREATING_SYSTEMS, axum::routing::get(creating_systems))
        .route(STORED_QUERIES, axum::routing::get(stored_queries))
        .route(DEPENDENCIES, axum::routing::get(dependencies))
        .route(DEPLOYMENT, axum::routing::get(deployment))
        .route_layer(axum::middleware::map_response(
            move |mut response: Response| {
                if let Some(value) = &named {
                    response
                        .headers_mut()
                        .insert(HeaderName::from_static(HEADER), value.clone());
                }
                std::future::ready(response)
            },
        ));
    surface.merge(operator)
}

/// `GET {base}/operator/dependencies`: the last observed state of each
/// member endpoint and service, always `200` ([`AppState::dependencies`]),
/// naming the replica that observed them.
async fn dependencies(
    State(state): State<Arc<AppState>>,
) -> Json<ferrofed_registry::health::DependencyReport> {
    Json(state.dependencies())
}

/// `GET {base}/operator/deployment`: the deployment profile, whether the
/// federation resolves through the static development cross-reference,
/// the product version, and the replica that answered; always `200`.
async fn deployment(State(state): State<Arc<AppState>>) -> Json<DeploymentReport> {
    Json(DeploymentReport {
        mode: state.mode(),
        development_crossref: state
            .federation()
            .is_some_and(|federation| federation.development_crossref()),
        product: crate::body::PRODUCT.to_owned(),
        version: crate::body::VERSION.to_owned(),
        replica: state.replica().to_string(),
    })
}

/// `GET {base}/operator/incidents`: every kind's count since the process
/// started, and the most recent incidents of each kind, naming the replica
/// that holds them.
async fn incidents(State(state): State<Arc<AppState>>) -> Json<IncidentReport> {
    let mut report = IncidentReport::current();
    report.replica = Some(state.replica().to_string());
    Json(report)
}

/// The `400` of a page request outside the surface's bound.
fn page_refused(headers: &HeaderMap) -> Response {
    error::response(
        Code::ParameterInvalid,
        format!("limit must be from 1 to {MAX_PAGE}"),
        request_id::of(headers).unwrap_or_default(),
    )
}

/// `GET {base}/operator/creating-systems`: one page of the routing table of
/// the registry document and the learned mappings; empty when the gateway
/// runs without a registry.
async fn creating_systems(
    State(state): State<Arc<AppState>>,
    Query(page): Query<PageRequest>,
    headers: HeaderMap,
) -> Response {
    if !page.is_valid() {
        return page_refused(&headers);
    }
    let table = state.federation().map_or_else(Vec::new, |federation| {
        let learned: LearnedMap = federation.learned().clone();
        routing_table(federation.snapshot(), &learned)
    });
    Json(Page::of(table, page).answered_by(state.replica())).into_response()
}

/// `GET {base}/operator/stored-queries`: one page of the held versions, as
/// ITS-REST `StoredQuery`s; empty when the gateway holds no stored-query
/// registry.
async fn stored_queries(
    State(state): State<Arc<AppState>>,
    Query(page): Query<PageRequest>,
    headers: HeaderMap,
) -> Response {
    if !page.is_valid() {
        return page_refused(&headers);
    }
    let Some(definitions) = state.definitions().map(Arc::clone) else {
        return Json(Page::<StoredQuery>::of(Vec::new(), page).answered_by(state.replica()))
            .into_response();
    };
    // NOTE: no specification governs this: our own design; a shared store is
    // read again first, on a thread that may block, as every stored-query read is.
    let read = tokio::task::spawn_blocking(move || {
        definitions.refresh().map(|()| {
            definitions
                .list("")
                .iter()
                .map(|definition| crate::facade::stored::its_rest(definition))
                .collect::<Vec<_>>()
        })
    })
    .await;
    match read {
        Ok(Ok(held)) => Json(Page::of(held, page).answered_by(state.replica())).into_response(),
        Ok(Err(failure)) => {
            tracing::error!(
                error = crate::chain(&failure),
                "the stored-query store could not be read for the operator"
            );
            error::fixed(Code::Internal, request_id::of(&headers).unwrap_or_default())
        }
        Err(failure) => {
            tracing::error!(
                error = %failure,
                "the stored-query read for the operator did not complete"
            );
            error::fixed(Code::Internal, request_id::of(&headers).unwrap_or_default())
        }
    }
}
