// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 face of the European exchange format, served on its own base,
//! `{fhir-base}` (Regulation (EU) 2025/327 Annex II 2.1).
//!
//! `GET {fhir-base}/metadata` answers the face's `CapabilityStatement`, and
//! `GET` or `POST {fhir-base}/Patient/$summary` the patient summary as an
//! HL7 Europe Patient Summary document `Bundle`, by the International Patient
//! Summary 2.0.0 `$summary` operation. `POST {fhir-base}/Bundle` receives a
//! document in the exchange format, at the `Bundle` end-point FHIR R4
//! documents §3.3.4 names (Annex II 2.2 and 2.3). Every other path under the
//! base is a `404`. A summary runs the gateway's own section queries over
//! the members (`facade::summary`), maps each composition through the
//! deployment's FHIRconnect mappings and writes the document
//! ([`ferrofed_eehrxf::summary`]); a received document is written to the
//! one member its category is declared for (`facade::receive`). The face
//! sits beside `{base}`, so no ITS-REST path changes (Federation Tier §4.1,
//! N28, N1).
//!
//! Every request is authenticated at the same gate as the ITS-REST face
//! (§13.1, N25), and every summary or document that reached a member is
//! recorded in the access log, naming the verified caller (Annex II 3.2). Every error the
//! face answers is an `OperationOutcome` ([`outcomes`]): the gateway's own
//! refusals keep their status and their headers, and the error body is
//! rewritten as the outcome. No specification governs the mapping of the
//! gateway's errors to issue types: our own design, over the FHIR R4
//! `issue-type` codes.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use ferrofed_eehrxf::face::{self, FHIR_JSON, Refusal, SummaryRequest};
use ferrofed_eehrxf::summary::{self, Author, Request as SummaryDocument};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::header::PatientHeader;
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use http::{HeaderValue, Method, StatusCode, Uri, header};
use openehr_sdt::smart_scopes::{Permission, ResourceFamily};
use secrecy::SecretString;
use serde::Serialize;

use crate::access::NoAccess;
use crate::auth::caller::Caller;
use crate::auth::permission::{Requirement, Resource};
use crate::config::fhir::FhirSettings;
use crate::conveyed;
use crate::error::Code;
use crate::facade::demographics::{self, Unheaded};
use crate::facade::request::Arrived;
use crate::facade::summary::{self as gathered, Unsummarised};
use crate::request_id;
use crate::state::AppState;

/// The path of the summary operation under `{fhir-base}`.
pub const SUMMARY: &str = "/Patient/$summary";

/// The path of the capability statement under `{fhir-base}`.
pub const METADATA: &str = "/metadata";

/// The path a document in the exchange format is received at under
/// `{fhir-base}`, the `Bundle` end-point of FHIR R4 documents §3.3.4.
pub const BUNDLE: &str = "/Bundle";

/// The product name the document's author `Device` and the capability
/// statement carry.
const PRODUCT: &str = "FerroFED";

/// The largest error body the face rewrites as an `OperationOutcome`; the
/// gateway's own error bodies are far smaller.
const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// The routes of the face, which the router nests under `{fhir-base}`.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(METADATA, get(metadata))
        .route(SUMMARY, get(summary_get).post(summary_post))
        .route(BUNDLE, post(bundle_post))
        .fallback(unserved)
}

/// What a path under the face addresses, as the gate reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacePath {
    /// `{fhir-base}/metadata`, exactly.
    Metadata,
    /// `{fhir-base}/Patient/$summary`, exactly.
    Summary,
    /// `{fhir-base}/Bundle`, exactly.
    Bundle,
    /// Any other path under the base, a variant of the three above included:
    /// another letter case, a percent-encoded character, a trailing or a
    /// doubled slash.
    Other,
}

