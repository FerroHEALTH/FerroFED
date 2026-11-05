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

use crate::auth::caller::Caller;
use crate::face_path::{FacePath, classify};
use crate::state::AppState;

use super::outcome;
use super::trc::{self, Service};

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

/// What the layer reads of an `OperationOutcome` the face answered.
#[derive(serde::Deserialize)]
struct Outcome {
    /// Its issues.
    #[serde(default)]
    issue: Vec<Issue>,
}

/// What the layer reads of one issue: its FHIR R4 `issue-type`.
#[derive(serde::Deserialize)]
struct Issue {
    /// The issue type.
    code: String,
}

/// The MyHealth@EU service of a patient-data request to `path` with the
/// query `query`, or `None` for a path of the face that carries no patient
/// data, the identification lookup among them.
fn service(face: FacePath, query: Option<&str>) -> Option<Service> {
    match face {
        FacePath::Summary | FacePath::Binary => Some(Service::PatientSummary),
        FacePath::Documents => Some(
            match ferrofed_eehrxf::list::ListRequest::from_query(query) {
                Ok(read) if !read.admits_summaries() => Service::OriginalDocument,
                _ => Service::PatientSummary,
            },
        ),
        FacePath::Medication(_) => Some(Service::Prescription),
        FacePath::Metadata | FacePath::Patients | FacePath::Bundle | FacePath::Other => None,
    }
}

/// The layer that rewrites every error the gateway answers under
/// `{fhir-base}` with its own error body as an `OperationOutcome`, keeping
/// its status and its headers.
///
/// An answer that is already FHIR JSON, a success, or an answer outside the
/// face passes unchanged, but for one case. To a national contact point,
/// every `404` `not-found` on a path that carries patient data is the one
/// no-data answer of the treatment relationship binding for the path's
/// service, so an unknown patient, one no member may disclose and one
/// outside the treatment relationship confirmation answer alike (eHDSI XCA
/// Profile 8.0.1 §4.2.2).
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
    let face = state.fhir().and_then(|fhir| classify(fhir, &path));
    let under = face.is_some();
    let relayed = request
        .extensions()
        .get::<Caller>()
        .is_some_and(Caller::is_contact_point);
    let no_data = face
        .filter(|_| relayed)
        .and_then(|face| service(face, request.uri().query()));
    let head = request.method() == Method::HEAD;
    let response = next.run(request).await;
    if let Some(service) = no_data
        && response.status() == StatusCode::NOT_FOUND
    {
        return not_found(response, service).await;
    }
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

/// `response`, a `404` to a national contact point on a path of `service`,
/// as [`trc::no_data`] when its issue is `not-found`, the service its
/// answer names kept and the access record it carries with it; any other
/// `404` is passed as it came.
async fn not_found(response: Response, service: Service) -> Response {
    let service = response
        .extensions()
        .get::<Service>()
        .copied()
        .unwrap_or(service);
    let (parts, body) = response.into_parts();
    let no_data = |extensions: http::Extensions| {
        let mut answer = trc::no_data(service);
        *answer.extensions_mut() = extensions;
        answer.extensions_mut().insert(service);
        answer
    };
    let Ok(bytes) = axum::body::to_bytes(body, ERROR_BODY_LIMIT).await else {
        return no_data(parts.extensions);
    };
    // NOTE: FHIR R4 `issue-type`: a `not-found` says the data are not there, which a
    // contact point is told as no data; a `not-supported` category stays as it was.
    let not_found = serde_json::from_slice::<Outcome>(&bytes)
        .is_ok_and(|outcome| outcome.issue.iter().any(|issue| issue.code == "not-found"));
    if not_found {
        no_data(parts.extensions)
    } else {
        Response::from_parts(parts, axum::body::Body::from(bytes))
    }
}
