// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The address space of the FHIR face.
//!
//! It holds the path of each interaction under `{fhir-base}` and what a raw
//! request path addresses, which the face routes, the gate requires a
//! permission for and the access log records.
//! No specification governs the layout: our own design.

use crate::config::fhir::FhirSettings;

/// The longest FHIR R4 `id`.
pub const LENGTH: usize = 64;

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

/// The path prefix of a member's document under `{fhir-base}`.
pub const BINARY: &str = "/Binary/";

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
    Medication(Addressed),
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
    } else if let Some(addressed) = addressed(fhir, path) {
        FacePath::Medication(addressed)
    } else {
        FacePath::Other
    })
}

/// Whether `path` is `{fhir-base}/Binary/{token}`, the token one segment
/// of the compact JWS alphabet (RFC 7515 §7.1).
fn is_binary(fhir: &FhirSettings, path: &str) -> bool {
    path.strip_prefix(fhir.base.join(BINARY).as_str())
        .is_some_and(|token| {
            !token.is_empty()
                && token
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
}

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
        (Some(id), None, None, None) => is_fhir_id(id).then_some(read),
        (Some(id), Some(HISTORY), Some(vid), None) => {
            (is_fhir_id(id) && is_fhir_id(vid)).then_some(version)
        }
        _ => None,
    }
}

/// Whether `text` is a FHIR R4 `id`: 1 to 64 of `[A-Za-z0-9\-\.]`.
#[must_use]
pub fn is_fhir_id(text: &str) -> bool {
    (1..=LENGTH).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
}