/// What `path`, the raw request path, addresses under the face `fhir`
/// serves, or `None` for a path outside it.
///
/// A path is under the face when its letters, ASCII case ignored, start
/// with the base, so a variant of the base is never read as a path outside
/// the face; only the three exact paths are named, and every other one is
/// [`FacePath::Other`], which the gate refuses.
#[must_use]
pub fn classify(fhir: &FhirSettings, path: &str) -> Option<FacePath> {
    let base = fhir.base.as_str().to_ascii_lowercase();
    let folded = path.to_ascii_lowercase();
    let under = folded == base
        || folded
            .strip_prefix(base.as_str())
            .is_some_and(|rest| rest.starts_with('/') || rest.starts_with('%'));
    if !under {
        return None;
    }
    Some(if path == fhir.base.join(METADATA) {
        FacePath::Metadata
    } else if path == fhir.base.join(SUMMARY) {
        FacePath::Summary
    } else if path == fhir.base.join(BUNDLE) {
        FacePath::Bundle
    } else {
        FacePath::Other
    })
}

/// Returns what a request of `method` to `face` requires at the gate.
///
/// The capability statement takes a verified caller, a `GET` or `POST` of a
/// summary the `aql-` search of every section query, a `POST` of a document
/// the `composition-` create of every template, and every other method and
/// path is refused, so no variant of a path is let through on a weaker
/// requirement.
#[must_use]
pub fn requirement(face: FacePath, method: &Method) -> Requirement {
    match face {
        FacePath::Metadata if matches!(*method, Method::GET | Method::HEAD) => Requirement::Caller,
        // NOTE: no specification governs the scope of the FHIR face: our own design; a
        // summary runs the section queries, so it takes the `aql-` search of each.
        FacePath::Summary if matches!(*method, Method::GET | Method::POST) => Requirement::Scope {
            family: ResourceFamily::Aql,
            permission: Permission::Search,
            resource: Resource::Sections,
        },
        // NOTE: SMART on openEHR master08 §Resource Scopes; a received document is written as
        // a composition whose template the body decides, so only a create of every one covers it.
        FacePath::Bundle if *method == Method::POST => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Create,
            resource: Resource::Unnamed,
        },
        FacePath::Metadata | FacePath::Summary | FacePath::Bundle | FacePath::Other => {
            Requirement::Refused
        }
    }
}

/// `GET {fhir-base}/metadata`: the face's `CapabilityStatement`.
async fn metadata() -> Response {
    let date = jiff::Timestamp::now().strftime("%Y-%m-%d").to_string();
    fhir_json(
        StatusCode::OK,
        &face::capability(PRODUCT, env!("CARGO_PKG_VERSION"), &date),
    )
}

/// Every other path under `{fhir-base}`: a `404` naming no path.
async fn unserved() -> Response {
    outcome(
        StatusCode::NOT_FOUND,
        "not-found",
        "the FHIR face serves metadata and Patient/$summary alone",
    )
}

/// `GET {fhir-base}/Patient/$summary`.
async fn summary_get(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    uri: Uri,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    let read = SummaryRequest::from_query(uri.query());
    summarised(&state, (outbound, caller), (&headers, started), read).await
}

/// `POST {fhir-base}/Patient/$summary` with a `Parameters` body.
async fn summary_post(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    headers: http::HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    let media = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !matches!(media, Some(FHIR_JSON | "application/json")) {
        return outcome(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not-supported",
            "a Parameters body is sent as application/fhir+json",
        );
    }
    let read = SummaryRequest::from_parameters(&body);
    summarised(&state, (outbound, caller), (&headers, started), read).await
}

