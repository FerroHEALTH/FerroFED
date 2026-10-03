// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Single-node routing: a request to `{base}/v1/ehr/{ehr_id}` or below it,
//! forwarded to the one node that owns the `ehr_id` and answered as that node
//! answered (§7a.1, §7a.3, §12.5).
//!
//! `openehr-its`'s `routes::lookup` names the ITS-REST operation from the
//! method and the path without reading the body, and every operation of the
//! EHR area under a path `ehr_id` is forwarded through the engine's
//! `NodeClient::forward`: the body byte for byte, never decoded and
//! re-encoded through the typed server traits, so no openEHR uid is rewritten
//! (N22, N33). The node's status, body, `Location` and `ETag` come back as the
//! node sent them, a `404` or a `500` included (§11.2), and every routed
//! answer carries `openEHR-federation-endpoint` and
//! `openEHR-federation-system-id` (N31, §9.6).
//!
//! The owner is found in the order of §12.5.1 ([`owner`]): the
//! targeting headers, a resolution binding of the client session, the `ehr_id` index,
//! and for a read only, the ask-all probe of every member, all within the
//! request's budget (§11.5). A write none of the first three routes is a
//! `400`, and is never probed (N41); so is a read whose `ehr_id` is no bare
//! UUID, because the probe would carry it to every member (§5.4.1, N33). An
//! `ehr_id` a binding, the index or the probe finds at several members is a
//! `409` listing the claimants, on a write as on a read, and raises the
//! integrity incident of N42; no claimant is sent the request (§12.5.2). A
//! successful answer teaches the index that its node holds the `ehr_id`, and
//! teaches the follow-up routing table the versions it names ([`follow_up`];
//! §12.2, N21). A read of one version routes the same way: by its path
//! `ehr_id`, never by the version's `creating_system_id` (§12a.1, N41).
//!
//! A versioned write routes by its path `ehr_id` too, and is sent only when
//! that node controls every version it amends, a `CONTRIBUTION`'s included
//! ([`write`](mod@write); §12.4, §12a.1, N23): one that does not is refused
//! `409`, and no node is sent the write (§10.3).
//! A new EHR has no owner, so only the targeting headers route it,
//! `POST {base}/v1/ehr` included, to exactly one endpoint (§12.4, §2.3).
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

use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::response::Response;
use ferrofed_engine::declared::{self, Refusal};
use ferrofed_engine::dispatch::{DispatchOptions, REQUEST_ID_HEADER};
use ferrofed_engine::forward::{ClientRequest, ForwardError, Forwarded};
use ferrofed_engine::hygiene;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::probe::{self, Answer, Probe, ProbedEhrId};
use ferrofed_identity::binding::SessionKey;
use ferrofed_registry::id::EhrId;
use ferrofed_registry::incident::Detection;
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus};
use http::{HeaderMap, Method, Uri};
use openehr_base::prelude::ObjectVersionId;
use openehr_its::rest::routes::{self, Lookup, RouteMatch};

use crate::error::{self, Code};
use crate::facade::provenance::Provenance;
use crate::facade::route::chosen::{Chooser, named};
use crate::facade::write::{self, Write};
use crate::facade::{follow_up, owner, security, subject};
use crate::federation::Federation;

mod chosen;
mod fan_out;

/// The API group of the EHR area (§7a.1).
pub(crate) const EHR_GROUP: &str = "ehr";

/// The API group of the definition area (§7a.1).
const DEFINITION_GROUP: &str = "definition";

/// The API group of the DEMOGRAPHIC area (§7a.1).
const DEMOGRAPHIC_GROUP: &str = "demographic";

/// The path template every single-node EHR resource sits at or below
/// (§7a.1).
const EHR_RESOURCE: &str = "/ehr/{ehr_id}";

