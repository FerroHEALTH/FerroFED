// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two correlation ids of a request: the client's, for the client
//! exchange, and the gateway's own, for everything past the gateway.
//!
//! The client exchange id is the `x-request-id` the response echoes and every
//! error body names: the client's value when it is short printable ASCII, so
//! a header cannot smuggle a line break or a control character, and otherwise
//! a gateway-minted one. The gateway cannot tell whether a client's free text
//! names a patient, so the client's value goes nowhere else: no node receives
//! it (§5.4.1, N33) and no log line records it (§5.4.3).
//!
//! The outbound id is an [`OutboundId`] [`mint_outbound`] mints for every
//! request, with no client input. It is the id every node the request reaches
//! receives and the id the log records. When the client names no request, the
//! exchange id is the outbound id, so the client, the log and every node name
//! the same request. No specification governs either header: our own design,
//! under the name every proxy already uses.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use ferrofed_engine::outbound_id::OutboundId;
use http::{Extensions, HeaderName, HeaderValue};
use tower_http::request_id::{MakeRequestId, RequestId};

/// The header a client sends to name its request, and the server echoes.
pub const HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// The longest client value this server echoes.
pub const MAX_LENGTH: usize = 128;

/// Returns whether `value` may be echoed as a request id.
///
/// The rule is printable ASCII (`0x20` to `0x7e`) of at most [`MAX_LENGTH`]
/// characters and never empty, so the value is safe in one header.
#[must_use]
pub fn is_legal(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_LENGTH
        && value.chars().all(|c| c.is_ascii_graphic() || c == ' ')
}

/// Mints the exchange id of a request the client named no legal id for.
///
/// It is the request's [`OutboundId`] when [`mint_outbound`] already ran, and
/// a fresh one otherwise, so it never carries client input.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mint;

impl MakeRequestId for Mint {
    fn make_request_id<B>(&mut self, request: &Request<B>) -> Option<RequestId> {
        let id = outbound(request.extensions()).unwrap_or_else(OutboundId::mint);
        HeaderValue::from_str(&id.to_string())
            .ok()
            .map(RequestId::new)
    }
}

tokio::task_local! {
    /// The [`OutboundId`] of the request whose task is running, for the
    /// panic hook, which sees neither the request nor its response.
    static SERVING: OutboundId;
}

/// Mints the gateway's [`OutboundId`] for `request` and records it on the
/// request and on its response.
///
/// The request carries it to the handler and the request log; the response
/// carries it out to the panic renderer, which sees no request; and while the
/// request is served, [`serving`] reads it for the panic hook.
pub async fn mint_outbound(mut request: Request, next: Next) -> Response {
    let id = OutboundId::mint();
    request.extensions_mut().insert(id);
    let mut response = SERVING.scope(id, next.run(request)).await;
    response.extensions_mut().insert(id);
    response
}

/// Returns the [`OutboundId`] [`mint_outbound`] recorded in `extensions`.
#[must_use]
pub fn outbound(extensions: &Extensions) -> Option<OutboundId> {
    extensions.get::<OutboundId>().copied()
}

/// Returns the [`OutboundId`] of the request the current task serves, or
/// `None` outside one.
#[must_use]
pub fn serving() -> Option<OutboundId> {
    // NOTE: no specification governs this: our own design; outside a served
    // request there is no id to read, so the absence is the answer.
    SERVING.try_with(|id| *id).ok()
}

/// Removes an `x-request-id` this server will not echo.
///
/// It runs outside the layer that mints one, so an illegal client value leaves
/// no header behind and the minted id takes its place.
pub async fn strip_illegal(mut request: Request) -> Request {
    let legal = request
        .headers()
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_legal);
    if !legal {
        request.headers_mut().remove(HEADER);
    }
    request
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

#[cfg(test)]
mod tests {
    use super::{MAX_LENGTH, Mint, is_legal};
    use axum::body::Body;
    use ferrofed_engine::outbound_id::OutboundId;
    use http::Request;
    use tower_http::request_id::MakeRequestId as _;

    #[test]
    fn a_printable_ascii_value_of_bounded_length_is_echoed() {
        assert!(is_legal("corr-42"));
        assert!(is_legal("a b"), "a space is printable");
        assert!(is_legal(&"a".repeat(MAX_LENGTH)));
    }

    #[test]
    fn a_control_character_a_non_ascii_byte_an_empty_value_and_an_over_long_one_are_refused() {
        assert!(!is_legal("corr\n42"), "a line break would forge a log line");
        assert!(!is_legal("corr\r42"));
        assert!(!is_legal("corr\t42"));
        assert!(!is_legal("corr\u{7f}42"), "DEL is not printable");
        assert!(!is_legal("corr\u{e9}42"), "only ASCII is echoed");
        assert!(!is_legal(""));
        assert!(!is_legal(&"a".repeat(MAX_LENGTH + 1)));
    }

    #[test]
    fn the_exchange_id_of_an_unnamed_request_is_its_outbound_id() {
        let id = OutboundId::mint();
        let mut request = Request::new(Body::empty());
        request.extensions_mut().insert(id);
        let minted = Mint.make_request_id(&request).expect("an id is minted");
        assert_eq!(id.to_string().as_bytes(), minted.header_value().as_bytes());
    }
}
