// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Single-node routing: a request to `{base}/v1/ehr/{ehr_id}` or below it,
//! forwarded to the one node that owns the `ehr_id` and answered as that node
//! answered (§7a.1, §7a.3, §12.5).
//!
//! `openehr-its`'s `routes::lookup` names the ITS-REST operation from the
//! method and the path without reading the body, and every operation of the
//! EHR area under a path `ehr_id` is forwarded through the engine's
//! `NodeClient::forward_held`: the body byte for byte, never decoded and
//! re-encoded through the typed server traits, so no openEHR uid is rewritten
//! (N22, N33). The node's status, body, `Location` and `ETag` come back as the
//! node sent them, a `404` or a `500` included (§11.2), and every routed
//! answer carries `openEHR-federation-endpoint` and
//! `openEHR-federation-system-id` (N31, §9.6).
//!
//! The owner is found in the order of §12.5.1 (`ehr`, over [`owner`]): the
//! targeting headers, a resolution binding of the client session, the
//! `ehr_id` index, and for a read only, the ask-all probe of every member.
//! The client's request is held to its operation once, before it is routed,
//! and the headers composed then are the ones a versioned write is checked
//! against and the node is sent (§5.4.1, N33). A
//! successful answer teaches the index that its node holds the `ehr_id`, and
//! teaches the follow-up routing table the versions it names ([`follow_up`];
//! §12.2, N21). A read of one version routes the same way: by its path
//! `ehr_id`, never by the version's `creating_system_id` (§12a.1, N41).
//!
//! A versioned write routes by its path `ehr_id` too, and is sent only when
//! that node controls every version it amends, a `CONTRIBUTION`'s included
//! ([`write`](mod@crate::facade::write); §12.4, §12a.1, N23): one that does not is refused
//! `409`, and no node is sent the write (§10.3).
//! A new EHR has no owner, so only the targeting headers route it,
//! `POST {base}/v1/ehr` included, to exactly one endpoint (§12.4, §2.3); a
//! path `ehr_id` a binding or the index places at another member is refused
//! `409` and sent nowhere ([`owner::held_elsewhere`]).
//!
//! A request under `{base}/v1/definition/` is routed by the targeting
//! headers alone too, to exactly one endpoint, and answered as that node
//! answered: nothing is picked implicitly and no two nodes' answers are
//! combined (§7a.1, §12.6, §12.7, N43). Where offered, a template upload
//! naming `*` or several endpoints fans out to each member instead.
//!
//! A request under `{base}/v1/demographic/` is never federated (§7a.1, N32).
//! It answers `501` unless the deployment declares one member endpoint for
//! the area (`federation.demographic_endpoint`); then a request whose
//! targeting header names that endpoint goes to it by the same path, and one
//! naming no endpoint or another is refused (§12.4, §12.6, §8.4.1, N23).
//!
//! `GET {base}/v1/ehr` names its EHR by subject, which the gateway resolves
//! and routes by the resolved `ehr_id` ([`subject`]; §5.2, N33).

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Extension;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::Response;
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::declared::Refusal;
use ferrofed_engine::dispatch::{Contact, DispatchOptions};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::single_node::forward::{ClientRequest, ForwardError, Forwarded, HeldRequest};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::consent::Requester;
use ferrofed_identity::session::SessionKey;
use ferrofed_registry::id::EhrId;
use ferrofed_registry::snapshot::Endpoint;
use http::{HeaderMap, Method, Uri};
use openehr_base::prelude::ObjectVersionId;

use openehr_its::rest::routes::{self, Lookup, RouteMatch};

use crate::ITS_REST_PREFIX;
use crate::auth::caller::Caller;
use crate::conveyed;
use crate::error::{self, Code};
use crate::facade::route::chosen::{Chooser, named};
use crate::facade::{confined, follow_up, owner, subject};
use crate::federation::Federation;
use crate::request_id;
use crate::security::logged;
use crate::state::AppState;

pub(crate) mod answer;
mod chosen;
pub(crate) mod ehr;
pub(crate) mod fan_out;