/// The path parameter that names the EHR (§12.5).
const EHR_ID_PARAM: &str = "ehr_id";

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
    match routes::lookup(arrived.method, arrived.path) {
        Lookup::Matched(matched) if in_ehr_area(&matched) => {
            route(federation, arrived, &matched).await
        }
        Lookup::Matched(matched) if write::creates_ehr(&matched) => {
            named(federation, arrived, &matched, EHR_GROUP, Chooser::Client).await
        }
        Lookup::Matched(matched) if in_definition_area(&matched) => {
            fan_out::definition(federation, arrived, &matched).await
        }
        Lookup::Matched(matched) if in_demographic_area(&matched) => {
            match federation.demographic_endpoint() {
                Some(declared) => {
                    let chooser = Chooser::Declared(declared);
                    named(federation, arrived, &matched, DEMOGRAPHIC_GROUP, chooser).await
                }
                None => error::fixed(Code::NotImplemented, arrived.request_id),
            }
        }
        Lookup::Matched(matched) if subject::serves(&matched) => {
            subject::serve(federation, arrived, &matched).await
        }
        Lookup::Matched(_) | Lookup::MethodNotAllowed { .. } | Lookup::NotFound => {
            error::fixed(Code::NotImplemented, arrived.request_id)
        }
    }
}

/// Whether `matched` is an operation of the DEMOGRAPHIC area under
/// `{base}/v1/demographic/` (§7a.1, N32).
pub(crate) fn in_demographic_area(matched: &RouteMatch) -> bool {
    matched.group == DEMOGRAPHIC_GROUP
}

