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
//! `400`, and is never probed (N41). A successful answer teaches the index
//! that its node holds the `ehr_id`.

use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::response::Response;
use ferrofed_engine::dispatch::{DispatchOptions, REQUEST_ID_HEADER};
use ferrofed_engine::forward::{ClientRequest, ForwardError, Forwarded};
use ferrofed_engine::hygiene;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::probe::{self, Answer, Probe};
use ferrofed_identity::binding::SessionKey;
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};
use openehr_federation::headers;
use openehr_its::rest::routes::{self, Lookup, RouteMatch};

use crate::error::{self, Code};
use crate::facade::{owner, security};
use crate::federation::Federation;

/// The API group of the EHR area (§7a.1).
const EHR_GROUP: &str = "ehr";

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
    /// It names the request in the answer only, and never reaches the node.
    pub request_id: &'a str,
    /// The gateway's id for the request, the `X-Request-Id` the node receives
    /// (§5.4.1, N33).
    pub outbound: OutboundId,
}

/// Answers a request under the ITS-REST prefix that no other route serves.
///
/// A request in the single-node EHR area is routed to one node; every other
/// ITS-REST path answers `501`, because the gateway does not expose that
/// area (§7a.1, N32), and so does every path when no federation is
/// configured.
pub async fn serve(federation: Option<&Federation>, arrived: Arrived<'_>) -> Response {
    let Some(federation) = federation else {
        return error::fixed(Code::NotImplemented, arrived.request_id);
    };
    // TODO(#68): DEMOGRAPHIC at 501 through the generated router, or routed as declared.
    // TODO(#75): definition requests routed to one explicitly chosen node.
    match routes::lookup(arrived.method, arrived.path) {
        Lookup::Matched(matched) if in_ehr_area(&matched) => {
            route(federation, arrived, &matched).await
        }
        Lookup::Matched(_) | Lookup::MethodNotAllowed { .. } | Lookup::NotFound => {
            error::fixed(Code::NotImplemented, arrived.request_id)
        }
    }
}