/// The API group of the definition area (§7a.1).
const DEFINITION_GROUP: &str = "definition";

/// One client request under the ITS-REST prefix, as it arrived.
#[derive(Debug)]
pub struct Arrived<'a> {
    /// The request method.
    pub method: &'a Method,
    /// The path relative to the ITS-REST base (`/ehr/…`), still
    /// percent-encoded.
    pub path: &'a str,
    /// The request URI, for its query string.
    pub uri: &'a Uri,
    /// Every header the client sent.
    pub headers: &'a HeaderMap,
    /// The body bytes the client sent.
    pub body: Bytes,
    /// The request id, empty when the client sent none.
    ///
    /// It names the request in the answer only, and never reaches the node
    /// or a log line.
    pub request_id: &'a str,
    /// The gateway's id for the request, the `X-Request-Id` the node receives
    /// (§5.4.1, N33).
    pub outbound: OutboundId,
    /// The verified caller's session, whose resolution bindings route a
    /// path `ehr_id` (§12.5.1 step 2); `None` when no caller was verified.
    pub session: Option<&'a SessionKey>,
    /// Who asks for the data, as the verified caller's token states it,
    /// which the consent pre-filter asks about (§13.4).
    pub requester: Option<&'a Requester>,
    /// The verified caller, whom every identity exchange the request makes
    /// is made for and whom its audit record names (PIXm §2:3.83.5.2.1).
    pub on_behalf: OnBehalfOf,
    /// Whom the request is on behalf of, conveyed to every node it reaches
    /// (§13.1, N24).
    pub conveyance: Conveyance,
    /// The patient a national contact point's treatment relationship
    /// confirmation binds, resolved at every member, which a patient-data
    /// request is held to; `None` for every other caller and request.
    pub(crate) trc: Option<&'a crate::facade::trc::Binding>,
}

/// Every path no route serves.
///
/// A path under [`ITS_REST_PREFIX`] is part of the ITS-REST surface: a
/// request to an EHR resource under a path `ehr_id`, the creation of an
/// EHR, a definition request the stored-query registry does not hold, and a
/// DEMOGRAPHIC request naming the endpoint the deployment declared for it, is
/// routed to one node ([`serve`]; §7a.1, §12.4, §12.6), `OPTIONS`
/// names the methods the gateway serves for the path
/// ([`crate::facade::options::allow`]; §7a.2), and every other path answers `501`
/// (§7a.1, N32), because a `404` would claim the resource does not exist.
/// Every other path answers `404`. No answer of the gateway's own echoes the path.
///
/// A routed request reaches its node under the request's [`OutboundId`],
/// never the client's `x-request-id` (§5.4.1, N33), conveying the verified
/// [`Caller`] (§13.1, N24), whose session the resolution bindings belong to
/// (§12.5.1 step 2).
pub(crate) async fn unrouted(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    let outbound = outbound.map_or_else(OutboundId::mint, |Extension(id)| id);
    let session = crate::facade::session(caller.clone());
    let Some(path) = uri
        .path()
        .strip_prefix(ITS_REST_PREFIX.trim_end_matches('/'))
        .filter(|path| path.starts_with('/'))
    else {
        return error::fixed(Code::NotFound, request_id);
    };
    if method == Method::OPTIONS {
        return crate::facade::options::allow(&state, path, request_id);
    }
    let federation = state.federation();
    let Some(serving) = federation.as_deref() else {
        return error::fixed(Code::NotImplemented, request_id);
    };
    let conveyance = match conveyed::of(serving.signer(), caller.as_deref()) {
        Ok(conveyance) => conveyance,
        Err(unconveyed) => return unconveyed.respond(request_id, &outbound.to_string()),
    };
    let Some(on_behalf) = caller.as_deref().map(Caller::on_behalf) else {
        return conveyed::Unconveyed::NoCaller.respond(request_id, &outbound.to_string());
    };
    let started = Instant::now();
    let conveyance =
        match crate::facade::confined_by(serving, caller.as_deref(), started, conveyance).await {
            Ok(conveyance) => conveyance,
            Err(unconfined) => return unconfined.respond(request_id, &outbound.to_string()),
        };
    let binding = if reaches_patient_data(&method, path) {
        let logged = outbound.to_string();
        match crate::facade::trc::admitted(
            serving,
            caller.as_deref(),
            started,
            (request_id, &logged),
        )
        .await
        {
            Ok(binding) => binding,
            Err(refused) => return refused,
        }
    } else {
        None
    };
    let mut arrived = Arrived {
        method: &method,
        path,
        uri: &uri,
        headers: &headers,
        body,
        request_id,
        outbound,
        session: session.as_ref(),
        requester: caller.as_deref().and_then(Caller::requester),
        on_behalf,
        conveyance,
        trc: binding.as_ref(),
    };
    if let Some(definitions) = state.definitions()
        && let Lookup::Matched(matched) = routes::lookup(&method, path)
    {
        match crate::facade::stored::serve(
            serving,
            (definitions, state.public_url()),
            &matched,
            arrived,
        )
        .await
        {
            Ok(response) => return response,
            Err(unanswered) => arrived = unanswered,
        }
    }
    let arriving = crate::access::routed::Arriving {
        method: arrived.method,
        path: arrived.path,
        query: arrived.uri.query(),
        headers: arrived.headers,
        body: arrived.body.as_ref(),
        request_id: arrived.request_id,
    };
    let routed = serving
        .access_log()
        .and_then(|log| crate::access::routed::Routed::of(log, &arriving, subject::named));
    let response = serve(Some(serving), arrived).await;
    match routed {
        Some(routed) => routed.attach(serving.snapshot(), response).await,
        None => response,
    }
}

