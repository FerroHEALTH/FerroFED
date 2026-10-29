// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two correlation ids of a request: the client's, for the client
//! exchange, and the gateway's own, for everything past the gateway.
//!
//! The stack of `ferrohealth-server` mints the gateway's id for every
//! request, with no client input, and echoes the client's `x-request-id`
//! when it is short printable ASCII, so a header cannot smuggle a line break
//! or a control character ([`ferrohealth_server::request_id`]). The gateway
//! cannot tell whether a client's free text names a patient, so the client's
//! value goes nowhere else: no node receives it (§5.4.1, N33) and no log
//! line records it (§5.4.3).
//!
//! [`bridge`] reads the stack's id as the [`OutboundId`] every node the
//! request reaches receives and the log records. When the client names no
//! request, the exchange id every error body names is that id, so the
//! client, the log and every node name the same request. No specification
//! governs either header: our own design, under the name every proxy already
//! uses.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use ferrofed_engine::outbound_id::OutboundId;
use ferrohealth_server::request_id::{HEADER, is_legal};
use ferrohealth_telemetry::request_log::RequestId;
use http::{Extensions, HeaderValue};

/// Reads the stack's id of `request` as its [`OutboundId`].
///
/// The id goes on the request for the handler and on the response, and
/// names the request in `x-request-id` when the client named none, so every
/// error body names the exchange id.
pub async fn bridge(mut request: Request, next: Next) -> Response {
    let id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| OutboundId::of_server_id(id.id()));
    if let Some(id) = id {
        request.extensions_mut().insert(id);
        // NOTE: no specification governs this: our own design; the stack removed an
        // illegal client id, so a header left here is one the client named.
        if !request.headers().contains_key(HEADER)
            && let Ok(value) = HeaderValue::from_str(&id.to_string())
        {
            request.headers_mut().insert(HEADER, value);
        }
    }
    let mut response = next.run(request).await;
    if let Some(id) = id {
        response.extensions_mut().insert(id);
    }
    response
}

/// Returns the [`OutboundId`] [`bridge`] recorded in `extensions`.
#[must_use]
pub fn outbound(extensions: &Extensions) -> Option<OutboundId> {
    extensions.get::<OutboundId>().copied()
}

/// Returns the exchange id `headers` carries, when it carries a legal one.
///
/// The value may be the client's own free text: it belongs in the response
/// and in an error body, and never in a log line or a request to a node.
#[must_use]
pub fn of(headers: &http::HeaderMap) -> Option<&str> {
    headers
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| is_legal(value))
}
