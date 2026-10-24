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
//! patient ([`ferrofed_eehrxf::list`]). `MedicationRequest` and
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

use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use axum::Extension;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get, post};
use ferrofed_eehrxf::face::{self, FHIR_JSON, Refusal, SummaryRequest};
use ferrofed_eehrxf::receive::Receivers;
use ferrofed_eehrxf::serving::Serving;
use ferrofed_eehrxf::summary::{self, Author};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::header::PatientHeader;
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrohealth_smart::openehr::smart_scopes::{Permission, ResourceFamily};
use http::{HeaderValue, Method, StatusCode, Uri, header};
use secrecy::SecretString;
use serde::Serialize;

use crate::access::NoAccess;
use crate::auth::caller::Caller;
use crate::auth::permission::{Requirement, Resource};
use crate::config::fhir::FhirSettings;
use crate::error::Code;
use crate::facade::demographics::{self, Unheaded};
use crate::facade::summary::{self as gathered, Scope, Unsummarised};
use crate::state::AppState;

mod admission;
mod binary;
pub mod errors;
mod list;
pub mod locator;
pub mod medication;

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
fn table() -> [(&'static str, MethodRouter<Arc<AppState>>); 11] {
    [
        (METADATA, get(metadata)),
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
            get(medication::read::read_dispensation),
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
/// The capability statement takes a verified caller, a `GET` or `POST` of a
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
        // NOTE: SMART on openEHR master08 §Resource Scopes; a received document is written as
        // a composition whose template the body decides, so only a create of every one covers it.
        FacePath::Bundle if *method == Method::POST => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Create,
            resource: Resource::Unnamed,
        },
        FacePath::Metadata
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

/// Every other path under `{fhir-base}`: a `404` naming the paths the face
/// serves, never the path asked for.
async fn unserved() -> Response {
    outcome(StatusCode::NOT_FOUND, "not-found", &UNSERVED)
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
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to create a composition",
            )
        },
    );
    let admitted = match admitted {
        Ok(admitted) => admitted,
        Err(refused) => return refused,
    };
    let confined = match admitted.confine().await {
        Ok(confined) => confined,
        Err(refused) => return refused,
    };
    let arrived = admitted.arrived(&confined);
    crate::facade::receive::receive(
        &admitted.federation,
        admitted.fhir,
        (arrived, admitted.caller),
        text,
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

/// The `OperationOutcome` of a caller the gate did not admit to the section
/// queries a summary, a list or a listed document runs over.
fn unadmitted() -> Response {
    outcome(
        StatusCode::FORBIDDEN,
        "forbidden",
        "the caller was not admitted to the section queries",
    )
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
    let admitted = admission::admit(
        state,
        (outbound, caller.as_deref()),
        (headers, started),
        unadmitted,
    );
    let admitted = match admitted {
        Ok(admitted) => admitted,
        Err(refused) => return refused,
    };
    let confined = match admitted.confine().await {
        Ok(confined) => confined,
        Err(refused) => return refused,
    };
    let arrived = admitted.arrived(&confined);
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let (request_id, outbound, logged) = (admitted.request_id, admitted.outbound, &admitted.logged);
    let patient = (read.system(), read.value());
    let header = async {
        let named = named(patient).map_err(Headless::Unheaded)?;
        // NOTE: Regulation (EU) 2025/327 Art 3(1), Federation Tier §5.2: a grant confined to one
        // patient is held to it before the demographics supplier is asked about anyone.
        let conveyed = (arrived.conveyance, arrived.on_behalf);
        let ids = (request_id, logged.as_str());
        if let Some(refused) =
            crate::facade::other_patient(federation, conveyed, (&named, started), ids).await
        {
            return Err(Headless::Refused(refused));
        }
        // NOTE: eHN PS A.1.1, A.1.2; the header comes from the identity binding before any
        // member is asked, so no member is reached for a summary that could name no patient.
        headed(federation, (&named, arrived.on_behalf), started)
            .await
            .map_err(Headless::Unheaded)
    };
    let header = match header.await {
        Ok(header) => header,
        Err(headless) => {
            let mut response = match headless {
                Headless::Refused(refused) => refused,
                Headless::Unheaded(unheaded) => unheaded_outcome(unheaded),
            };
            response.extensions_mut().insert(NoAccess);
            return response;
        }
    };
    match gathered::gather(Arc::clone(federation), arrived, patient, Scope::Summary).await {
        Ok(gathered) => document(federation, fhir, &gathered, (patient, header), logged).await,
        Err(unsummarised) => refused(
            federation,
            (unsummarised, Scope::Summary),
            patient,
            (request_id, outbound),
        ),
    }
}

/// Why a summary request gets no header, so nothing is sent to a member.
enum Headless {
    /// The caller is confined to another patient, or its patient could not
    /// be checked: the answer.
    Refused(Response),
    /// The identity binding gives no header.
    Unheaded(Unheaded),
}

/// The patient reference of the identifier `system`|`value` a summary
/// request names.
fn named((system, value): (&str, &str)) -> Result<PatientRef, Unheaded> {
    // NOTE: the request reader refuses an empty system or value, so a reference that cannot
    // be built names no identifier the binding is asked about.
    IdentifierNamespace::new(system)
        .and_then(|namespace| PatientRef::new(namespace, SecretString::from(value)))
        .map_err(|_empty| Unheaded::NoBinding)
}

/// The summary header the identity binding holds of `patient`, asked on
/// behalf of `on_behalf` within the budget of a request that started at
/// `started`, or the [`Unheaded`] that says why there is none.
async fn headed(
    federation: &crate::federation::Federation,
    (patient, on_behalf): (&PatientRef, &OnBehalfOf),
    started: Instant,
) -> Result<PatientHeader, Unheaded> {
    let deadline = started
        .checked_add(federation.budget().overall())
        .unwrap_or(started);
    demographics::header(federation, (patient, on_behalf), deadline).await
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
async fn document(
    federation: &crate::federation::Federation,
    fhir: &FhirSettings,
    gathered: &gathered::Gathered,
    ((system, value), header): ((&str, &str), PatientHeader),
    logged: &str,
) -> Response {
    let request = summary::Request {
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
    let elsewhere = gathered::elsewhere(federation.snapshot(), gathered);
    let assembled = summary::assemble(
        &request,
        &author,
        (&gathered.origins, &gathered.sections, &elsewhere),
        (&fhir.mappings, &fhir.terminology),
    )
    .await;
    let (mut response, delivered, transformations) = match assembled {
        Ok(assembled) => (
            fhir_json(StatusCode::OK, &assembled.bundle),
            assembled.mapped,
            assembled
                .transformations
                .iter()
                .map(crate::access::transformation::of)
                .collect(),
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
                Vec::new(),
            )
        }
    };
    let accessed = gathered::accessed(
        federation,
        gathered,
        (system, value),
        (Scope::Summary, Some(&delivered)),
    )
    .map(|mut accessed| {
        accessed.transformations = transformations;
        accessed
    });
    attach(&mut response, accessed);
    response
}

/// The answer to a request of `scope` the members did not give,
/// `unsummarised`.
fn refused(
    federation: &crate::federation::Federation,
    (unsummarised, scope): (Unsummarised, Scope),
    patient: (&str, &str),
    (request_id, outbound): (&str, OutboundId),
) -> Response {
    // NOTE: no specification governs this: our own design; a refused summary counts what
    // it delivered, none, and a refused list delivered no member's own document.
    let none = BTreeSet::new();
    let delivered = (scope == Scope::Summary).then_some(&none);
    let what = match scope {
        Scope::Summary => "summary",
        Scope::List => "list",
    };
    match unsummarised {
        Unsummarised::Failure(failure) => failure.respond(request_id, outbound),
        Unsummarised::Internal(what) => {
            tracing::error!(request_id = %outbound, what, "the section queries could not be run");
            crate::error::fixed(Code::Internal, request_id)
        }
        // NOTE: Federation Tier §11.3, Regulation (EU) 2025/327 Art 8: an unknown patient and
        // one no member may disclose answer alike, so the outcome names neither.
        Unsummarised::NotFound(gathered) => {
            let mut response = outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                "no member holds a patient summary for this identifier",
            );
            let accessed = gathered.and_then(|gathered| {
                gathered::accessed(federation, &gathered, patient, (scope, delivered))
            });
            attach(&mut response, accessed);
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
                format!(
                    "the {what} is incomplete: the patient could not be resolved at every member (§11.3)",
                )
            } else {
                format!("the {what} is incomplete: no answer from {named} (§11.3)")
            };
            let mut response = outcome(status, code, &diagnostics);
            attach(
                &mut response,
                gathered::accessed(federation, &gathered, patient, (scope, delivered)),
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

#[cfg(test)]
mod tests {
    use super::UNSERVED;

    #[test]
    fn the_404_names_every_path_the_router_serves() {
        assert_eq!(
            UNSERVED.as_str(),
            "the FHIR face serves metadata, Patient/$summary, Bundle, DocumentReference, \
             Binary/{token}, MedicationRequest, MedicationRequest/{id}, \
             MedicationRequest/{id}/_history/{vid}, MedicationDispense, MedicationDispense/{id}, \
             MedicationDispense/{id}/_history/{vid} alone"
        );
    }
}
