// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The JSON documents this server answers with on its own routes.
//!
//! Each is a typed struct serialized by `serde`, never a free-form JSON value
//! (`.claude/rules/rust-style.md`, typed carriers). The ITS-REST surface
//! answers with the ITS-REST shapes when it lands; these cover the gateway's
//! own routes and its refusals. No specification governs them: our own
//! design.

use axum::Json;
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use serde::Serialize;

/// The product name the root document reports.
pub const PRODUCT: &str = "FerroFED";

/// The product version the root document reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `GET /`: the product and its version.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Root {
    /// The product name.
    pub product: &'static str,
    /// The product version.
    pub version: &'static str,
}

impl Default for Root {
    fn default() -> Self {
        Self {
            product: PRODUCT,
            version: VERSION,
        }
    }
}

/// `GET /health`: the process is up.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Liveness {
    /// Always `up`: a process that can answer is live.
    pub state: crate::health::State,
}

/// What this server answers a request it refuses with.
///
/// It names a stable error code and the request id, never the request's path,
/// query, headers or body, any of which may carry a patient identifier
/// (§5.4.3).
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    /// The stable, machine-readable error code.
    pub error: &'static str,
    /// The request id, so a client and an operator name the same request.
    pub request_id: String,
}

/// Returns `status` with an [`ErrorBody`] carrying `error` and `request_id`.
#[must_use]
pub fn error(status: StatusCode, error: &'static str, request_id: &str) -> Response {
    (
        status,
        Json(ErrorBody {
            error,
            request_id: request_id.to_owned(),
        }),
    )
        .into_response()
}