/// Whether `matched` is an operation on an EHR resource addressed by a path
/// `ehr_id` (§7a.1).
fn in_ehr_area(matched: &RouteMatch) -> bool {
    matched.group == EHR_GROUP
        && matched
            .template
            .strip_prefix(EHR_RESOURCE)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Routes one request in the EHR area to the owner of its path `ehr_id` and
/// answers as the owner did.
///
/// The path `ehr_id` is parsed first, and a malformed one is a `400` before
/// any routing (§12.5); so is a query parameter the operation does not
/// declare, before anything is sent (§5.4.1, N33). The owner is then found in
/// the order of §12.5.1 (N41): the targeting headers, a binding the client
/// session holds, the `ehr_id` index, and for a read only, the ask-all probe.
/// A write none of the first three routes is a `400` (`target-required`),
/// and nothing is probed.
async fn route(federation: &Federation, arrived: Arrived<'_>, matched: &RouteMatch) -> Response {
    let started = Instant::now();
    let request_id = arrived.request_id;
    let Some((segment, ehr_id)) = path_ehr_id(matched) else {
        return error::fixed(Code::EhrIdInvalid, request_id);
    };
    if let Some(query) = arrived.uri.query()
        && let Err(unlisted) = hygiene::forwarded_query(matched, query)
    {
        security::forward_refused(unlisted.position, request_id);
        return error::response(
            Code::QueryParameterRefused,
            unlisted.to_string(),
            request_id,
        );
    }
    // TODO(#80): the authenticated client session the resolution bindings belong to.
    let session: Option<SessionKey> = None;
    let held = session.as_ref().map(|session| owner::Held {
        bindings: federation.bindings(),
        session,
        now: started,
    });
    let snapshot = federation.snapshot();
    let located = match owner::located(snapshot, arrived.headers, held, federation.index(), &ehr_id)
    {
        Ok(located) => located,
        Err(untargeted) => {
            return error::response(untargeted.code(), untargeted.to_string(), request_id);
        }
    };
    let Some(budget) = Deadlines::from(federation, started) else {
        tracing::error!("the routed request's deadline cannot be represented");
        return error::fixed(Code::Internal, request_id);
    };
    let (endpoint, step, probed) = match located {
        owner::Located::At { endpoint, step } => (endpoint, step, None),
        owner::Located::Unreachable { .. } => {
            return error::fixed(Code::NoDestination, request_id);
        }
        owner::Located::Unknown if arrived.method.is_safe() => {
            let probe = Probe {
                ehr_id_segment: segment,
                headers: arrived.headers.clone(),
                per_node: budget.per_node(),
                overall: budget.overall,
                request_id: arrived.outbound,
            };
            match ask_all(federation, &probe, request_id).await {
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
        // TODO(#65): a versioned write routed to its controlling CDR by the target version's creating_system_id (§12.4, N23).
        owner::Located::Unknown => return error::fixed(Code::TargetRequired, request_id),
    };
    // NOTE: §11.1 never contacts a suspended endpoint, so a request that names
    // one resolves to no destination (§11.2); the suspension rule is our own design.
    if endpoint.status() == EndpointStatus::Suspended {
        return error::fixed(Code::NoDestination, request_id);
    }
    tracing::debug!(endpoint = %endpoint.id(), step = step.as_str(), "routed a path ehr_id");
    let system_id = snapshot
        .node(endpoint.node())
        .map(|node| node.system_id().as_str());
    let provenance = Provenance {
        endpoint: endpoint.id(),
        system_id,
    };
    let forwarded = match probed {
        Some(answer) => Ok(answer),
        None => forward(federation, endpoint, &arrived, &budget).await,
    };
    match forwarded {
        Ok(forwarded) => {
            if forwarded.status().is_success() {
                owner::learn(federation.index(), &ehr_id, endpoint.node());
            }
            provenance.stamp(answered(forwarded))
        }
        Err(Failure::Internal) => error::fixed(Code::Internal, request_id),
        Err(Failure::Forward(failure)) => failed(&failure, provenance, request_id),
    }
}

/// The `ehr_id` path segment of `matched` as received, and the `ehr_id` it
/// decodes to, or `None` when it is no `HIER_OBJECT_ID`.
fn path_ehr_id(matched: &RouteMatch) -> Option<(String, EhrId)> {
    let param = matched.path_param(EHR_ID_PARAM)?;
    // NOTE: §12.5, a segment that is not UTF-8 or not a HIER_OBJECT_ID names
    // no EHR, so either failure is the malformed-ehr_id answer.
    let ehr_id = EhrId::new(param.decoded().ok()?).ok()?;
    Some((param.raw.clone(), ehr_id))
}

/// The per-node timeout and the overall budget of one routed request, the
/// overall budget counted from its arrival (§11.5, N38).
#[derive(Debug, Clone, Copy)]
struct Deadlines {
    per_node: Duration,
    overall: Instant,
}

impl Deadlines {
    /// The deadlines of a request to `federation` that arrived at `started`,
    /// or `None` when the overall deadline cannot be represented.
    fn from(federation: &Federation, started: Instant) -> Option<Self> {
        let budget = federation.budget();
        Some(Self {
            per_node: budget.per_node(),
            overall: started.checked_add(budget.overall())?,
        })
    }

    /// The instant a node asked now must have answered by: the per-node
    /// timeout, never past the overall budget.
    fn per_node(&self) -> Instant {
        Instant::now()
            .checked_add(self.per_node)
            .map_or(self.overall, |at| at.min(self.overall))
    }
}

/// Why a routed request has no answer of the node's to pass on.
#[derive(Debug)]
enum Failure {
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
) -> Result<Forwarded, Failure> {
    let Some(client) = federation.clients().get(endpoint.id()) else {
        tracing::error!(endpoint = %endpoint.id(), "a registry endpoint has no node client");
        return Err(Failure::Internal);
    };
    let options = DispatchOptions::new(budget.per_node()).with_request_id(arrived.outbound);
    let request = ClientRequest {
        method: arrived.method.clone(),
        path: arrived.path.to_owned(),
        query: arrived.uri.query().map(str::to_owned),
        headers: arrived.headers.clone(),
        body: arrived.body.to_vec(),
    };
    client
        .forward(request, &options)
        .await
        .map_err(Failure::Forward)
}

/// Runs the ask-all probe and returns the one owner it found with its
/// answer, or the code and the message that refuse the read (§12.5.1 step
/// 4).
async fn ask_all<'a>(
    federation: &'a Federation,
    probe: &Probe,
    request_id: &str,
) -> Result<(&'a Endpoint, Forwarded), (Code, String)> {
    let internal = || (Code::Internal, Code::Internal.message().to_owned());
    let snapshot = federation.snapshot();
    let members = owner::probed(snapshot);
    let answers = probe::ask_all(federation.clients(), &members, probe)
        .await
        .map_err(|error| {
            tracing::error!(error = %crate::chain(&error), "the ask-all probe could not run");
            internal()
        })?;
    for (endpoint, answer) in &answers {
        if let Answer::Failed(ForwardError::Withheld { part, .. }) = answer {
            security::forward_withheld(endpoint, *part, request_id);
        }
    }
    let (endpoint, answer) = match owner::settled(answers) {
        owner::Settled::Owner { endpoint, answer } => (endpoint, answer),
        owner::Settled::Failed(unsettled) => {
            let code = unsettled.code();
            if code.status().is_server_error() {
                tracing::error!(
                    code = code.as_str(),
                    error = %unsettled,
                    "the ask-all probe named no owner"
                );
            }
            return Err((code, unsettled.to_string()));
        }
    };
    let declared = snapshot.endpoint(&endpoint).ok_or_else(|| {
        tracing::error!(endpoint = %endpoint, "a probed endpoint left the snapshot");
        internal()
    })?;
    Ok((declared, answer))
}

/// The node's answer as the client's response: its status, its headers and
/// its body bytes.
///
/// The node's own `X-Request-Id` is dropped, so the response names the
/// gateway's request id like every other answer.
fn answered(forwarded: Forwarded) -> Response {
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
/// (N31).
fn failed(failure: &ForwardError, provenance: Provenance<'_>, request_id: &str) -> Response {
    let code = match failure {
        ForwardError::QueryParameter(unlisted) => {
            security::forward_refused(unlisted.position, request_id);
            return error::response(Code::QueryParameterRefused, failure.to_string(), request_id);
        }
        ForwardError::Withheld { endpoint, part } => {
            security::forward_withheld(endpoint, *part, request_id);
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
            "the routed request failed"
        );
    }
    provenance.stamp(error::response(code, failure.to_string(), request_id))
}

/// The acting endpoint and its node's `system_id`, which every routed answer
/// names (§7a.3, N31).
#[derive(Debug, Clone, Copy)]
struct Provenance<'a> {
    endpoint: &'a EndpointId,
    system_id: Option<&'a str>,
}

impl Provenance<'_> {
    /// `response` with `openEHR-federation-endpoint` set to the acting
    /// endpoint and `openEHR-federation-system-id` to its node's `system_id`.
    #[expect(
        clippy::expect_used,
        reason = "registry ids are ASCII letters, digits and . - _, and a system_id is an openEHR UID, so both are valid header values"
    )]
    fn stamp(self, mut response: Response) -> Response {
        let fields = response.headers_mut();
        let endpoint = HeaderValue::try_from(self.endpoint.as_str())
            .expect("a registry endpoint id should be a valid header value");
        fields.insert(header_name(headers::ENDPOINT), endpoint);
        if let Some(system_id) = self.system_id {
            let system_id = HeaderValue::try_from(system_id)
                .expect("a registry system_id should be a valid header value");
            fields.insert(header_name(headers::SYSTEM_ID), system_id);
        }
        response
    }
}

/// The field name `name` spells, lower-cased as HTTP/2 sends it.
#[expect(
    clippy::expect_used,
    reason = "the federation's header names are ASCII tokens, which are valid field names"
)]
fn header_name(name: &str) -> HeaderName {
    HeaderName::from_bytes(name.as_bytes())
        .expect("a federation header name should be a valid field name")
}

#[cfg(test)]
mod tests {
    use super::in_ehr_area;
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
}
