// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routes of electronic prescriptions and dispensations on the FHIR
//! face (Regulation (EU) 2025/327 Art 14(1)(b) and (c)).
//!
//! `GET {fhir-base}/MedicationRequest` searches a patient's prescriptions,
//! `GET {fhir-base}/MedicationRequest/{id}` reads one, and
//! `POST {fhir-base}/MedicationDispense` receives a dispensation, each at
//! the members `[[fhir.serve]]` declares for its category. A category no
//! member is declared for is not served: every route of it answers `404`
//! with an `OperationOutcome` of the issue type `not-supported` saying so,
//! and asks no member, so the answer is never an empty `searchset` a client
//! could read as a patient with no prescription. The status and the issue
//! type are our own design: no specification governs them.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::extract::State;
use axum::response::Response;
use eehrxf::category::Category;
use ferrofed_eehrxf::prescriptions::SearchRequest;
use ferrofed_eehrxf::prescriptions::searchset::{
    Answered, Assembled, AssemblyError, Silent, assemble,
};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_engine::outbound_id::OutboundId;
use http::{Method, StatusCode, Uri};
use openehr_sdt::smart_scopes::{Permission, ResourceFamily};

use crate::auth::caller::Caller;
use crate::auth::permission::{Requirement, Resource};
use crate::config::fhir::FhirSettings;
use crate::error::Code;
use crate::facade::prescriptions::{self, Held, Prescribed, Unprescribed};
use crate::facade::summary::organisation;
use crate::federation::Federation;
use crate::state::AppState;

use super::{admission, attach, fhir_json, outcome, refusal};

/// The path of the prescription search under `{fhir-base}`.
pub const PRESCRIPTIONS: &str = "/MedicationRequest";

/// The path of the read of one prescription under `{fhir-base}`.
pub const PRESCRIPTION: &str = "/MedicationRequest/{id}";

/// The path a dispensation is received at under `{fhir-base}`.
pub const DISPENSATIONS: &str = "/MedicationDispense";

/// The longest FHIR R4 resource id.
const ID_LENGTH: usize = 64;

/// What a path of the prescriptions and dispensations addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Addressed {
    /// `{fhir-base}/MedicationRequest`, exactly.
    Prescriptions,
    /// `{fhir-base}/MedicationRequest/{id}`, its id one FHIR R4 `id`.
    Prescription,
    /// `{fhir-base}/MedicationDispense`, exactly.
    Dispensations,
}

/// What `path`, the raw request path, addresses among the prescriptions and
/// dispensations of the face `fhir` serves, or `None` for any other path.
#[must_use]
pub fn addressed(fhir: &FhirSettings, path: &str) -> Option<Addressed> {
    if path == fhir.base.join(PRESCRIPTIONS) {
        return Some(Addressed::Prescriptions);
    }
    if path == fhir.base.join(DISPENSATIONS) {
        return Some(Addressed::Dispensations);
    }
    let prefix = fhir.base.join(PRESCRIPTIONS);
    let id = path.strip_prefix(prefix.as_str())?.strip_prefix('/')?;
    // NOTE: FHIR R4 Datatypes `id`, `[A-Za-z0-9\-\.]{1,64}`; any other segment names no
    // resource, so the gate refuses it as a variant of the path.
    let resource_id = (1..=ID_LENGTH).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'));
    resource_id.then_some(Addressed::Prescription)
}

/// Returns what a request of `method` to `addressed` requires at the gate.
///
/// A search of prescriptions is a `GET` that takes the `composition-`
/// search over every template, a read of one a `GET` and the receipt of a
/// dispensation a `POST`, each by a verified caller; every other method is
/// refused.
#[must_use]
pub fn requirement(addressed: Addressed, method: &Method) -> Requirement {
    match addressed {
        // NOTE: no specification governs the scope of the FHIR face: our own design; a
        // prescription is EHR content its member holds, so only a search of every template's covers it.
        Addressed::Prescriptions if *method == Method::GET => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Search,
            resource: Resource::Unnamed,
        },
        // TODO(#1041): require the scope a prescription read reads under.
        Addressed::Prescription if *method == Method::GET => Requirement::Caller,
        // TODO(#1042): require the scope a dispensation is written under.
        Addressed::Dispensations if *method == Method::POST => Requirement::Caller,
        Addressed::Prescriptions | Addressed::Prescription | Addressed::Dispensations => {
            Requirement::Refused
        }
    }
}

/// `GET {fhir-base}/MedicationRequest?patient.identifier=system|value`: a
/// patient's prescriptions, at every member that serves them
/// ([`crate::facade::prescriptions`]).
///
/// A patient no member serving prescriptions holds, and one none may say
/// anything about, both answer the same empty `searchset` (FHIR R4 search
/// §3.1.1.3: an unknown subject is no failure; Federation Tier §11.3).
pub(super) async fn search(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    uri: Uri,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    if !serves(&state, Category::Prescription) {
        return not_served(Category::Prescription);
    }
    let read = match SearchRequest::from_query(uri.query()) {
        Ok(read) => read,
        Err(refused) => {
            return refusal(StatusCode::BAD_REQUEST, "invalid", &refused.to_string());
        }
    };
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to search prescriptions",
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
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let none = BTreeSet::new();
    let declared = fhir
        .serving
        .members_of(Category::Prescription)
        .unwrap_or(&none);
    let patient = (read.system(), read.value());
    let searched = prescriptions::search(federation, &arrived, patient, declared).await;
    let (request_id, outbound) = (admitted.request_id, admitted.outbound);
    match searched {
        Ok(prescribed) => listed(federation, fhir, (&read, &prescribed), &admitted.logged),
        Err(Unprescribed::NotFound(prescribed)) => {
            let mut response = listed_none(fhir, &read, &admitted.logged);
            let accessed = prescribed.and_then(|prescribed| {
                prescriptions::accessed(federation, &prescribed, patient, None)
            });
            attach(&mut response, accessed);
            response
        }
        Err(Unprescribed::Incomplete {
            status,
            silent,
            prescribed,
        }) => {
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
                "the search is incomplete: the patient could not be resolved at every member (§11.3)"
                    .to_owned()
            } else {
                format!("the search is incomplete: no answer from {named} (§11.3)")
            };
            let mut response = outcome(status, code, &diagnostics);
            attach(
                &mut response,
                prescriptions::accessed(federation, &prescribed, patient, None),
            );
            response
        }
        Err(Unprescribed::Failure(failure)) => failure.respond(request_id, outbound),
        Err(Unprescribed::Internal(what)) => {
            tracing::error!(request_id = %outbound, what, "the prescription search could not be run");
            crate::error::fixed(Code::Internal, request_id)
        }
    }
}

