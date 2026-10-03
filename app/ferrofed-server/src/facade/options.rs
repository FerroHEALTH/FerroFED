// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `OPTIONS {base}/`: the gateway's self-description (§7a.2, N30, CP-23),
//! and `OPTIONS` on a sub-path, answered with the methods the gateway serves
//! there.
//!
//! [`describe`] builds the body from the running [`Federation`] alone: the
//! budget, the completion strategies, the `OFFSET` strategy, the dedup modes,
//! the decomposable aggregates, the node selection and the registry members
//! are each read from what the query path itself uses, so the declaration
//! cannot drift from the behaviour. [`Federation::load`] builds it once at
//! boot as well, so a configuration it cannot describe refuses to start.
//!
//! The body declares no targeting mechanism and no patient-resolution
//! carrier: both are mandatory at every conformant gateway, so §7a.2 leaves
//! them out on purpose. It needs no patient identifier and holds none.
//! A facility the gateway does not offer and the schema gives no member
//! (asynchronous queries, §11.7) is declared by its absence.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus, RegistrySnapshot};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use openehr_federation::aql::{OffsetStrategy, Targeting};
use openehr_federation::error::WireError;
use openehr_federation::headers;
use openehr_federation::id::EndpointId;
use openehr_federation::object::Extra;
use openehr_federation::options::{
    Aggregates, AqlBehaviour, Completeness, DedupDefault, DedupModes, DedupPolicy,
    DefinitionBehaviour, DemographicSupport, GatewayDescription, ItsRestAreas, Localization,
    MemberEndpoint, MembershipStatus, OptIn, OptionsRoot, Paging, SpecVersion, TimeoutPolicy,
};
use openehr_its::rest::routes::{self, Lookup};

use crate::error::{self, Code};
use crate::facade::{QUERY_AQL, route, stored, subject, write};
use crate::federation::Federation;
use crate::request_id;
use crate::state::AppState;

/// The `Allow` value of `{base}/`, which answers `GET` (and so `HEAD`) and
/// `OPTIONS`.
const ROOT_ALLOW: &str = "GET, HEAD, OPTIONS";

/// The `paging` member that carries the `bounded` strategy's bound on
/// `k + n`.
// NOTE: §11.6.2 bounds the second strategy, and the schema's `paging` object
// is open; it names no member for the bound, so the name is our own design.
pub const MAX_WINDOW: &str = "max_window";

/// Why the self-description cannot be built from the running federation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DescribeError {
    /// A value the configuration holds is refused by the wire type.
    #[error("a declared value is refused by the OPTIONS {{base}}/ wire type")]
    Wire(#[from] WireError),
    /// A budget does not fit the integer the body carries it in.
    #[error("the {member} budget does not fit in a whole number of milliseconds")]
    Budget {
        /// The `timeout` member it would fill.
        member: &'static str,
    },
    /// An endpoint names a node the registry does not hold.
    #[error("the endpoint {endpoint} names a node the registry does not hold")]
    UnknownNode {
        /// The endpoint's id.
        endpoint: String,
    },
}

