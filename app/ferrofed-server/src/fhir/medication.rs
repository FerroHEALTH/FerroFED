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

use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use eehrxf::category::Category;
use http::{Method, StatusCode};

use crate::auth::permission::Requirement;
use crate::config::fhir::FhirSettings;
use crate::state::AppState;

use super::refusal;

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
/// A search or a read of prescriptions is a `GET` and the receipt of a
/// dispensation a `POST`, each by a verified caller; every other method is
/// refused.
#[must_use]
pub fn requirement(addressed: Addressed, method: &Method) -> Requirement {
    match addressed {
        // TODO(#1040): require the scope a prescription search reads under.
        // TODO(#1041): require the scope a prescription read reads under.
        Addressed::Prescriptions | Addressed::Prescription if *method == Method::GET => {
            Requirement::Caller
        }
        // TODO(#1042): require the scope a dispensation is written under.
        Addressed::Dispensations if *method == Method::POST => Requirement::Caller,
        Addressed::Prescriptions | Addressed::Prescription | Addressed::Dispensations => {
            Requirement::Refused
        }
    }
}

/// `GET {fhir-base}/MedicationRequest`: a patient's prescriptions.
pub(super) async fn search(State(state): State<Arc<AppState>>) -> Response {
    if !serves(&state, Category::Prescription) {
        return not_served(Category::Prescription);
    }
    // TODO(#1040): search the patient's prescriptions at every member that serves them.
    unavailable("the search of prescriptions")
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
