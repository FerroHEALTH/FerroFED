// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routes of electronic prescriptions and dispensations on the FHIR
//! face (Regulation (EU) 2025/327 Art 14(1)(b) and (c)).
//!
//! `GET {fhir-base}/MedicationRequest` searches a patient's prescriptions,
//! `GET {fhir-base}/MedicationRequest/{id}` reads one,
//! `POST {fhir-base}/MedicationDispense` receives a dispensation and
//! `GET {fhir-base}/MedicationDispense/{id}` reads one it received, each at
//! the members `[[fhir.serve]]` declares for its category; the two reads
//! also answer at `{id}/_history/{vid}`, the version the member holds now
//! (FHIR R4 `http.html` §3.1.0.3). A category no
//! member is declared for is not served: every route of it answers `404`
//! with an `OperationOutcome` of the issue type `not-supported` saying so,
//! and asks no member, so the answer is never an empty `searchset` a client
//! could read as a patient with no prescription. The status and the issue
//! type are our own design: no specification governs them.
//!
//! The search names each prescription by the id the read takes
//! ([`super::locator`]), which names the member that holds it, so a read
//! goes to that member alone; a received dispensation is named the same way.

pub(super) mod dispense;
pub(super) mod read;
pub(super) mod search;

use axum::response::Response;
use eehrxf::category::Category;
use ferrohealth_smart::openehr::smart_scopes::{Permission, ResourceFamily};
use http::{Method, StatusCode};

use crate::auth::permission::{Requirement, Resource};
use crate::config::fhir::FhirSettings;
use crate::state::AppState;

use super::locator;
use super::refusal;

/// The path of the prescription search under `{fhir-base}`.
pub const PRESCRIPTIONS: &str = "/MedicationRequest";

/// The path of the read of one prescription under `{fhir-base}`.
pub const PRESCRIPTION: &str = "/MedicationRequest/{id}";

/// The path a dispensation is received at under `{fhir-base}`.
pub const DISPENSATIONS: &str = "/MedicationDispense";

/// The path of the read of one dispensation under `{fhir-base}`.
pub const DISPENSATION: &str = "/MedicationDispense/{id}";

/// The path of the version read of one prescription under `{fhir-base}`
/// (FHIR R4 `http.html` §3.1.0.3, vread).
pub const PRESCRIPTION_VERSION: &str = "/MedicationRequest/{id}/_history/{vid}";

/// The path of the version read of one dispensation under `{fhir-base}`
/// (FHIR R4 `http.html` §3.1.0.3, vread), the form of the `Location` a
/// receipt answers when the member names a version.
pub const DISPENSATION_VERSION: &str = "/MedicationDispense/{id}/_history/{vid}";

/// The path segment that introduces a version (FHIR R4 `http.html` §3.1.0.3).
const HISTORY: &str = "_history";

/// What a path of the prescriptions and dispensations addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Addressed {
    /// `{fhir-base}/MedicationRequest`, exactly.
    Prescriptions,
    /// `{fhir-base}/MedicationRequest/{id}`, its id one FHIR R4 `id`.
    Prescription,
    /// `{fhir-base}/MedicationRequest/{id}/_history/{vid}`, its id and its
    /// vid each one FHIR R4 `id`.
    PrescriptionVersion,
    /// `{fhir-base}/MedicationDispense`, exactly.
    Dispensations,
    /// `{fhir-base}/MedicationDispense/{id}`, its id one FHIR R4 `id`.
    Dispensation,
    /// `{fhir-base}/MedicationDispense/{id}/_history/{vid}`, its id and its
    /// vid each one FHIR R4 `id`.
    DispensationVersion,
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
    let (prefix, read, version) = [
        (
            PRESCRIPTIONS,
            Addressed::Prescription,
            Addressed::PrescriptionVersion,
        ),
        (
            DISPENSATIONS,
            Addressed::Dispensation,
            Addressed::DispensationVersion,
        ),
    ]
    .into_iter()
    .find(|(prefix, _, _)| path.starts_with(&format!("{}/", fhir.base.join(prefix))))?;
    let rest = path
        .strip_prefix(fhir.base.join(prefix).as_str())?
        .strip_prefix('/')?;
    let mut segments = rest.split('/');
    // NOTE: FHIR R4 Datatypes `id`, `[A-Za-z0-9\-\.]{1,64}`, for an id and a vid alike (`http.html`
    // §3.1.0.3); any other segment names no resource, so the gate refuses it as a variant of the path.
    match (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) {
        (Some(id), None, None, None) => locator::is_fhir_id(id).then_some(read),
        (Some(id), Some(HISTORY), Some(vid), None) => {
            (locator::is_fhir_id(id) && locator::is_fhir_id(vid)).then_some(version)
        }
        _ => None,
    }
}

/// Returns what a request of `method` to `addressed` requires at the gate.
///
/// A search of prescriptions is a `GET` that takes the `composition-`
/// search over every template, a read of one prescription or one
/// dispensation, or of one version of either, a `GET` that takes the
/// `composition-` read over every template, and the receipt of a
/// dispensation a `POST` that takes the `composition-` create over every
/// template; every other method is refused.
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
        // NOTE: no specification governs the scope of the FHIR face: our own design; the read
        // names no template before the member answers, so only a read of every template's covers it.
        Addressed::Prescription
        | Addressed::PrescriptionVersion
        | Addressed::Dispensation
        | Addressed::DispensationVersion
            if *method == Method::GET =>
        {
            Requirement::Scope {
                family: ResourceFamily::Composition,
                permission: Permission::Read,
                resource: Resource::Unnamed,
            }
        }
        // NOTE: no specification governs the scope of the FHIR face: our own design; a
        // dispensation is EHR content its member writes, as a received document is, so it takes
        // the `composition-` create of every template.
        Addressed::Dispensations if *method == Method::POST => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Create,
            resource: Resource::Unnamed,
        },
        Addressed::Prescriptions
        | Addressed::Prescription
        | Addressed::PrescriptionVersion
        | Addressed::Dispensations
        | Addressed::Dispensation
        | Addressed::DispensationVersion => Requirement::Refused,
    }
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