/// `POST {fhir-base}/Bundle` with a document in the exchange format: the
/// document written to the member that receives its category
/// (`facade::receive`).
async fn bundle_post(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    headers: http::HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    if !fhir_json_body(&headers) {
        return refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not-supported",
            "a document is sent as application/fhir+json",
        );
    }
    let Ok(text) = std::str::from_utf8(&body) else {
        return refusal(
            StatusCode::BAD_REQUEST,
            "invalid",
            "the document is not UTF-8 text (RFC 8259 §8.1)",
        );
    };
    let outbound = outbound.map_or_else(OutboundId::mint, |Extension(id)| id);
    let request_id = request_id::of(&headers).unwrap_or_default();
    let logged = outbound.to_string();
    let (Some(federation), Some(fhir)) = (state.federation(), state.fhir()) else {
        return crate::error::fixed(Code::NotImplemented, request_id);
    };
    let caller = caller.as_deref();
    // NOTE: §13.1, N25: a document is written only for a caller the gate admitted with the
    // scopes that cover it, whatever path reached this handler.
    if caller.is_none_or(|caller| caller.covering().is_empty()) {
        return refusal(
            StatusCode::FORBIDDEN,
            "forbidden",
            "the caller was not admitted to create a composition",
        );
    }
    let conveyance = match conveyed::of(&federation, caller) {
        Ok(conveyance) => conveyance,
        Err(unconveyed) => return unconveyed.respond(request_id, &logged),
    };
    let Some(on_behalf) = caller.map(Caller::on_behalf) else {
        return conveyed::Unconveyed::NoCaller.respond(request_id, &logged);
    };
    let conveyance =
        match crate::facade::confined_by(&federation, caller, started, conveyance).await {
            Ok(conveyance) => conveyance,
            Err(unconfined) => return unconfined.respond(request_id, &logged),
        };
    let session = caller.map(Caller::session);
    let arrived = Arrived {
        headers: &headers,
        request_id,
        outbound,
        conveyance: &conveyance,
        started,
        session: session.as_ref(),
        requester: caller.and_then(Caller::requester),
        on_behalf: &on_behalf,
    };
    crate::facade::receive::receive(&federation, fhir, arrived, text).await
}

/// Whether `headers` declare a FHIR JSON body.
fn fhir_json_body(headers: &http::HeaderMap) -> bool {
    let media = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    matches!(media, Some(FHIR_JSON | "application/json"))
}

/// The `OperationOutcome` of one error, answered with `status`, stating that
/// no member was sent anything.
fn refusal(status: StatusCode, code: &str, diagnostics: &str) -> Response {
    let mut response = outcome(status, code, diagnostics);
    response.extensions_mut().insert(NoAccess);
    response
}

/// The answer to a summary request `read` from the request's caller and
/// headers.
async fn summarised(
    state: &AppState,
    (outbound, caller): (Option<Extension<OutboundId>>, Option<Extension<Caller>>),
    (headers, started): (&http::HeaderMap, Instant),
    read: Result<SummaryRequest, Refusal>,
) -> Response {
    let read = match read {
        Ok(read) => read,
        Err(refusal) => {
            return outcome(StatusCode::BAD_REQUEST, "invalid", &refusal.to_string());
        }
    };
    let outbound = outbound.map_or_else(OutboundId::mint, |Extension(id)| id);
    let request_id = request_id::of(headers).unwrap_or_default();
    let logged = outbound.to_string();
    let (Some(federation), Some(fhir)) = (state.federation(), state.fhir()) else {
        return crate::error::fixed(Code::NotImplemented, request_id);
    };
    let caller = caller.as_deref();
    // NOTE: §13.1, N25: the summary runs only for a caller the gate admitted with the scopes
    // that cover it, whatever path reached this handler.
    if caller.is_none_or(|caller| caller.covering().is_empty()) {
        return outcome(
            StatusCode::FORBIDDEN,
            "forbidden",
            "the caller was not admitted to the section queries",
        );
    }
    let conveyance = match conveyed::of(&federation, caller) {
        Ok(conveyance) => conveyance,
        Err(unconveyed) => return unconveyed.respond(request_id, &logged),
    };
    let Some(on_behalf) = caller.map(Caller::on_behalf) else {
        return conveyed::Unconveyed::NoCaller.respond(request_id, &logged);
    };
    let conveyance =
        match crate::facade::confined_by(&federation, caller, started, conveyance).await {
            Ok(conveyance) => conveyance,
            Err(unconfined) => return unconfined.respond(request_id, &logged),
        };
    let session = caller.map(Caller::session);
    let arrived = Arrived {
        headers,
        request_id,
        outbound,
        conveyance: &conveyance,
        started,
        session: session.as_ref(),
        requester: caller.and_then(Caller::requester),
        on_behalf: &on_behalf,
    };
    let patient = (read.system(), read.value());
    // NOTE: eHN PS A.1.1, A.1.2; the header comes from the identity binding before any member
    // is asked, so no member is reached for a summary that could name no patient.
    let header = match headed(&federation, (patient, &on_behalf), started).await {
        Ok(header) => header,
        Err(unheaded) => {
            let mut response = unheaded_outcome(unheaded);
            response.extensions_mut().insert(NoAccess);
            return response;
        }
    };
    match gathered::gather(Arc::clone(&federation), arrived, patient).await {
        Ok(gathered) => document(&federation, fhir, &gathered, (patient, header), &logged),
        Err(unsummarised) => refused(&federation, unsummarised, patient, (request_id, outbound)),
    }
}