/// Whether `matched` is an operation on an EHR resource addressed by a path
/// `ehr_id` (§7a.1).
pub(crate) fn in_ehr_area(matched: &RouteMatch) -> bool {
    matched.group == EHR_GROUP
        && matched
            .template
            .strip_prefix(EHR_RESOURCE)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Whether `matched` is an operation of the definition area, a template or
/// a stored query under `{base}/v1/definition/`, that the gateway routes to
/// one node (§7a.1, §12.6).
pub(crate) fn in_definition_area(matched: &RouteMatch) -> bool {
    matched.group == DEFINITION_GROUP
}

/// Routes one request in the EHR area to the owner of its path `ehr_id` and
/// answers as the owner did.
///
/// The path `ehr_id` is parsed first, and a malformed one is a `400` before
/// any routing (§12.5); so is a query parameter the operation does not
/// declare, or a declared header or query value that does not match its
/// declared kind, before anything is sent (§5.4.1, N33). The owner is then found in
/// the order of §12.5.1 (N41): the targeting headers, a binding the client
/// session holds, the `ehr_id` index, and for a read only, the ask-all probe.
/// A write none of the first three routes is a `400` (`target-required`),
/// and so is a read whose `ehr_id` is no bare UUID (`probe-requires-uuid`):
/// nothing is probed. A new EHR is routed by the targeting headers alone,
/// and a versioned write is sent only when the node controls the version it
/// amends ([`write::controlled`]; §12.4, N23).
async fn route(federation: &Federation, arrived: Arrived<'_>, matched: &RouteMatch) -> Response {
    let started = Instant::now();
    let request_id = arrived.request_id;
    // NOTE: §5.4.1, N33: the client's id is free text that may carry an
    // identifier, so every log event names the gateway's own id instead.
    let logged = arrived.outbound.to_string();
    let Some(ehr_id) = path_ehr_id(matched) else {
        return error::fixed(Code::EhrIdInvalid, request_id);
    };
    let write = Write::of(matched);
    if let Some(refused) = write::path_refused(write, matched) {
        return error::response(refused.code(), refused.to_string(), request_id);
    }
    if let Some(refused) = refused_carriers(matched, &arrived, &logged) {
        return refused;
    }
    let snapshot = federation.snapshot();
    let located = match locate(federation, arrived.headers, write, &ehr_id, started) {
        Ok(located) => located,
        Err(untargeted) => {
            return error::response(untargeted.code(), untargeted.to_string(), request_id);
        }
    };
    let Some(budget) = Deadlines::from(federation, started) else {
        tracing::error!(
            request_id = logged,
            "the routed request's deadline cannot be represented"
        );
        return error::fixed(Code::Internal, request_id);
    };
    let (endpoint, step, probed) = match located {
        owner::Located::At { endpoint, step } => (endpoint, step, None),
        owner::Located::Collision(claimed) => return collision(&ehr_id, claimed, request_id),
        owner::Located::Unreachable { .. } => {
            return error::fixed(Code::NoDestination, request_id);
        }
        owner::Located::Unknown if arrived.method.is_safe() => {
            // NOTE: §5.4.1, N33: the probe reaches members the client never named,
            // so an ehr_id that may be a patient identifier is never probed.
            let Ok(asked) = ProbedEhrId::try_from(&ehr_id) else {
                security::probe_refused(&logged);
                return error::fixed(Code::ProbeRequiresUuid, request_id);
            };
            let probe = Probe {
                ehr_id: asked,
                headers: arrived.headers.clone(),
                per_node: budget.per_node(),
                overall: budget.overall,
                request_id: arrived.outbound,
            };
            match ask_all(federation, &probe, &logged).await {
                Ok((endpoint, answer)) => {
                    let probed = (arrived.method == Method::GET
                        && arrived.path == probe.path()
                        && arrived.uri.query().is_none_or(str::is_empty))
                    .then_some(answer);
                    (endpoint, owner::Step::AskAll, probed)
                }
                Err((code, message)) => return error::response(code, message, request_id),
            }
        }
        // NOTE: §12a.1 route-ehr, N41: a write no earlier step routes is refused,
        // never routed by its version's creating_system_id alone.
        owner::Located::Unknown => return error::fixed(Code::TargetRequired, request_id),
    };
    if let Write::Versioned(preceding) = write
        && let Err(refused) = write::controlled(
            snapshot,
            preceding,
            (matched, arrived.headers, &arrived.body),
            endpoint.node(),
        )
    {
        return not_controlled(&refused, endpoint, (request_id, &logged));
    }
    // NOTE: §11.1 never contacts a suspended endpoint, so a request that names
    // one resolves to no destination (§11.2); the suspension rule is our own design.
    if endpoint.status() == EndpointStatus::Suspended {
        return error::fixed(Code::NoDestination, request_id);
    }
    tracing::debug!(
        endpoint = %endpoint.id(),
        step = step.as_str(),
        request_id = logged,
        "routed a path ehr_id"
    );
    let provenance = Provenance::of(snapshot, endpoint);
    let forwarded = match probed {
        Some(answer) => Ok(answer),
        None => forward(federation, endpoint, &arrived, &budget, &logged).await,
    };
    match forwarded {
        Ok(forwarded) => {
            let read = follow_up::version_of(arrived.method, matched);
            learn(federation, (&ehr_id, read), endpoint, &forwarded, &logged);
            provenance.stamp(answered(forwarded))
        }
        Err(Failure::Internal) => error::fixed(Code::Internal, request_id),
        Err(Failure::Forward(failure)) => failed(&failure, provenance, (request_id, &logged)),
    }
}

/// The `400` refusing a request whose query string carries a parameter the
/// operation `matched` does not declare, or `None` when it carries none
/// (§5.4.1, N33).
fn query_refused(matched: &RouteMatch, arrived: &Arrived<'_>, logged: &str) -> Option<Response> {
    let query = arrived.uri.query()?;
    let unlisted = hygiene::forwarded_query(matched, query).err()?;
    security::forward_refused(unlisted.position, logged);
    Some(error::response(
        Code::QueryParameterRefused,
        unlisted.to_string(),
        arrived.request_id,
    ))
}

/// What the first three steps of §12.5.1 say about the owner of `ehr_id`
/// for a request that writes `write`, read at `started` (N41).
///
/// A new EHR has no owner for a binding or the index to name, so only the
/// targeting headers route it (§12.4, §8.4, N23).
///
/// # Errors
///
/// Returns [`owner::Untargeted`] when the targeting headers name no one
/// endpoint the registry holds (§8.4.1).
fn locate<'a>(
    federation: &'a Federation,
    headers: &HeaderMap,
    write: Write,
    ehr_id: &EhrId,
    started: Instant,
) -> Result<owner::Located<'a>, owner::Untargeted> {
    let snapshot = federation.snapshot();
    if write == Write::NewEhr {
        return Ok(match owner::targeted(snapshot, headers)? {
            Some(endpoint) => owner::Located::At {
                endpoint,
                step: owner::Step::Target,
            },
            None => owner::Located::Unknown,
        });
    }
    // TODO(#80): the authenticated client session the resolution bindings belong to.
    let session: Option<SessionKey> = None;
    let held = session.as_ref().map(|session| owner::Held {
        bindings: federation.bindings(),
        session,
        now: started,
    });
    owner::located(snapshot, headers, held, federation.index(), ehr_id)
}

