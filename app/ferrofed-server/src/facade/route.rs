// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Single-node routing: a request to `{base}/v1/ehr/{ehr_id}` or below it,
//! forwarded to the one node it names and answered as that node answered
//! (§7a.1, §7a.3, §12.5).
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
//! The node is the one the `openEHR-federation-endpoint` or
//! `openEHR-federation-organisation` header selects, the first step of
//! §12.5.1 (§8.4). A write that names none is a `400` (§12.5.1, N41).

use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::response::Response;
use ferrofed_engine::dispatch::{DispatchOptions, REQUEST_ID_HEADER};
use ferrofed_engine::forward::{ClientRequest, ForwardError, Forwarded};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus, RegistrySnapshot};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};
use openehr_federation::headers;
use openehr_its::rest::routes::{self, Lookup, RouteMatch};

use crate::error::{self, Code};
use crate::facade::{security, target};
use crate::federation::Federation;

/// The API group of the EHR area (§7a.1).
const EHR_GROUP: &str = "ehr";

/// The path template every single-node EHR resource sits at or below
/// (§7a.1).
const EHR_RESOURCE: &str = "/ehr/{ehr_id}";

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
        Lookup::Matched(matched) if in_ehr_area(&matched) => route(federation, arrived).await,
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

/// Routes one request in the EHR area to its node and answers as the node
/// did.
async fn route(federation: &Federation, arrived: Arrived<'_>) -> Response {
    let request_id = arrived.request_id;
    // TODO(#62): parse the path ehr_id as a HierObjectId, refusing a malformed one with 400 before routing.
    let endpoint = match target(federation.snapshot(), arrived.headers) {
        Ok(Some(endpoint)) => endpoint,
        Ok(None) if arrived.method.is_safe() => {
            // TODO(#62): route a read by the held binding, the ehr_id index, then an ask-all probe (§12.5.1).
            return error::fixed(Code::NotImplemented, request_id);
        }
        // TODO(#62): route a write by the held binding or the ehr_id index before refusing it (§12.5.1).
        Ok(None) => return error::fixed(Code::TargetRequired, request_id),
        Err(untargeted) => {
            return error::response(untargeted.code(), untargeted.to_string(), request_id);
        }
    };
    // NOTE: §11.1 never contacts a suspended endpoint, so a request that names
    // one resolves to no destination (§11.2); the suspension rule is our own design.
    if endpoint.status() == EndpointStatus::Suspended {
        return error::fixed(Code::NoDestination, request_id);
    }
    let Some(client) = federation.clients().get(endpoint.id()) else {
        tracing::error!(endpoint = %endpoint.id(), "a registry endpoint has no node client");
        return error::fixed(Code::Internal, request_id);
    };
    let Some(deadline) = Instant::now().checked_add(federation.budget().per_node()) else {
        tracing::error!("the routed request's deadline cannot be represented");
        return error::fixed(Code::Internal, request_id);
    };
    let options = DispatchOptions::new(deadline).with_request_id(arrived.outbound);
    let request = ClientRequest {
        method: arrived.method.clone(),
        path: arrived.path.to_owned(),
        query: arrived.uri.query().map(str::to_owned),
        headers: arrived.headers.clone(),
        body: arrived.body.to_vec(),
    };
    let system_id = federation
        .snapshot()
        .node(endpoint.node())
        .map(|node| node.system_id().as_str());
    let provenance = Provenance {
        endpoint: endpoint.id(),
        system_id,
    };
    match client.forward(request, &options).await {
        Ok(forwarded) => provenance.stamp(answered(forwarded)),
        Err(failure) => failed(&failure, provenance, request_id),
    }
}

/// The endpoint the targeting headers name, `None` without either header
/// (§8.4, §12.5.1 step 1).
///
/// The `openEHR-federation-endpoint` and `openEHR-federation-organisation`
/// headers both apply to a routed request, read as [`target::requested`]
/// reads them for a query; a routed request reaches one node, so together
/// they select exactly one endpoint (§7a.1, §12.4).
fn target<'a>(
    snapshot: &'a RegistrySnapshot,
    headers: &HeaderMap,
) -> Result<Option<&'a Endpoint>, Untargeted> {
    let Some(selected) = target::requested(snapshot, None, headers)? else {
        return Ok(None);
    };
    let mut selected = selected.into_iter();
    let id = match (selected.next(), selected.next()) {
        (Some(id), None) => id,
        (Some(_), Some(_)) => return Err(Untargeted::Several),
        (None, _) => return Err(Untargeted::Nothing),
    };
    Ok(Some(registered(snapshot, &id)))
}

/// The endpoint `id` of `snapshot`, which selected it.
#[expect(
    clippy::expect_used,
    reason = "target::requested selects only endpoints it found in this same snapshot"
)]
fn registered<'a>(snapshot: &'a RegistrySnapshot, id: &EndpointId) -> &'a Endpoint {
    snapshot
        .endpoint(id)
        .expect("a selected endpoint should be in the snapshot that selected it")
}

/// Why the targeting headers of a routed request name no one endpoint.
#[derive(Debug, thiserror::Error)]
enum Untargeted {
    /// A header names what the registry does not know, or the two headers
    /// select different node sets (§8.4.1).
    #[error(transparent)]
    Target(#[from] target::TargetError),
    /// The headers select more than one endpoint.
    #[error(
        "the targeting headers select more than one endpoint, and a request routed to one node selects exactly one (§7a.1, §12.4)"
    )]
    Several,
    /// The headers select no endpoint: the organisation manages none.
    #[error("the targeting headers select no endpoint, so the request has no destination (§11.2)")]
    Nothing,
}

impl Untargeted {
    /// The stable code the error body names.
    fn code(&self) -> Code {
        match self {
            Self::Target(error) => error.code(),
            Self::Several => Code::EndpointSeveral,
            Self::Nothing => Code::NoDestination,
        }
    }
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