/// Whether the operation `method` names at `path` reaches patient data at a
/// node, so a national contact point's request to it is held to its TRC
/// patient ([`crate::facade::trc`]); a definition request and a path no
/// operation matches reach none.
fn reaches_patient_data(method: &Method, path: &str) -> bool {
    match routes::lookup(method, path) {
        Lookup::Matched(matched) => crate::auth::permission::of(&matched)
            .is_some_and(crate::auth::permission::Requirement::reaches_patient_data),
        Lookup::MethodNotAllowed { .. } | Lookup::NotFound => false,
    }
}

/// Answers a request under the ITS-REST prefix that no other route serves.
///
/// A request in the single-node EHR area, the creation of an EHR and a
/// request in the definition area are each routed to one node, and so is a
/// DEMOGRAPHIC request naming the endpoint the deployment declared, and the
/// read of an EHR by subject once the subject is resolved; every other
/// ITS-REST path answers `501`, because the gateway does not expose that
/// area (§7a.1, N32), and so does every path when no federation is
/// configured. The stored-query registry, where offered, answers its own
/// definition requests before this is reached (§12.7).
pub async fn serve(federation: Option<&Federation>, arrived: Arrived<'_>) -> Response {
    let Some(federation) = federation else {
        return error::fixed(Code::NotImplemented, arrived.request_id);
    };
    let looked_up = routes::lookup(arrived.method, arrived.path);
    // NOTE: master08 §Resource Scopes, §12.4: a patient grant reaches data in its patient's
    // existing EHRs, so a new EHR, a definition and a party are beyond it.
    if confined::is_confined(&arrived.conveyance)
        && !matches!(&looked_up, Lookup::Matched(matched) if crate::route_area::in_ehr_area(matched) || crate::route_area::reads_ehr_by_subject(matched))
    {
        let logged = arrived.outbound.to_string();
        return confined::refused("area", arrived.request_id, &logged);
    }
    // NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: a contact point's request is held to its TRC
    // patient, so one that names no patient or EHR it can bind is refused with nothing sent.
    if let (Some(binding), Lookup::Matched(matched)) = (arrived.trc, &looked_up) {
        let unbound = if crate::route_area::creates_ehr(matched) {
            Some("ehr-create")
        } else if crate::route_area::in_demographic_area(matched) {
            Some("demographic")
        } else {
            None
        };
        if let Some(what) = unbound {
            let logged = arrived.outbound.to_string();
            let ids = (arrived.request_id, logged.as_str());
            return crate::facade::trc::unbound(binding, arrived.conveyance.relayed(), what, ids);
        }
    }
    match looked_up {
        Lookup::Matched(matched) if crate::route_area::in_ehr_area(&matched) => {
            ehr::route(federation, arrived, &matched).await
        }
        Lookup::Matched(matched) if crate::route_area::creates_ehr(&matched) => {
            named(
                federation,
                arrived,
                crate::route_area::EHR_GROUP,
                Chooser::Client,
            )
            .await
        }
        Lookup::Matched(matched) if in_definition_area(&matched) => {
            fan_out::definition(federation, arrived, &matched).await
        }
        Lookup::Matched(matched) if crate::route_area::in_demographic_area(&matched) => {
            match federation.demographic_endpoint() {
                Some(declared) => {
                    let chooser = Chooser::Declared(declared);
                    named(
                        federation,
                        arrived,
                        crate::route_area::DEMOGRAPHIC_GROUP,
                        chooser,
                    )
                    .await
                }
                None => error::fixed(Code::NotImplemented, arrived.request_id),
            }
        }
        Lookup::Matched(matched) if crate::route_area::reads_ehr_by_subject(&matched) => {
            subject::serve(federation, arrived, &matched).await
        }
        Lookup::Matched(_) | Lookup::MethodNotAllowed { .. } | Lookup::NotFound => {
            error::fixed(Code::NotImplemented, arrived.request_id)
        }
    }
}

