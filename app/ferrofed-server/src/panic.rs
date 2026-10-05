// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A handler panic becomes a `500` with a JSON body, never a dropped
//! connection, and every panic leaves one fixed line in the log.
//!
//! The release profile pins `panic = "unwind"`, which is what lets
//! `std::panic::catch_unwind` turn an unwound handler into a response
//! (<https://doc.rust-lang.org/cargo/reference/profiles.html#panic>). The
//! panic message reaches neither the body, the log nor stderr, because a
//! message can quote whatever value the handler held, a patient identifier
//! included (§5.4.3). Rust's default hook prints the message to stderr, so
//! the binary replaces it with [`install_hook`] before it serves. No
//! specification governs the body shape or the hook: our own design.

use axum::response::Response;
use std::any::Any;
use std::panic::PanicHookInfo;

/// Replaces the process panic hook with one that writes a fixed line through
/// `tracing` and nothing to stderr.
///
/// The line names where the panic happened and, when the panicking task
/// serves a request, the gateway's outbound id for it
/// ([`crate::request_id::serving`]). The payload is never read, so its text
/// reaches neither the log nor stderr. The `500` a panicking handler answers
/// is unchanged: [`caught`] and [`render`] still make it.
pub fn install_hook() {
    std::panic::set_hook(Box::new(hook));
}

/// Logs that a thread panicked, with its location and request, never its
/// payload.
fn hook(info: &PanicHookInfo<'_>) {
    let location = info.location().map_or_else(
        || "unknown".to_owned(),
        |at| format!("{}:{}:{}", at.file(), at.line(), at.column()),
    );
    let request_id = crate::request_id::serving().map(|id| id.to_string());
    tracing::error!(
        location = location.as_str(),
        request_id = request_id.as_deref(),
        "a thread panicked"
    );
}

/// The marker a caught panic leaves on its response.
///
/// The panic handler sees the payload and no request, so it marks the response
/// and [`render`] fills the request id in outside the layer that sets it.
#[derive(Debug, Clone, Copy)]
struct Panicked;

/// Renders a panicked handler as a `500` marked for [`render`].
///
/// The payload is dropped unread: its text is not logged and not answered.
#[must_use]
pub fn caught(payload: Box<dyn Any + Send + 'static>) -> Response {
    drop(payload);
    let mut response = crate::error::fixed(crate::error::Code::Internal, "");
    response.extensions_mut().insert(Panicked);
    response
}

/// Fills the exchange id into a panic response and logs that the handler
/// panicked, under the gateway's outbound id.
///
/// It runs outside the layers that mint the outbound id and propagate the
/// exchange id, so both are already on the response: the body names the
/// client's own id and the log line the gateway's, which is the client's too
/// when the client named none ([`crate::request_id`]).
pub async fn render(response: Response) -> Response {
    if response.extensions().get::<Panicked>().is_none() {
        return response;
    }
    let request_id = crate::request_id::of(response.headers())
        .unwrap_or_default()
        .to_owned();
    let outbound = crate::request_id::outbound(response.extensions())
        .map(|id| id.to_string())
        .unwrap_or_default();
    tracing::error!(
        request_id = outbound.as_str(),
        "the request handler panicked"
    );
    let mut rendered = crate::error::fixed(crate::error::Code::Internal, &request_id);
    // The request id header the propagate layer set is on the old response;
    // carry every header over so the client sees the value the body names.
    let content_type = rendered.headers().get(http::header::CONTENT_TYPE).cloned();
    *rendered.headers_mut() = response.headers().clone();
    if let Some(content_type) = content_type {
        rendered
            .headers_mut()
            .insert(http::header::CONTENT_TYPE, content_type);
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::{Panicked, caught};
    use http::StatusCode;

    #[test]
    fn a_caught_panic_is_a_500_carrying_the_marker() {
        let response = caught(Box::new("the handler gave up"));
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, response.status());
        assert!(
            response.extensions().get::<Panicked>().is_some(),
            "the renderer finds the marker"
        );
    }
}