/// The summary header the identity binding holds of the patient
/// `system`|`value`, asked on behalf of `on_behalf` within the budget of a
/// request that started at `started`, or the [`Unheaded`] that says why
/// there is none.
async fn headed(
    federation: &crate::federation::Federation,
    ((system, value), on_behalf): ((&str, &str), &OnBehalfOf),
    started: Instant,
) -> Result<PatientHeader, Unheaded> {
    // NOTE: the request reader refuses an empty system or value, so a reference that cannot
    // be built names no identifier the binding is asked about.
    let patient = IdentifierNamespace::new(system)
        .and_then(|namespace| PatientRef::new(namespace, SecretString::from(value)))
        .map_err(|_empty| Unheaded::NoBinding)?;
    let deadline = started
        .checked_add(federation.budget().overall())
        .unwrap_or(started);
    demographics::header(federation, (&patient, on_behalf), deadline).await
}

/// The `OperationOutcome` of a summary request whose header `unheaded` says
/// why the identity binding gives none.
fn unheaded_outcome(unheaded: Unheaded) -> Response {
    match unheaded {
        Unheaded::NoBinding => outcome(
            StatusCode::UNPROCESSABLE_ENTITY,
            "not-supported",
            "the demographics binding is not asked about identifiers of this system, so no summary header can be written",
        ),
        // NOTE: Federation Tier §11.3; a patient the binding does not know answers as one
        // no member holds, so the outcome names neither.
        Unheaded::NoMatch => outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "no member holds a patient summary for this identifier",
        ),
        Unheaded::Ambiguous(reason) => outcome(
            StatusCode::UNPROCESSABLE_ENTITY,
            "multiple-matches",
            &format!("no summary header can be written: {reason}"),
        ),
        Unheaded::Unnamed => outcome(
            StatusCode::UNPROCESSABLE_ENTITY,
            "required",
            "the demographics binding holds no name for this patient, which the HL7 Europe Patient Summary Patient requires (ips-pat-1)",
        ),
        Unheaded::Unavailable { failure, timed_out } => {
            let (status, code) = if timed_out {
                (StatusCode::GATEWAY_TIMEOUT, "timeout")
            } else {
                (StatusCode::BAD_GATEWAY, "exception")
            };
            outcome(status, code, &format!("no summary header: {failure}"))
        }
    }
}