/// Whether `matched` is an operation of the definition area, a template or
/// a stored query under `{base}/v1/definition/`, that the gateway routes to
/// one node (§7a.1, §12.6).
pub(crate) fn in_definition_area(matched: &RouteMatch) -> bool {
    matched.group == DEFINITION_GROUP
}

/// Teaches what `endpoint`'s answer shows: on a success, that its node holds
/// `ehr_id` (§12.5.1 step 3), and to the follow-up routing table, the
/// versions the read named and the answer's `ETag` names (§12.2, N21).
pub(crate) fn learn(
    federation: &Federation,
    (ehr_id, read): (&EhrId, Option<ObjectVersionId>),
    endpoint: &Endpoint,
    forwarded: &Forwarded,
    logged: &str,
) {
    if forwarded.status().is_success() {
        owner::learn(federation.index(), ehr_id, endpoint.node());
    }
    follow_up::learn_from(federation, endpoint.id(), read, forwarded, logged);
}

/// The client's request held to the ITS-REST operation it addresses, with
/// the headers composed for the node once, before anything is routed
/// (§5.4.1, N33).
///
/// The held request is the one the route reads and sends.
///
/// # Errors
///
/// Returns the [`ForwardError`] of [`HeldRequest::hold`], which [`unheld`]
/// answers.
fn held(arrived: &Arrived<'_>) -> Result<HeldRequest, ForwardError> {
    HeldRequest::hold(ClientRequest {
        method: arrived.method.clone(),
        path: arrived.path.to_owned(),
        query: arrived.uri.query().map(str::to_owned),
        headers: arrived.headers.clone(),
        body: arrived.body.to_vec(),
    })
}

/// The answer to a request that could not be held to its operation
/// ([`held`]), naming the client's `request_id`.
///
/// A query parameter the operation does not declare is
/// `query-parameter-refused`; a declared header or query value that does not
/// match its declared kind is `parameter-value-invalid`, and an `Accept` or
/// a `Content-Type` naming no listed media type is a `406` or a `415`
/// ([`refused`]). Any other failure is the gateway's own, logged under its
/// `logged` id.
fn unheld(failure: &ForwardError, request_id: &str, logged: &str) -> Response {
    refused(failure, request_id, logged).unwrap_or_else(|| {
        tracing::error!(
            error = %crate::chain(failure),
            request_id = logged,
            "the routed request could not be held to its operation"
        );
        error::fixed(Code::Internal, request_id)
    })
}

