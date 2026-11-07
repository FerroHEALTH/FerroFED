// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer to a routed request: the node's own, passed through with the
//! §11.1 status of the node for its access record (§11.2), the gateway's in
//! its place where consent exclusions are withheld (Regulation (EU) 2025/327
//! Art 8), or the gateway's error body for a request the node gave no answer
//! to (N31).

use axum::body::Body;
use axum::response::Response;
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::dispatch::{REQUEST_ID_HEADER, is_consent_refusal};
use ferrofed_engine::single_node::committer::CommitterError;
use ferrofed_engine::single_node::forward::{ForwardError, Forwarded};
use ferrofed_registry::snapshot::Endpoint;
use http::StatusCode;
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;

use super::refused;
use crate::access::Reached;
use crate::error::{self, Code};
use crate::facade::provenance::Provenance;
use crate::federation::Federation;
use crate::node_status;
use crate::security::logged;

/// The answer to a routed request the node answered with `forwarded`, under
/// `provenance`: the node's own ([`answered`]), or [`withheld`]'s where it
/// replaces it.
pub(crate) fn passed(
    provenance: Provenance,
    (federation, endpoint, conveyance): (&Federation, &Endpoint, &Conveyance),
    forwarded: Forwarded,
    ids: (&str, &str),
) -> Response {
    let status = reached(endpoint, &forwarded);
    let mut response = provenance.stamp(
        withheld((federation, conveyance), endpoint, &forwarded, ids)
            .unwrap_or_else(|| answered(forwarded)),
    );
    response.extensions_mut().insert(Reached(status));
    response
}

/// The node's answer `forwarded` as the client's response ([`answered`]),
/// with the §11.1 status of the node at `endpoint` for its access record.
pub(crate) fn answered_by(endpoint: &Endpoint, forwarded: Forwarded) -> Response {
    let status = reached(endpoint, &forwarded);
    let mut response = answered(forwarded);
    response.extensions_mut().insert(Reached(status));
    response
}

/// The §11.1 status of the node at `endpoint` that answered `forwarded`
/// ([`node_status::answered`]).
fn reached(endpoint: &Endpoint, forwarded: &Forwarded) -> EndpointStatus {
    node_status::answered(
        forwarded.status(),
        is_consent_refusal(forwarded, endpoint.consent_refusal_codes()),
    )
}

/// The answer to a routed request in place of `forwarded`, the node's own,
/// where the request on behalf of `conveyance` is served without disclosing
/// consent exclusions ([`Federation::discloses_consent_to`]): `404
/// subject-unavailable`, naming the client's `request_id`, for a node that
/// refused on consent grounds and for a node that answered `404`.
///
/// Both get the one answer, so a client cannot tell a refusal from an absent
/// resource by its status, code, message or headers; the node's own body,
/// which may name the refusal, is never passed on (Regulation (EU) 2025/327
/// Art 8; RFC 9110 §15.5.5). The refusal is read from the registry's codes
/// for `endpoint`, logged for the operator under the gateway's `logged` id,
/// and counted in the node request metrics by [`super::send`].
// NOTE: Regulation (EU) 2025/327 Art 8 against §11.2, which passes a node's 404 through: here it
// is the gateway's, or the gateway's 404 for a refusal would be the only one and name it.
pub(crate) fn withheld(
    (federation, conveyance): (&Federation, &Conveyance),
    endpoint: &Endpoint,
    forwarded: &Forwarded,
    (request_id, logged): (&str, &str),
) -> Option<Response> {
    if federation.discloses_consent_to(conveyance) {
        return None;
    }
    if is_consent_refusal(forwarded, endpoint.consent_refusal_codes()) {
        tracing::info!(
            endpoint = %endpoint.id(),
            request_id = logged,
            "the node refused on consent grounds, which the answer does not disclose"
        );
    } else if forwarded.status() != StatusCode::NOT_FOUND {
        return None;
    }
    Some(error::fixed(Code::SubjectUnavailable, request_id))
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
    if let Some(refused) = refused(failure, request_id, logged) {
        return refused;
    }
    let code = match failure {
        ForwardError::Withheld { endpoint, part } => {
            logged::forward_withheld(endpoint, *part, logged);
            Code::Internal
        }
        ForwardError::TimeOut { .. } | ForwardError::Expired(_) | ForwardError::Capped(_) => {
            Code::NodeTimeout
        }
        ForwardError::Unreachable { .. } => Code::NodeUnreachable,
        ForwardError::Refused { .. } => Code::NodeRefused,
        ForwardError::Committer {
            source: CommitterError::Attestation { .. },
            ..
        }
        | ForwardError::ReadOnly(_) => Code::OperationRefused,
        ForwardError::Credentials { .. }
        | ForwardError::Oversized { .. }
        | ForwardError::Committer { .. } => Code::NodeError,
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
    let message = match failure {
        ForwardError::Credentials {
            endpoint,
            error: ErrorDetail::Text(text),
            ..
        } => format!("endpoint {endpoint}: {text}"),
        other => other.to_string(),
    };
    let mut response = provenance.stamp(error::response(code, message, request_id));
    response
        .extensions_mut()
        .insert(Reached(node_status::unanswered(failure)));
    response
}
