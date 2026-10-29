// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The layer that answers every error under `{fhir-base}` as an
//! `OperationOutcome`.

use std::sync::Arc;

use axum::extract::{OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use ferrofed_eehrxf::face::FHIR_JSON;
use http::{Method, StatusCode, header};

use crate::state::AppState;

use super::{classify, outcome};

/// The largest error body the face rewrites as an `OperationOutcome`; the
/// gateway's own error bodies are far smaller.
const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// The FHIR R4 `issue-type` of an error answered with `status`.
fn issue_type(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "invalid",
        StatusCode::UNAUTHORIZED => "login",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not-found",
        StatusCode::METHOD_NOT_ALLOWED
        | StatusCode::NOT_ACCEPTABLE
        | StatusCode::UNSUPPORTED_MEDIA_TYPE
        | StatusCode::NOT_IMPLEMENTED => "not-supported",
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => "timeout",
        StatusCode::CONFLICT => "conflict",
        StatusCode::FAILED_DEPENDENCY => "incomplete",
        StatusCode::TOO_MANY_REQUESTS => "throttled",
        StatusCode::SERVICE_UNAVAILABLE => "transient",
        _ => "exception",
    }
}

/// The `OperationOutcome` of a refusal of `status` the stack writes under
/// `{fhir-base}`, with `message` as its diagnostics.
pub(crate) fn refused(status: StatusCode, message: &str) -> Response {
    outcome(status, issue_type(status), message)
}

/// The gateway's problem details body, as far as the outcome reads it.
#[derive(serde::Deserialize)]
struct GatewayError {
    /// The problem's `detail` (RFC 9457 §3.1.4).
    #[serde(rename = "detail")]
    message: String,
    /// The stable code.
    code: Option<String>,
}

/// The layer that rewrites every error the gateway answers under
/// `{fhir-base}` with its own error body as an `OperationOutcome`, keeping
/// its status and its headers.
///
/// An answer that is already FHIR JSON, a success, or an answer outside the
/// face passes unchanged.
pub async fn outcomes(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| request.uri().path(), |original| original.path())
        .to_owned();
    let under = state
        .fhir()
        .is_some_and(|fhir| classify(fhir, &path).is_some());
    let head = request.method() == Method::HEAD;
    let response = next.run(request).await;
    if !under
        || head
        || !response.status().is_client_error() && !response.status().is_server_error()
    {
        return response;
    }
    let is_fhir = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with(FHIR_JSON));
    if is_fhir {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let read = axum::body::to_bytes(body, ERROR_BODY_LIMIT).await;
    let message = read
        .ok()
        .and_then(|bytes| serde_json::from_slice::<GatewayError>(&bytes).ok())
        .map_or_else(
            || String::from("the request was refused"),
            |error| match error.code {
                Some(code) => format!("{code}: {}", error.message),
                None => error.message,
            },
        );
    let rewritten = outcome(parts.status, issue_type(parts.status), &message);
    let (rewritten_parts, rewritten_body) = rewritten.into_parts();
    parts.headers.remove(header::CONTENT_LENGTH);
    for (name, value) in &rewritten_parts.headers {
        parts.headers.insert(name.clone(), value.clone());
    }
    Response::from_parts(parts, rewritten_body)
}