/// The answer to a request the gateway refused for a value it carries, before
/// anything was sent, or `None` for a failure that is no such refusal.
///
/// A query parameter the operation does not declare is
/// `query-parameter-refused`, and a declared value the operation does not
/// admit is answered as [`declared_refused`] answers it. Each refusal is a
/// security event under the gateway's `logged` id.
pub(super) fn refused(failure: &ForwardError, request_id: &str, logged: &str) -> Option<Response> {
    match failure {
        ForwardError::QueryParameter(unlisted) => {
            logged::query_parameter_refused(unlisted.position, logged);
            Some(error::response(
                Code::QueryParameterRefused,
                failure.to_string(),
                request_id,
            ))
        }
        ForwardError::Value(refusal) => Some(declared_refused(refusal, request_id, logged)),
        _ => None,
    }
}

/// The answer to a request whose declared values `refusal` refuses: `400`
/// for a malformed value, a security event as well, and the `406` or `415` a
/// node answers an `Accept` or a `Content-Type` it cannot serve with (RFC
/// 9110 §12.4.1, §15.5.16).
pub(crate) fn declared_refused(refusal: &Refusal, request_id: &str, logged: &str) -> Response {
    let code = match refusal {
        Refusal::Malformed(malformed) => {
            logged::value_refused(malformed.carrier(), logged);
            Code::ParameterValueInvalid
        }
        Refusal::NotAcceptable { .. } => Code::MediaTypeNotAcceptable,
        Refusal::UnsupportedMediaType { .. } => Code::MediaTypeUnsupported,
    };
    error::response(code, refusal.to_string(), request_id)
}

/// The per-node timeout and the overall budget of one routed request, the
/// overall budget counted from its arrival (§11.5, N38).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Deadlines {
    per_node: Duration,
    overall: Instant,
}

impl Deadlines {
    /// The deadlines of a request to `federation` that arrived at `started`,
    /// or `None` when the overall deadline cannot be represented.
    pub(crate) fn from(federation: &Federation, started: Instant) -> Option<Self> {
        let budget = federation.budget();
        Some(Self {
            per_node: budget.per_node(),
            overall: started.checked_add(budget.overall())?,
        })
    }

    /// The instant a node asked now must have answered by: the per-node
    /// timeout, never past the overall budget.
    pub(crate) fn per_node(&self) -> Instant {
        Instant::now()
            .checked_add(self.per_node)
            .map_or(self.overall, |at| at.min(self.overall))
    }

    /// The instant the overall budget runs out.
    pub(crate) fn overall(&self) -> Instant {
        self.overall
    }
}

/// Why a routed request has no answer of the node's to pass on.
#[derive(Debug)]
pub(crate) enum Failure {
    /// The gateway failed on its own side before sending.
    Internal,
    /// The node gave no answer.
    Forward(ForwardError),
}

/// Forwards the held `request` to `endpoint` once, within `budget`, under
/// the gateway's `outbound` id for it, conveying `conveyance`.
async fn forward(
    federation: &Federation,
    endpoint: &Endpoint,
    (request, outbound, conveyance): (HeldRequest, OutboundId, &Conveyance),
    budget: &Deadlines,
    logged: &str,
) -> Result<Forwarded, Failure> {
    let options =
        DispatchOptions::new(budget.per_node(), conveyance.clone()).with_request_id(outbound);
    send(federation, endpoint, request, &options, logged).await
}

/// Sends the held `request` to `endpoint` once, under `options`.
pub(crate) async fn send(
    federation: &Federation,
    endpoint: &Endpoint,
    request: HeldRequest,
    options: &DispatchOptions,
    logged: &str,
) -> Result<Forwarded, Failure> {
    let Some(client) = federation.clients().get(endpoint.id()) else {
        tracing::error!(
            endpoint = %endpoint.id(),
            request_id = logged,
            "a registry endpoint has no node client"
        );
        return Err(Failure::Internal);
    };
    let started = Instant::now();
    let forwarded = client.forward_held(request, options).await;
    federation
        .dependencies()
        .contacted(endpoint.id(), Contact::of_forwarded(&forwarded));
    let refused = forwarded
        .as_ref()
        .is_ok_and(|answer| client.refuses_on_consent(answer));
    federation
        .requests()
        .forwarded(endpoint.id(), (&forwarded, refused), started.elapsed());
    forwarded.map_err(Failure::Forward)
}