/// `OPTIONS {base}/`: the self-description of the running federation
/// (§7a.2, N30).
///
/// Without a federation the gateway federates nothing and answers as the
/// unserved ITS-REST surface does.
pub async fn options_root(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    let Some(federation) = state.federation() else {
        return error::fixed(Code::NotImplemented, request_id);
    };
    // TODO(#80): answer 401 to a caller the gateway has not authenticated (§7a.2, §13).
    match describe(&federation, state.definitions().is_some()) {
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

/// Builds the `OPTIONS {base}/` body of `federation` (§7a.2, N30), declaring
/// the stored-query registry offered when `registry` is `true` (§12.7, N44).
///
/// # Errors
///
/// Returns a [`DescribeError`] when a configured value is one the wire type
/// refuses, a budget does not fit in a `u64` of milliseconds, or an endpoint
/// names a node the registry does not hold.
pub fn describe(federation: &Federation, registry: bool) -> Result<OptionsRoot, DescribeError> {
    let gateway = GatewayDescription {
        id: federation.id().clone(),
        spec_version: SpecVersion::of_release(openehr_federation::FEDERATION_SPEC)?,
        aql: AqlBehaviour {
            fan_out: fans_out(federation.context().targeting()),
            extra: Extra::new(),
        },
        dedup: DedupPolicy {
            default: DedupDefault,
            modes: DedupModes::new(
                Federation::dedup_modes()
                    .iter()
                    .map(|mode| mode.name().to_owned())
                    .collect(),
            )?,
            request_header: Some(headers::DEDUP.to_owned()),
            extra: Extra::new(),
        },
        timeout: timeout(federation)?,
        completeness: completeness(federation.best_effort()),
        paging: paging(federation.offset_strategy())?,
        aggregates: Aggregates {
            decomposable: federation
                .decomposable_aggregates()
                .iter()
                .map(|function| function.name().to_owned())
                .collect(),
            extra: Extra::new(),
        },
        // TODO(#76): declare fan_out_template_upload true once template fan-out upload is offered.
        // TODO(#78): declare stored_query_fan_out true once definitions fan out to the nodes.
        definition: DefinitionBehaviour::new(false)
            .with_stored_query_registry(registry)?
            .with_stored_query_fan_out(false)?,
        // NOTE: §14.1, fail-closed is the default and the gateway offers no
        // fail-open, so `closed` holds for any localizer it is configured with.
        localization: Localization {
            on_failure: "closed".to_owned(),
            extra: Extra::new(),
        },
        // TODO(#81): declare auth.jwks_uri once the gateway publishes its JWKS (§13.1).
        auth: None,
        its_rest: its_rest(registry, federation.demographic_endpoint())?,
        extra: Extra::new(),
    };
    Ok(OptionsRoot {
        federation: gateway,
        endpoints: members(federation.snapshot())?,
        extra: Extra::new(),
    })
}

/// Whether an undirected query fans out under `targeting`, the node
/// selection the deployment declared (§4.3, N4).
fn fans_out(targeting: Targeting) -> bool {
    matches!(targeting, Targeting::AskAll | Targeting::Localized)
}

/// The `timeout` member: the configured budget, under which a node past it
/// is abandoned and marked `time-out` (§11.5, N38).
fn timeout(federation: &Federation) -> Result<TimeoutPolicy, DescribeError> {
    let budget = federation.budget();
    let millis = |member: &'static str, duration: std::time::Duration| {
        u64::try_from(duration.as_millis()).map_err(|_overflow| DescribeError::Budget { member })
    };
    Ok(TimeoutPolicy {
        per_node_ms: millis("per_node_ms", budget.per_node())?,
        overall_ms: millis("overall_ms", budget.overall())?,
        // NOTE: §11.5, N38: the schema leaves `policy` open, and the value
        // names what the budget does to a late node.
        policy: "abandon-and-mark".to_owned(),
        extra: Extra::new(),
    })
}

/// The `completeness` member: all-or-nothing by default, and best-effort
/// when offered, selected by `openEHR-federation-completeness: partial`
/// (§11.4, N37).
fn completeness(best_effort: bool) -> Completeness {
    if best_effort {
        Completeness::with_best_effort(OptIn {
            header: Some(headers::COMPLETENESS.to_owned()),
            value: Some(headers::COMPLETENESS_PARTIAL.to_owned()),
            extra: Extra::new(),
        })
    } else {
        Completeness::all_or_nothing_only()
    }
}

/// The `paging` member: the `OFFSET k > 0` strategy and, when it is
/// bounded, its bound on `k + n` (§11.6.2, N39). No cursor is offered, so
/// the strategy is never `cursor` (§11.6.4).
fn paging(strategy: OffsetStrategy) -> Result<Paging, DescribeError> {
    let mut extra = Extra::new();
    if let Some(window) = strategy.max_window() {
        extra.insert_serialized(MAX_WINDOW, &window.get())?;
    }
    Ok(Paging {
        offset_strategy: strategy.name().to_owned(),
        extra,
    })
}

/// The `its_rest` member: how each ITS-REST area is served (§7a.1, N30,
/// N32), with stored queries held at the gateway when `registry` is `true`
/// (§12.7) and every other definition request routed to one explicitly
/// chosen node (§12.6, N43). The DEMOGRAPHIC area is never federated: it is
/// `501`, or routed to the one `demographic` endpoint the deployment
/// declared, which each request names (§7a.1, §12.4, §12.6, N23, N32).
fn its_rest(
    registry: bool,
    demographic: Option<&ferrofed_registry::id::EndpointId>,
) -> Result<ItsRestAreas, DescribeError> {
    let (query, definition) = if registry {
        (
            "federated: GET and POST {base}/v1/query/aql and GET and POST \
             {base}/v1/query/{name}[/{version}] fan out",
            "routed-single-node: a template request under {base}/v1/definition/template/ goes \
             to the one endpoint the targeting headers name, never merged; \
             stored queries at the gateway registry",
        )
    } else {
        (
            "federated: GET and POST {base}/v1/query/aql fan out",
            "routed-single-node: a request under {base}/v1/definition/ goes to the one \
             endpoint the targeting headers name, never merged",
        )
    };
    Ok(ItsRestAreas {
        query: query.to_owned(),
        ehr: "routed: a request under {base}/v1/ehr/{ehr_id} goes to the one node \
              that owns the ehr_id, found by the targeting headers, the session's \
              resolution binding, the ehr_id index, then for a read an ask-all probe; \
              a versioned write only when that node controls the version it amends; \
              GET {base}/v1/ehr?subject_id= resolves the subject and goes to the one \
              member that holds it, by its ehr_id; \
              POST {base}/v1/ehr to the one endpoint the targeting headers name"
            .to_owned(),
        definition: definition.to_owned(),
        demographic: DemographicSupport::new(match demographic {
            Some(endpoint) => format!(
                "routed-single-node: a request under {{base}}/v1/demographic/ names the \
                 declared endpoint {endpoint} in the targeting headers and goes to it \
                 alone, never federated"
            ),
            None => "unsupported: 501".to_owned(),
        })?,
        extra: Extra::new(),
    })
}

/// The member endpoints of `snapshot`, in endpoint-id order (§7a.2, N30).
fn members(snapshot: &RegistrySnapshot) -> Result<Vec<MemberEndpoint>, DescribeError> {
    snapshot
        .endpoints()
        .map(|endpoint| member(snapshot, endpoint))
        .collect()
}

/// One member endpoint: its id, managing organisation (N20) and membership
/// status, its node, and the node's `system_id`, product and version where
/// the registry holds them.
fn member(
    snapshot: &RegistrySnapshot,
    endpoint: &Endpoint,
) -> Result<MemberEndpoint, DescribeError> {
    let node = snapshot
        .node(endpoint.node())
        .ok_or_else(|| DescribeError::UnknownNode {
            endpoint: endpoint.id().as_str().to_owned(),
        })?;
    // NOTE: §7a.2 leaves the membership vocabulary open; `active` is its
    // conventional value and `suspended` the registry's own.
    let status = match endpoint.status() {
        EndpointStatus::Active => "active",
        EndpointStatus::Suspended => "suspended",
    };
    Ok(MemberEndpoint {
        id: EndpointId::new(endpoint.id().as_str())?,
        organisation: endpoint.managing_organisation().as_str().to_owned(),
        status: MembershipStatus::new(status)?,
        node_id: Some(endpoint.node().as_str().to_owned()),
        system_id: Some(node.system_id().as_str().to_owned()),
        product: node.product().map(str::to_owned),
        version: node.version().map(str::to_owned),
        latency_ms_p50: None,
        url: None,
        extra: Extra::new(),
    })
}

/// Answers `OPTIONS` on `path`, relative to the ITS-REST base (§7a.2).
///
/// The answer is `204` with the methods the gateway serves for that resource
/// in `Allow`, or `501` for a resource it does not serve (N32; RFC 9110
/// §9.3.7, §10.2.1). The gateway answers itself: no node is asked, so nothing is dispatched.
#[must_use]
pub fn allow(state: &AppState, path: &str, request_id: &str) -> Response {
    let registry = state.definitions().is_some();
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
/// `POST`, and a stored-query definition `GET` and `PUT` at the gateway
/// (§12.7). Where a `demographic` endpoint is configured, a DEMOGRAPHIC
/// resource takes every method
/// ITS-REST declares for it, each routed to that endpoint (§7a.1, N32).
fn served(path: &str, registry: bool, demographic: bool) -> Option<Vec<Method>> {
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
            .filter(|method| {
                matches!(
                    routes::lookup(method, path),
                    Lookup::Matched(matched)
                        if route::in_ehr_area(&matched)
                            || write::creates_ehr(&matched)
                            || route::in_definition_area(&matched)
                            || subject::serves(&matched)
                            || (registry && stored::serves(&matched))
                            || (demographic && route::in_demographic_area(&matched))
                )
            })
            .collect()
    };
    if methods.is_empty() {
        return None;
    }
    methods.push(Method::OPTIONS);
    Some(methods)
}