/// The refusal of a versioned write the path `ehr_id` routes to `endpoint`,
/// before anything is sent: it names no single preceding version, or the
/// node does not control that version (§10.3, §12.4, N23).
fn not_controlled(
    refused: &write::Refused,
    endpoint: &Endpoint,
    (request_id, logged): (&str, &str),
) -> Response {
    if let write::Refused::NotControlling(_) = refused {
        // NOTE: §10.3 copy-write-reject, CP-29: the controlling node is never asked, so a
        // write from a de-duplicated row is refused alike whether its owner is up or down.
        tracing::warn!(
            endpoint = %endpoint.id(),
            code = refused.code().as_str(),
            request_id = logged,
            "a versioned write was refused at a node that does not control the version it amends"
        );
    }
    error::response(refused.code(), refused.to_string(), request_id)
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

/// The `409` refusing a request whose `ehr_id` the members of `claimed`
/// claim, once its integrity incident is raised (§12.5.2, N42).
///
/// No claimant is sent the request, a read or a write.
fn collision(ehr_id: &EhrId, claimed: owner::Claimed, request_id: &str) -> Response {
    owner::collided(ehr_id, claimed.detection, &claimed.claimants);
    let refused = owner::Unsettled::Claimed(claimed.claimants);
    error::response(refused.code(), refused.to_string(), request_id)
}

/// The `400` for a request whose query string or headers `matched` does not
/// forward, or `None` when every declared value may travel (§5.4.1, N33).
///
/// A query parameter the operation does not declare is
/// `query-parameter-refused`; a declared header or query value that does not
/// match its declared kind is `parameter-value-invalid`. Each refusal is a
/// security event under the gateway's `logged` id.
fn refused_carriers(matched: &RouteMatch, arrived: &Arrived<'_>, logged: &str) -> Option<Response> {
    let request_id = arrived.request_id;
    if let Some(refused) = query_refused(matched, arrived, logged) {
        return Some(refused);
    }
    declared::held(matched, arrived.uri.query(), arrived.headers, &arrived.body)
        .err()
        .map(|refusal| declared_refused(&refusal, request_id, logged))
}

/// The answer to a request whose declared values `refusal` refuses: `400`
/// for a malformed value, a security event as well, and the `406` or `415` a
/// node answers an `Accept` or a `Content-Type` it cannot serve with (RFC
/// 9110 §12.4.1, §15.5.16).
pub(crate) fn declared_refused(refusal: &Refusal, request_id: &str, logged: &str) -> Response {
    let code = match refusal {
        Refusal::Malformed(malformed) => {
            security::value_refused(malformed.carrier(), logged);
            Code::ParameterValueInvalid
        }
        Refusal::NotAcceptable { .. } => Code::MediaTypeNotAcceptable,
        Refusal::UnsupportedMediaType { .. } => Code::MediaTypeUnsupported,
    };
    error::response(code, refusal.to_string(), request_id)
}

/// The `ehr_id` the path segment of `matched` decodes to, or `None` when it
/// is no `HIER_OBJECT_ID`.
fn path_ehr_id(matched: &RouteMatch) -> Option<EhrId> {
    let param = matched.path_param(EHR_ID_PARAM)?;
    // NOTE: §12.5, a segment that is not UTF-8 or not a HIER_OBJECT_ID names
    // no EHR, so either failure is the malformed-ehr_id answer.
    EhrId::new(param.decoded().ok()?).ok()
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

/// Forwards the client's request to `endpoint` once, within `budget`.
async fn forward(
    federation: &Federation,
    endpoint: &Endpoint,
    arrived: &Arrived<'_>,
    budget: &Deadlines,
    logged: &str,
) -> Result<Forwarded, Failure> {
    let options = DispatchOptions::new(budget.per_node()).with_request_id(arrived.outbound);
    let request = ClientRequest {
        method: arrived.method.clone(),
        path: arrived.path.to_owned(),
        query: arrived.uri.query().map(str::to_owned),
        headers: arrived.headers.clone(),
        body: arrived.body.to_vec(),
    };
    send(federation, endpoint, request, &options, logged).await
}

/// Sends `request` to `endpoint` once, under `options`.
pub(crate) async fn send(
    federation: &Federation,
    endpoint: &Endpoint,
    request: ClientRequest,
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
    let forwarded = client.forward(request, options).await;
    let dependencies = federation.dependencies();
    dependencies.forwarded(endpoint.id(), &forwarded);
    forwarded.map_err(Failure::Forward)
}

/// Runs the ask-all probe and returns the one owner it found with its
/// answer, or the code and the message that refuse the read (§12.5.1 step
/// 4).
pub(crate) async fn ask_all<'a>(
    federation: &'a Federation,
    probe: &Probe,
    logged: &str,
) -> Result<(&'a Endpoint, Forwarded), (Code, String)> {
    let internal = || (Code::Internal, Code::Internal.message().to_owned());
    let snapshot = federation.snapshot();
    let members = owner::probed(snapshot);
    let answers = probe::ask_all(federation.clients(), &members, probe)
        .await
        .map_err(|error| {
            tracing::error!(
                error = %crate::chain(&error),
                request_id = logged,
                "the ask-all probe could not run"
            );
            internal()
        })?;
    for (endpoint, answer) in &answers {
        if let Answer::Failed(ForwardError::Withheld { part, .. }) = answer {
            security::forward_withheld(endpoint, *part, logged);
        }
        federation.dependencies().probed(endpoint, answer);
    }
    let (endpoint, answer) = match owner::settled(answers) {
        owner::Settled::Owner { endpoint, answer } => (endpoint, answer),
        owner::Settled::Failed(unsettled) => {
            if let owner::Unsettled::Claimed(claimants) = &unsettled {
                owner::collided(probe.ehr_id.ehr_id(), Detection::AskAll, claimants);
            }
            let code = unsettled.code();
            if code.status().is_server_error() {
                tracing::error!(
                    code = code.as_str(),
                    error = %unsettled,
                    request_id = logged,
                    "the ask-all probe named no owner"
                );
            }
            return Err((code, unsettled.to_string()));
        }
    };
    let declared = snapshot.endpoint(&endpoint).ok_or_else(|| {
        tracing::error!(
            endpoint = %endpoint,
            request_id = logged,
            "a probed endpoint left the snapshot"
        );
        internal()
    })?;
    Ok((declared, answer))
}

/// The node's answer as the client's response: its status, its headers and
/// its body bytes.
///
/// The node's own `X-Request-Id` is dropped, so the response names the
/// gateway's request id like every other answer.
pub(crate) fn answered(forwarded: Forwarded) -> Response {
    let (status, mut headers, body) = forwarded.into_parts();
    headers.remove(REQUEST_ID_HEADER);
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

/// The answer for a routed request the node gave no answer of its own to.
///
/// The gateway's error body under the code the failure names; one that
/// reached the node's wire, or was meant to, still names the acting endpoint
/// (N31). The body names the client's `request_id`, and the log the
/// gateway's own `logged` id.
pub(crate) fn failed(
    failure: &ForwardError,
    provenance: Provenance,
    (request_id, logged): (&str, &str),
) -> Response {
    let code = match failure {
        ForwardError::QueryParameter(unlisted) => {
            security::forward_refused(unlisted.position, logged);
            return error::response(Code::QueryParameterRefused, failure.to_string(), request_id);
        }
        ForwardError::Value(refusal) => return declared_refused(refusal, request_id, logged),
        ForwardError::Withheld { endpoint, part } => {
            security::forward_withheld(endpoint, *part, logged);
            Code::Internal
        }
        ForwardError::TimeOut { .. } => Code::NodeTimeout,
        ForwardError::Unreachable { .. } => Code::NodeUnreachable,
        ForwardError::Refused { .. } => Code::NodeRefused,
        _ => Code::Internal,
    };
    if code.status().is_server_error() {
        tracing::error!(
            code = code.as_str(),
            error = %crate::chain(failure),
            request_id = logged,
            "the routed request failed"
        );
    }
    provenance.stamp(error::response(code, failure.to_string(), request_id))
}

#[cfg(test)]
mod tests {
    use super::{in_definition_area, in_demographic_area, in_ehr_area};
    use http::Method;
    use openehr_its::rest::routes::{Lookup, lookup};

    fn ehr_area(method: &Method, path: &str) -> bool {
        matches!(lookup(method, path), Lookup::Matched(matched) if in_ehr_area(&matched))
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
        matches!(lookup(method, path), Lookup::Matched(matched) if in_demographic_area(&matched))
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
