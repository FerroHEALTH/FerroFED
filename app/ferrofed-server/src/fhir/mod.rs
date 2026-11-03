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
//! documents §3.3.4 names (Annex II 2.2 and 2.3), and
//! `GET {fhir-base}/DocumentReference` lists the summaries that exist for a
//! patient ([`ferrofed_eehrxf::list`]), and `GET {fhir-base}/Patient` looks a
//! patient up by identifier ([`ferrofed_eehrxf::lookup`]). `MedicationRequest` and
//! `MedicationDispense` answer a category no member is declared for as not
//! served ([`medication`]). Every other path under the base is a
//! `404`. A summary runs the gateway's own section queries over
//! the members (`facade::summary`), maps each composition through the
//! deployment's FHIRconnect mappings and writes the document
//! ([`ferrofed_eehrxf::summary`]); a received document is written to the
//! one member its category is declared for (`facade::receive`). The face
//! sits beside `{base}`, so no ITS-REST path changes (Federation Tier §4.1,
//! N28, N1).
//!
//! Every request is authenticated at the same gate as the ITS-REST face
//! (§13.1, N25), and every summary, list or document that reached a member
//! is recorded in the access log, naming the verified caller (Annex II 3.2).
//! Every error the face answers is an `OperationOutcome` ([`errors::outcomes`]): the gateway's own
//! refusals keep their status and their headers, and the error body is
//! rewritten as the outcome. No specification governs the mapping of the
//! gateway's errors to issue types: our own design, over the FHIR R4
//! `issue-type` codes.

use std::ops::ControlFlow;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get, post};
use ferrofed_eehrxf::face::{self, FHIR_JSON, SummaryRequest};
use ferrofed_eehrxf::receive::Receivers;
use ferrofed_eehrxf::serving::Serving;
use ferrohealth_smart::openehr::smart_scopes::{Permission, ResourceFamily};
use http::{HeaderValue, Method, StatusCode, Uri, header};
use serde::Serialize;

use crate::access::NoAccess;
use crate::auth::permission::{Requirement, Resource};
use crate::config::fhir::FhirSettings;
use crate::fhir::admission::Asked;
use crate::state::AppState;

mod admission;
mod binary;
pub mod errors;
mod list;
pub mod locator;
mod lookup;
pub mod medication;
mod patient_summary;

/// The path of the summary operation under `{fhir-base}`.
pub const SUMMARY: &str = "/Patient/$summary";

/// The path of the capability statement under `{fhir-base}`.
pub const METADATA: &str = "/metadata";

/// The path a document in the exchange format is received at under
/// `{fhir-base}`, the `Bundle` end-point of FHIR R4 documents §3.3.4.
pub const BUNDLE: &str = "/Bundle";

/// The path of the document list under `{fhir-base}`, the
/// `DocumentReference` search-type interaction.
pub const DOCUMENTS: &str = "/DocumentReference";

/// The path of the patient lookup under `{fhir-base}`, the `Patient`
/// search-type interaction.
pub const PATIENTS: &str = "/Patient";

/// The product name the document's author `Device` and the capability
/// statement carry.
const PRODUCT: &str = "FerroFED";

/// The text of the `404` every other path answers, naming each path
/// [`table`] routes.
static UNSERVED: LazyLock<String> = LazyLock::new(|| {
    let paths: Vec<&str> = table()
        .iter()
        .map(|(path, _)| path.trim_start_matches('/'))
        .collect();
    format!("the FHIR face serves {} alone", paths.join(", "))
});