/// The document `gathered` makes about the patient the identity binding
/// describes in `header`, answered with the access it records.
fn document(
    federation: &crate::federation::Federation,
    fhir: &FhirSettings,
    gathered: &gathered::Gathered,
    ((system, value), header): ((&str, &str), PatientHeader),
    logged: &str,
) -> Response {
    let request = SummaryDocument {
        base: fhir.absolute.clone(),
        identifier: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
        patient_id: uuid::Uuid::new_v4().to_string(),
        timestamp: jiff::Timestamp::now()
            .strftime("%Y-%m-%dT%H:%M:%SZ")
            .to_string(),
        system: system.to_owned(),
        value: value.to_owned(),
        header,
    };
    let author = Author {
        product: String::from(PRODUCT),
        version: String::from(env!("CARGO_PKG_VERSION")),
        operator: fhir.operator.clone(),
    };
    let assembled = summary::assemble(
        &request,
        &author,
        (&gathered.origins, &gathered.sections),
        &fhir.mappings,
    );
    let (mut response, delivered) = match assembled {
        Ok(assembled) => (
            fhir_json(StatusCode::OK, &assembled.bundle),
            assembled.mapped,
        ),
        Err(failure) => {
            tracing::error!(
                error = %crate::chain(&failure),
                request_id = logged,
                "the patient summary could not be written"
            );
            (
                outcome(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "exception",
                    "the patient summary could not be written from what the members answered",
                ),
                BTreeSet::new(),
            )
        }
    };
    attach(
        &mut response,
        gathered::accessed(federation, gathered, (system, value), &delivered),
    );
    response
}

/// The answer to a summary the section queries did not give, `unsummarised`.
fn refused(
    federation: &crate::federation::Federation,
    unsummarised: Unsummarised,
    patient: (&str, &str),
    (request_id, outbound): (&str, OutboundId),
) -> Response {
    match unsummarised {
        Unsummarised::Failure(failure) => failure.respond(request_id, outbound),
        Unsummarised::Internal(what) => {
            tracing::error!(request_id = %outbound, what, "the section queries could not be run");
            crate::error::fixed(Code::Internal, request_id)
        }
        // NOTE: Federation Tier §11.3, Regulation (EU) 2025/327 Art 8: an unknown patient and
        // one no member may disclose answer alike, so the outcome names neither.
        Unsummarised::NotFound => {
            let mut response = outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                "no member holds a patient summary for this identifier",
            );
            attach(&mut response, None);
            response
        }
        Unsummarised::Incomplete {
            status,
            silent,
            gathered,
        } => {
            let named = silent
                .iter()
                .map(|(endpoint, status)| format!("{endpoint} ({status})"))
                .collect::<Vec<_>>()
                .join(", ");
            let code = if status == StatusCode::GATEWAY_TIMEOUT {
                "timeout"
            } else {
                "incomplete"
            };
            let diagnostics = if named.is_empty() {
                String::from(
                    "the summary is incomplete: the patient could not be resolved at every member (§11.3)",
                )
            } else {
                format!("the summary is incomplete: no answer from {named} (§11.3)")
            };
            let mut response = outcome(status, code, &diagnostics);
            attach(
                &mut response,
                gathered::accessed(federation, &gathered, patient, &BTreeSet::new()),
            );
            response
        }
    }
}

/// Attaches `accessed` to `response` for the access log, or the statement
/// that no member was sent anything.
fn attach(response: &mut Response, accessed: Option<crate::access::Accessed>) {
    if let Some(accessed) = accessed {
        response.extensions_mut().insert(accessed);
    } else {
        response.extensions_mut().insert(NoAccess);
    }
}

/// `resource` as FHIR JSON with `status`.
pub(crate) fn fhir_json(status: StatusCode, resource: &impl Serialize) -> Response {
    match serde_json::to_vec(resource) {
        Ok(body) => (
            status,
            [(header::CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON))],
            body,
        )
            .into_response(),
        Err(_unencoded) => (StatusCode::INTERNAL_SERVER_ERROR, Body::empty()).into_response(),
    }
}

/// The `OperationOutcome` of one error of the FHIR R4 issue type `code`,
/// with `status`.
fn outcome(status: StatusCode, code: &str, diagnostics: &str) -> Response {
    fhir_json(status, &face::outcome(code, diagnostics))
}

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

/// The gateway's error body, as far as the outcome reads it.
#[derive(serde::Deserialize)]
struct GatewayError {
    /// The message.
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