/// The page of the `searchset` `read` asks for of what `prescribed` holds,
/// on the face `fhir`, with the access it records.
fn listed(
    federation: &Federation,
    fhir: &FhirSettings,
    (read, prescribed): (&SearchRequest, &Prescribed),
    logged: &str,
) -> Response {
    let snapshot = federation.snapshot();
    let found: Vec<(&prescriptions::Member, Organisation)> = prescribed
        .members
        .iter()
        .filter(|member| matches!(member.held, Held::Found(_)))
        .map(|member| (member, organisation(snapshot, &member.chosen.node)))
        .collect();
    let answered: Vec<Answered<'_>> = found
        .iter()
        .filter_map(|(member, organisation)| match &member.held {
            Held::Found(pages) => Some(Answered {
                endpoint: member.chosen.native.as_str(),
                organisation,
                pages,
            }),
            Held::Unusable { .. } | Held::Silent(_) => None,
        })
        .collect();
    let silent: Vec<Silent> = prescribed
        .members
        .iter()
        .filter(|member| !matches!(member.held, Held::Found(_)))
        .map(|member| Silent {
            endpoint: member.chosen.native.as_str().to_owned(),
            status: member.held.status().as_str().to_owned(),
        })
        .collect();
    let assembled = written(fhir, read, (&answered, &silent));
    let (mut response, assembled) = match assembled {
        Ok(assembled) => (
            fhir_json(StatusCode::OK, &assembled.searchset),
            Some(assembled),
        ),
        Err(failure) => {
            tracing::error!(
                error = %crate::chain(&failure),
                request_id = logged,
                "the prescription searchset could not be written"
            );
            let response = outcome(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the prescriptions could not be listed from what the members answered",
            );
            (response, None)
        }
    };
    let accessed = prescriptions::accessed(
        federation,
        prescribed,
        (read.system(), read.value()),
        assembled.as_ref(),
    );
    attach(&mut response, accessed);
    response
}

/// The empty `searchset` `read` is answered with when no member serving
/// prescriptions holds the patient, or none may say so.
fn listed_none(fhir: &FhirSettings, read: &SearchRequest, logged: &str) -> Response {
    match written(fhir, read, (&[], &[])) {
        Ok(assembled) => fhir_json(StatusCode::OK, &assembled.searchset),
        Err(failure) => {
            tracing::error!(
                error = %crate::chain(&failure),
                request_id = logged,
                "the prescription searchset could not be written"
            );
            outcome(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the prescriptions could not be listed",
            )
        }
    }
}

/// The page `read` asks for of what `answered` holds, naming every member
/// of `silent`, on the face `fhir`.
fn written(
    fhir: &FhirSettings,
    read: &SearchRequest,
    (answered, silent): (&[Answered<'_>], &[Silent]),
) -> Result<Assembled, AssemblyError> {
    let recorded = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let mut mint = || format!("urn:uuid:{}", uuid::Uuid::new_v4());
    assemble(
        (read, fhir.absolute.trim_end_matches('/')),
        (answered, silent),
        &recorded,
        &mut mint,
    )
}

/// `GET {fhir-base}/MedicationRequest/{id}`: one prescription.
pub(super) async fn read(State(state): State<Arc<AppState>>) -> Response {
    if !serves(&state, Category::Prescription) {
        return not_served(Category::Prescription);
    }
    // TODO(#1041): read the prescription at the member its id names.
    unavailable("the read of a prescription")
}

/// `POST {fhir-base}/MedicationDispense`: a dispensation.
pub(super) async fn dispense(State(state): State<Arc<AppState>>) -> Response {
    if !serves(&state, Category::Dispensation) {
        return not_served(Category::Dispensation);
    }
    // TODO(#1042): write the dispensation to the member that owns its prescription.
    unavailable("the receipt of a dispensation")
}

/// Whether a member is declared for `category`.
fn serves(state: &AppState, category: Category) -> bool {
    state
        .fhir()
        .is_some_and(|fhir| fhir.serving.serves(category))
}

/// The answer to a route of `category` when no member serves it, asking no
/// member.
fn not_served(category: Category) -> Response {
    let what = if category == Category::Dispensation {
        "electronic dispensations"
    } else {
        "electronic prescriptions"
    };
    refusal(
        StatusCode::NOT_FOUND,
        "not-supported",
        &format!(
            "{what} ({}) are not served by this deployment",
            category.code()
        ),
    )
}

/// The answer to a route of a served category whose interaction this
/// release does not carry, asking no member.
fn unavailable(interaction: &str) -> Response {
    refusal(
        StatusCode::NOT_IMPLEMENTED,
        "not-supported",
        &format!("{interaction} is not available in this release"),
    )
}