/// Every path of the face with its handlers, the one list the router is
/// built from and the `404` of any other path names.
fn table() -> [(&'static str, MethodRouter<Arc<AppState>>); 12] {
    [
        (METADATA, get(metadata)),
        (PATIENTS, get(lookup::patients)),
        (SUMMARY, get(summary_get).post(summary_post)),
        (BUNDLE, post(bundle_post)),
        (DOCUMENTS, get(list::documents)),
        (binary::ROUTE, get(binary::read)),
        (medication::PRESCRIPTIONS, get(medication::search::search)),
        (medication::PRESCRIPTION, get(medication::read::read)),
        (
            medication::PRESCRIPTION_VERSION,
            get(medication::read::vread),
        ),
        (
            medication::DISPENSATIONS,
            post(medication::dispense::dispense),
        ),
        (
            medication::DISPENSATION,
            get(medication::read::read_dispensation).put(medication::withdraw::withdraw),
        ),
        (
            medication::DISPENSATION_VERSION,
            get(medication::read::vread_dispensation),
        ),
    ]
}

/// The routes of the face, which the router nests under `{fhir-base}`.
pub fn routes() -> Router<Arc<AppState>> {
    table()
        .into_iter()
        .fold(Router::new(), |router, (path, handlers)| {
            router.route(path, handlers)
        })
        // NOTE: no specification governs this: our own design; every other path under
        // `{fhir-base}` is a `404` naming the paths the face serves, never the path asked for.
        .fallback(|| async { outcome(StatusCode::NOT_FOUND, "not-found", &UNSERVED) })
}

/// What a path under the face addresses, as the gate reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacePath {
    /// `{fhir-base}/metadata`, exactly.
    Metadata,
    /// `{fhir-base}/Patient`, exactly.
    Patients,
    /// `{fhir-base}/Patient/$summary`, exactly.
    Summary,
    /// `{fhir-base}/Bundle`, exactly.
    Bundle,
    /// `{fhir-base}/DocumentReference`, exactly.
    Documents,
    /// `{fhir-base}/Binary/{token}`, its token one segment of the compact
    /// JWS alphabet.
    Binary,
    /// A path of the electronic prescriptions and dispensations, exactly.
    Medication(medication::Addressed),
    /// Any other path under the base, a variant of a path the face's route
    /// table (`table`) names included: another letter case, a
    /// percent-encoded character, a trailing or a doubled slash.
    Other,
}

/// What `path`, the raw request path, addresses under the face `fhir`
/// serves, or `None` for a path outside it.
///
/// A path is under the face when its letters, ASCII case ignored, start
/// with the base, so a variant of the base is never read as a path outside
/// the face; only the paths of the face's route table (`table`) are named,
/// and every other one is [`FacePath::Other`], which the gate refuses.
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
    } else if path == fhir.base.join(PATIENTS) {
        FacePath::Patients
    } else if path == fhir.base.join(SUMMARY) {
        FacePath::Summary
    } else if path == fhir.base.join(BUNDLE) {
        FacePath::Bundle
    } else if path == fhir.base.join(DOCUMENTS) {
        FacePath::Documents
    } else if is_binary(fhir, path) {
        FacePath::Binary
    } else if let Some(addressed) = medication::addressed(fhir, path) {
        FacePath::Medication(addressed)
    } else {
        FacePath::Other
    })
}

/// Whether `path` is `{fhir-base}/Binary/{token}`, the token one segment
/// of the compact JWS alphabet (RFC 7515 §7.1).
fn is_binary(fhir: &FhirSettings, path: &str) -> bool {
    path.strip_prefix(fhir.base.join(binary::BINARY).as_str())
        .is_some_and(|token| {
            !token.is_empty()
                && token
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
}

/// Returns what a request of `method` to `face` requires at the gate.
///
/// The capability statement takes a verified caller, a `GET` of the patient
/// lookup the `composition-` read of every template, a `GET` or `POST` of a
/// summary, a `GET` of the document list or of a listed document the `aql-` search of every
/// section query, a `POST` of a document
/// the `composition-` create of every template, a prescription or
/// dispensation path what [`medication::requirement`] says, and every other
/// method and path is refused, so no variant of a path is let through on a
/// weaker requirement.
#[must_use]
pub fn requirement(face: FacePath, method: &Method) -> Requirement {
    match face {
        FacePath::Medication(addressed) => medication::requirement(addressed, method),
        FacePath::Metadata if matches!(*method, Method::GET | Method::HEAD) => Requirement::Caller,
        // NOTE: no specification governs the scope of the FHIR face: our own design; a
        // summary runs the section queries, so it takes the `aql-` search of each.
        FacePath::Summary if matches!(*method, Method::GET | Method::POST) => Requirement::Scope {
            family: ResourceFamily::Aql,
            permission: Permission::Search,
            resource: Resource::Sections,
        },
        // NOTE: no specification governs this: our own design; the list names the summary
        // and carries the members' own ones, so it takes what a summary takes.
        FacePath::Documents | FacePath::Binary if *method == Method::GET => Requirement::Scope {
            family: ResourceFamily::Aql,
            permission: Permission::Search,
            resource: Resource::Sections,
        },
        // NOTE: SMART on openEHR master08 §Resource Scopes; the lookup finds whether members hold
        // the patient's EHRs, as `GET /ehr?subject_id=` does, so it takes what that read takes.
        FacePath::Patients if *method == Method::GET => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Read,
            resource: Resource::Unnamed,
        },
        // NOTE: SMART on openEHR master08 §Resource Scopes; a received document is written as
        // a composition whose template the body decides, so only a create of every one covers it.
        FacePath::Bundle if *method == Method::POST => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Create,
            resource: Resource::Unnamed,
        },
        FacePath::Metadata
        | FacePath::Patients
        | FacePath::Summary
        | FacePath::Bundle
        | FacePath::Documents
        | FacePath::Binary
        | FacePath::Other => Requirement::Refused,
    }
}

