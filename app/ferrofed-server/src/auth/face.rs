// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a request to the FHIR face requires at the gate, by the path it
//! addresses. No specification governs the scope of the FHIR face: our own
//! design.

use ferrohealth_smart::openehr::smart_scopes::{Permission, ResourceFamily};
use http::Method;

use crate::auth::permission::{Requirement, Resource};
use crate::face_path::{Addressed, FacePath};

/// Returns what a request of `method` to `face` requires at the gate.
///
/// The capability statement takes a verified caller, a `GET` of the patient
/// lookup the `composition-` read of every template, a `GET` or `POST` of a
/// summary, a `GET` of the document list or of a listed document the `aql-` search of every
/// section query, a `POST` of a document
/// the `composition-` create of every template, a prescription or
/// dispensation path what [`medication`] says, and every other
/// method and path is refused, so no variant of a path is let through on a
/// weaker requirement.
#[must_use]
pub fn requirement(face: FacePath, method: &Method) -> Requirement {
    match face {
        FacePath::Medication(addressed) => medication(addressed, method),
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

/// Returns what a request of `method` to `addressed` requires at the gate.
///
/// A search of prescriptions is a `GET` that takes the `composition-`
/// search over every template, a read of one prescription or one
/// dispensation, or of one version of either, a `GET` that takes the
/// `composition-` read over every template, and the receipt of a
/// dispensation a `POST` that takes the `composition-` create over every
/// template, and its withdrawal a `PUT` that takes the `composition-`
/// update over every template; every other method is refused.
#[must_use]
pub fn medication(addressed: Addressed, method: &Method) -> Requirement {
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
        // NOTE: no specification governs the scope of the FHIR face: our own design; a
        // withdrawal changes EHR content its member holds, so it takes the `composition-` update
        // of every template.
        Addressed::Dispensation if *method == Method::PUT => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Update,
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
