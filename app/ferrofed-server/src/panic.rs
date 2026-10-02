// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A handler panic becomes a `500` with a JSON body, never a dropped
//! connection.
//!
//! The release profile pins `panic = "unwind"`, which is what lets
//! `std::panic::catch_unwind` turn an unwound handler into a response
//! (<https://doc.rust-lang.org/cargo/reference/profiles.html#panic>). The
//! panic message reaches neither the body nor the log, because a message can
//! quote whatever value the handler held, a patient identifier included
//! (§5.4.3). No specification governs the body shape: our own design.

use axum::response::Response;
use std::any::Any;

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

/// Fills the request id into a panic response and logs that the handler
/// panicked.
///
/// It runs outside the layer that propagates the request id, so the header is
/// already on the response and the body, the log line and the client's own
/// trace name the same request.
pub async fn render(response: Response) -> Response {
    if response.extensions().get::<Panicked>().is_none() {
        return response;
    }
    let request_id = crate::request_id::of(response.headers())
        .unwrap_or_default()
        .to_owned();
    tracing::error!(
        request_id = request_id.as_str(),
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