/// `GET {fhir-base}/metadata`: the face's `CapabilityStatement`, with the
/// categories the deployment receives documents of and those it serves.
async fn metadata(State(state): State<Arc<AppState>>) -> Response {
    let date = jiff::Timestamp::now().strftime("%Y-%m-%d").to_string();
    let (no_receivers, no_serving) = (Receivers::default(), Serving::default());
    let receivers = state
        .fhir()
        .map_or(&no_receivers, |fhir| fhir.receivers.as_ref());
    let serving = state
        .fhir()
        .map_or(&no_serving, |fhir| fhir.serving.as_ref());
    fhir_json(
        StatusCode::OK,
        &face::capability(
            PRODUCT,
            env!("CARGO_PKG_VERSION"),
            &date,
            (receivers, serving),
        ),
    )
}

/// `GET {fhir-base}/Patient/$summary`.
async fn summary_get(asked: Asked, uri: Uri) -> Response {
    let started = Instant::now();
    let read = SummaryRequest::from_query(uri.query());
    patient_summary::summarised((&asked, started), read).await
}

/// `POST {fhir-base}/Patient/$summary` with a `Parameters` body.
async fn summary_post(asked: Asked, body: Bytes) -> Response {
    let started = Instant::now();
    if !fhir_json_body(&asked.headers) {
        return outcome(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not-supported",
            "a Parameters body is sent as application/fhir+json",
        );
    }
    let read = SummaryRequest::from_parameters(&body);
    patient_summary::summarised((&asked, started), read).await
}

/// `POST {fhir-base}/Bundle` with a document in the exchange format: the
/// document written to the member that receives its category
/// (`facade::receive`).
async fn bundle_post(asked: Asked, body: Bytes) -> Response {
    let started = Instant::now();
    if !fhir_json_body(&asked.headers) {
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
    admission::served(
        (&asked, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to create a composition",
            )
        },
        |_| ControlFlow::Continue(()),
        async |admitted, arrived, ()| {
            crate::facade::receive::receive(
                &admitted.federation,
                admitted.fhir,
                (arrived, admitted.caller),
                text,
            )
            .await
        },
    )
    .await
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

/// The text of the `OperationOutcome` a caller the gate did not admit to the
/// section queries a summary, a list or a listed document runs over is
/// answered `403` with.
const UNADMITTED: &str = "the caller was not admitted to the section queries";

/// The `OperationOutcome` of one error, answered with `status`, stating that
/// no member was sent anything.
fn refusal(status: StatusCode, code: &str, diagnostics: &str) -> Response {
    let mut response = outcome(status, code, diagnostics);
    response.extensions_mut().insert(NoAccess);
    response
}

/// The answer to a search of the face `asked`, begun at `started`, whose
/// query read as `read`.
///
/// A query the face cannot read is `400`, stating that no member was sent
/// anything; a caller the gate did not admit is `403` with `unadmitted` as
/// its text; a search `held` breaks off is answered with its answer before
/// anything is conveyed; every other is `serve`'s.
async fn searched<R, E: std::fmt::Display>(
    (asked, started): (&Asked, Instant),
    read: Result<R, E>,
    unadmitted: &str,
    held: impl FnOnce(&R) -> ControlFlow<Response>,
    serve: impl AsyncFnOnce(
        &R,
        &admission::Admitted<'_>,
        crate::facade::request::Arrived<'_>,
    ) -> Response,
) -> Response {
    let read = match read {
        Ok(read) => read,
        Err(refusal) => {
            let mut response = outcome(StatusCode::BAD_REQUEST, "invalid", &refusal.to_string());
            response.extensions_mut().insert(NoAccess);
            return response;
        }
    };
    admission::served(
        (asked, started),
        || outcome(StatusCode::FORBIDDEN, "forbidden", unadmitted),
        |_| held(&read),
        async |admitted, arrived, ()| serve(&read, admitted, arrived).await,
    )
    .await
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

#[cfg(test)]
mod tests {
    use super::UNSERVED;

    #[test]
    fn the_404_names_every_path_the_router_serves() {
        assert_eq!(
            UNSERVED.as_str(),
            "the FHIR face serves metadata, Patient, Patient/$summary, Bundle, DocumentReference, \
             Binary/{token}, MedicationRequest, MedicationRequest/{id}, \
             MedicationRequest/{id}/_history/{vid}, MedicationDispense, MedicationDispense/{id}, \
             MedicationDispense/{id}/_history/{vid} alone"
        );
    }
}
