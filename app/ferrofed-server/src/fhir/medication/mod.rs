// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routes of electronic prescriptions and dispensations on the FHIR
//! face (Regulation (EU) 2025/327 Art 14(1)(b) and (c)).
//!
//! `GET {fhir-base}/MedicationRequest` searches a patient's prescriptions,
//! `GET {fhir-base}/MedicationRequest/{id}` reads one,
//! `POST {fhir-base}/MedicationDispense` receives a dispensation,
//! `GET {fhir-base}/MedicationDispense/{id}` reads one it received and
//! `PUT {fhir-base}/MedicationDispense/{id}` withdraws one, each at
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
//! ([`crate::fhir_wire::locator`]), which names the member that holds it, so a read
//! goes to that member alone; a received dispensation is named the same way.

pub(super) mod dispense;
mod dispenser;
pub(super) mod read;
pub(super) mod search;
pub(super) mod withdraw;

use axum::response::Response;
use eehrxf::category::Category;
use http::StatusCode;

use crate::state::AppState;

use super::refusal;

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