#[cfg(test)]
mod tests {
    use super::in_definition_area;

    use http::Method;
    use openehr_its::rest::routes::{Lookup, lookup};

    fn ehr_area(method: &Method, path: &str) -> bool {
        matches!(lookup(method, path), Lookup::Matched(matched) if crate::route_area::in_ehr_area(&matched))
    }

    #[test]
    fn every_resource_under_a_path_ehr_id_is_in_the_ehr_area() {
        assert!(ehr_area(&Method::GET, "/ehr/7d44"));
        assert!(ehr_area(&Method::PUT, "/ehr/7d44"));
        assert!(ehr_area(&Method::POST, "/ehr/7d44/composition"));
        assert!(ehr_area(&Method::PUT, "/ehr/7d44/composition/u::s::1"));
        assert!(ehr_area(&Method::DELETE, "/ehr/7d44/composition/u::s::1"));
        assert!(ehr_area(&Method::GET, "/ehr/7d44/ehr_status"));
        assert!(ehr_area(&Method::POST, "/ehr/7d44/contribution"));
        assert!(ehr_area(&Method::GET, "/ehr/7d44/directory"));
    }

    #[test]
    fn the_ehr_collection_and_every_other_area_are_not() {
        assert!(!ehr_area(&Method::GET, "/ehr"));
        assert!(!ehr_area(&Method::POST, "/ehr"));
        assert!(!ehr_area(&Method::POST, "/query/aql"));
        assert!(!ehr_area(&Method::GET, "/demographic/agent/u::s::1"));
        assert!(!ehr_area(&Method::GET, "/definition/template/adl1.4"));
        assert!(!ehr_area(&Method::DELETE, "/admin/ehr/7d44"));
        assert!(!ehr_area(&Method::PATCH, "/ehr/7d44"));
    }

    fn area(method: &Method, path: &str) -> bool {
        matches!(lookup(method, path), Lookup::Matched(matched) if in_definition_area(&matched))
    }

    #[test]
    fn templates_and_stored_query_definitions_are_the_definition_area() {
        for (method, path) in [
            (Method::GET, "/definition/template/adl1.4/t.v1/example"),
            (Method::POST, "/definition/template/adl2"),
            (Method::GET, "/definition/template/adl2/t.v1/1.0.0"),
            (Method::GET, "/definition/query/org::q/1.0.0"),
            (Method::PUT, "/definition/query/org::q/1.0.0"),
        ] {
            assert!(area(&method, path), "{method} {path}");
        }
        for (method, path) in [(Method::POST, "/query/org::q"), (Method::POST, "/ehr")] {
            assert!(!area(&method, path), "{method} {path}");
        }
    }

    fn demographic(method: &Method, path: &str) -> bool {
        matches!(lookup(method, path), Lookup::Matched(matched) if crate::route_area::in_demographic_area(&matched))
    }

    #[test]
    fn every_party_operation_is_the_demographic_area_and_nothing_else_is() {
        for (method, path) in [
            (Method::POST, "/demographic/person"),
            (Method::GET, "/demographic/person/u::s::1"),
            (Method::PUT, "/demographic/agent/u::s::1"),
            (Method::DELETE, "/demographic/role/u::s::1"),
            (
                Method::GET,
                "/demographic/versioned_party/7d44/revision_history",
            ),
            (Method::POST, "/demographic/contribution"),
        ] {
            assert!(demographic(&method, path), "{method} {path}");
        }
        for (method, path) in [
            (Method::GET, "/demographic/party/u::s::1"),
            (Method::GET, "/ehr/7d44"),
            (Method::POST, "/definition/template/adl2"),
            (Method::POST, "/query/aql"),
        ] {
            assert!(!demographic(&method, path), "{method} {path}");
        }
    }
}
