// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's request on the FHIR face held to the patient
//! its treatment relationship confirmation binds (eHDSI XCA Profile 8.0.1
//! §4.2.1 and §4.2.2).
//!
//! A patient-data request from a contact point needs the TRC patient its
//! token names, or it is refused `403` as `trc-missing` with nothing sent.
//! The patient it names must be the TRC's own identifier, compared on the
//! assigning authority and the value, before anything is looked up; a
//! resource it reads or writes must sit at one of the TRC patient's own
//! `{node, ehr_id}` pairs ([`crate::facade::trc`]).
//!
//! A request that breaks the binding, one for a patient no member holds, and
//! one for a resource no pair of the TRC patient holds get one answer
//! ([`no_data`]). It is `404` `not-found`, the `trc-patient-mismatch`
//! refusal with the service's Requirements Catalogue 10.0.0 §1.25
//! `*_NOT_FOUND` code, as XCA §4.2.2 has NCP-A answer "No Data" so that no
//! existence is disclosed. It carries nothing that tells the three apart.
//! The legacy numeric codes of appendix 6.4 of the eHDSI components
//! specification for the contact points, 10.0.0, are never used. Inside the gateway a break of the binding stays
//! distinct: it is a security event ([`recorded`]) naming the professional,
//! with both patients pseudonymised (eHDSI Section I Security Policies
//! 10.0.0 SR 5). Blocking the professional, XCA's SHOULD, stays with the
//! contact point.

use std::fmt::Write as _;
use std::sync::LazyLock;

use aws_lc_rs::hmac;
use aws_lc_rs::rand::SystemRandom;
use axum::response::Response;
use ferrofed_eehrxf::face;
use ferrofed_registry::id::{EhrId, NodeId};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use http::StatusCode;
use secrecy::ExposeSecret;

use super::admission::Admitted;
use crate::access::NoAccess;
use crate::facade::request::Arrived;
use crate::facade::trc::Binding;
use crate::fhir_wire::fhir_json;
use crate::security::{Event, TARGET};

/// The code system of the gateway's own refusal codes on the FHIR face.
// NOTE: no specification governs this: our own design; the refusal code travels as a coding
// so a national connector maps it, under the gateway's own domain.
pub(super) const REFUSAL_SYSTEM: &str = "https://ferrofed.eu/fhir/CodeSystem/refusal";

/// The code system the Requirements Catalogue 10.0.0 §1.25 exception code
/// names travel under.
// NOTE: MyHealth@EU RC 10.0.0 §1.25 publishes the code names and no code system URI,
// so the names travel under the gateway's own system (our own design).
pub(super) const EXCEPTION_SYSTEM: &str =
    "https://ferrofed.eu/fhir/CodeSystem/myhealth-eu-exception";

/// The refusal code of a request outside the TRC's patient.
pub(super) const MISMATCH: &str = "trc-patient-mismatch";

/// The refusal code of a contact point's request whose token names no TRC
/// patient.
pub(super) const MISSING: &str = "trc-missing";

/// The diagnostics of [`no_data`], the same for every request it answers.
const NO_DATA: &str =
    "no data is available for the patient the treatment relationship confirmation binds";

/// The MyHealth@EU service a request on the face belongs to, whose §1.25
/// `*_NOT_FOUND` code its [`no_data`] answer carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Service {
    /// The patient summary and its document list.
    PatientSummary,
    /// The prescriptions and the dispensations.
    Prescription,
    /// An original clinical document: a laboratory report.
    OriginalDocument,
}

impl Service {
    /// The Requirements Catalogue 10.0.0 §1.25 code name.
    pub(super) const fn not_found(self) -> &'static str {
        match self {
            Self::PatientSummary => "ERROR_PS_NOT_FOUND",
            Self::Prescription => "ERROR_EP_NOT_FOUND",
            Self::OriginalDocument => "ERROR_ORCD_NOT_FOUND",
        }
    }
}

/// What a break of the binding reached for, as its security record names it.
#[derive(Debug, Clone, Copy)]
pub(super) enum Reached<'a> {
    /// A patient named by `system`|`value`.
    Patient(&'a str, &'a str),
    /// A resource at the member `node`, in its EHR `ehr_id`.
    Resource(&'a NodeId, &'a EhrId),
}

/// The binding of `arrived`'s contact point, or `None` for a caller that is
/// no contact point.
///
/// # Errors
///
/// The `403` `trc-missing` refusal of a contact point whose token names no
/// TRC patient, with nothing sent.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(super) fn binding<'a>(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'a>,
) -> Result<Option<&'a Binding>, Response> {
    if !admitted.caller.is_contact_point() {
        return Ok(None);
    }
    arrived.trc.map(Some).ok_or_else(missing)
}

/// Holds the request `arrived` carries, for the patient `system`|`value`
/// it names, to its contact point's TRC patient before anything is looked
/// up; a caller that is no contact point passes.
///
/// # Errors
///
/// `trc-missing`, or the [`no_data`] answer of `service` for another
/// patient, recorded as a security event, or for a patient no member holds.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(super) fn named(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (system, value): (&str, &str),
    service: Service,
) -> Result<(), Response> {
    let Some(binding) = binding(admitted, arrived)? else {
        return Ok(());
    };
    if binding.names(system, value) {
        return Ok(());
    }
    if !binding.patient().is(system, value) {
        recorded(admitted, binding, Reached::Patient(system, value));
    }
    Err(no_data(service))
}

/// Holds the request `arrived` carries, for a dispensation whose subject
/// names `identifiers`, to its contact point's TRC patient: one identifier
/// must be the TRC's, and the facade holds every other to the same EHRs
/// ([`crate::facade::dispensation`]); a caller that is no contact point
/// passes.
///
/// # Errors
///
/// `trc-missing`, or the [`no_data`] answer of `service` for a subject that
/// names no identifier of the TRC patient, recorded as a security event, or
/// for a TRC patient no member holds.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(super) fn subject(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    identifiers: &[(String, String)],
    service: Service,
) -> Result<(), Response> {
    let Some(binding) = binding(admitted, arrived)? else {
        return Ok(());
    };
    let own = identifiers
        .iter()
        .find(|(system, value)| binding.patient().is(system, value));
    match (own, identifiers.first()) {
        (Some((system, value)), _) if binding.names(system, value) => Ok(()),
        (Some(_), _) => Err(no_data(service)),
        (None, first) => {
            if let Some((system, value)) = first {
                recorded(admitted, binding, Reached::Patient(system, value));
            }
            Err(no_data(service))
        }
    }
}

/// Holds the request `arrived` carries, for the resource it reaches in
/// `ehr_id` at `node`, to its contact point's TRC patient before the member
/// is asked; a caller that is no contact point passes.
///
/// # Errors
///
/// `trc-missing`, or the [`no_data`] answer of `service` for a resource no
/// pair of the TRC patient holds, recorded as a security event.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(super) fn located(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (node, ehr_id): (&NodeId, &EhrId),
    service: Service,
) -> Result<(), Response> {
    let Some(binding) = binding(admitted, arrived)? else {
        return Ok(());
    };
    if binding.holds(node, ehr_id) {
        return Ok(());
    }
    recorded(admitted, binding, Reached::Resource(node, ehr_id));
    Err(no_data(service))
}

/// The answer to a read whose resource sits in `ehr_id` at `node`, which
/// the facade found outside `arrived`'s TRC patient: recorded as a security
/// event, and answered as [`no_data`] of `service`.
pub(super) fn unbound(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (node, ehr_id): (&NodeId, &EhrId),
    service: Service,
) -> Response {
    if let Some(binding) = arrived.trc {
        recorded(admitted, binding, Reached::Resource(node, ehr_id));
    }
    no_data(service)
}

/// The one answer of a request outside its TRC patient: `404` `not-found`,
/// the [`MISMATCH`] refusal and the §1.25 code of `service`, with fixed
/// diagnostics and nothing about the patient or the resource asked.
pub(super) fn no_data(service: Service) -> Response {
    let coding = |system: &str, code: &str| Coding {
        system: Some(system.into()),
        code: Some(code.into()),
        ..Coding::default()
    };
    let outcome = OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "error".into(),
            code: "not-found".into(),
            details: Some(CodeableConcept {
                coding: vec![
                    coding(REFUSAL_SYSTEM, MISMATCH),
                    coding(EXCEPTION_SYSTEM, service.not_found()),
                ],
                ..CodeableConcept::default()
            }),
            diagnostics: Some(NO_DATA.into()),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    };
    let mut response = fhir_json(StatusCode::NOT_FOUND, &outcome);
    response.extensions_mut().insert(NoAccess);
    response.extensions_mut().insert(service);
    response
}

/// The refusal code of an emergency access a contact point relays past the
/// professional's limit.
pub(super) const EMERGENCY_LIMIT: &str = "emergency-limit-reached";

/// The exception code name of a failed security validation of the request
/// or the requestor (eHDSI XCA Profile 8.0.1 §2.2.3).
pub(super) const SECURITY_GENERIC: &str = "ERROR_SEC_GENERIC";

/// The `OperationOutcome` of an emergency access denied past the relayed
/// professional's limit, answered with `status` and `diagnostics`: issue
/// `forbidden`, the [`EMERGENCY_LIMIT`] refusal and [`SECURITY_GENERIC`]
/// (eHDSI Section II Security Services 10.0.0 §2.5; `NCPeH` Architecture
/// Specifications 10.0.0 §5.1.9).
pub(super) fn emergency_denied(status: StatusCode, diagnostics: &str) -> Response {
    let coding = |system: &str, code: &str| Coding {
        system: Some(system.into()),
        code: Some(code.into()),
        ..Coding::default()
    };
    let outcome = OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "error".into(),
            code: "forbidden".into(),
            details: Some(CodeableConcept {
                coding: vec![
                    coding(REFUSAL_SYSTEM, EMERGENCY_LIMIT),
                    coding(EXCEPTION_SYSTEM, SECURITY_GENERIC),
                ],
                ..CodeableConcept::default()
            }),
            diagnostics: Some(diagnostics.into()),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    };
    fhir_json(status, &outcome)
}

/// The `403` refusal of a contact point's patient-data request whose token
/// names no TRC patient (eHDSI SAML Profile 10.0.0 §4.3 makes the TRC's
/// `subject-id` mandatory).
fn missing() -> Response {
    let mut response = fhir_json(
        StatusCode::FORBIDDEN,
        &face::outcome(
            "forbidden",
            &format!(
                "{MISSING}: the national contact point's access token names no patient of a treatment relationship confirmation, so no patient data is served"
            ),
        ),
    );
    response.extensions_mut().insert(NoAccess);
    response
}

/// The key the pseudonyms of one process are drawn with, generated at
/// start and never written anywhere.
static PSEUDONYM_KEY: LazyLock<Option<hmac::Key>> = LazyLock::new(|| {
    // NOTE: no specification governs this: our own design; with no random source the
    // record names no pseudonym, and never the identifier in its place.
    hmac::Key::generate(hmac::HMAC_SHA256, &SystemRandom::new()).ok()
});

/// The pseudonym of `identifier`: the first 16 bytes of its HMAC-SHA256
/// under this process's key, in hexadecimal.
fn pseudonym(identifier: &str) -> String {
    let Some(key) = PSEUDONYM_KEY.as_ref() else {
        return String::from("unavailable");
    };
    let tag = hmac::sign(key, identifier.as_bytes());
    tag.as_ref()
        .iter()
        .take(16)
        .fold(String::with_capacity(32), |mut hex, byte| {
            let _hex_written = write!(hex, "{byte:02x}");
            hex
        })
}

/// Records that `admitted`'s contact point reached `reached` outside the
/// patient `binding` holds: a security event naming the relayed
/// professional, with the patient or resource asked and the TRC patient
/// each by its pseudonym alone.
fn recorded(admitted: &Admitted<'_>, binding: &Binding, reached: Reached<'_>) {
    Event::TrcPatientMismatch.record();
    let relayed = admitted.caller.relayed();
    let professional = relayed.map(|relayed| &relayed.professional);
    let (what, asked) = match reached {
        Reached::Patient(system, value) => ("patient", pseudonym(&format!("{system}|{value}"))),
        Reached::Resource(node, ehr_id) => (
            "resource",
            pseudonym(&format!("{}/{}", node.as_str(), ehr_id.as_str())),
        ),
    };
    tracing::warn!(
        target: TARGET,
        event = Event::TrcPatientMismatch.as_str(),
        what,
        asked,
        bound = pseudonym(binding.patient().token().expose_secret()),
        contact_point = relayed.map(|relayed| relayed.contact_point.as_str()),
        country_code = relayed.map(|relayed| relayed.country_code.as_str()),
        hp_identifier = professional.map(|professional| professional.hp_identifier.as_str()),
        hp_issuing_authority = professional.map(|professional| professional.issuing_authority_name.as_str()),
        request_id = admitted.logged.as_str(),
        "a national contact point's request reached outside the patient its treatment relationship confirmation binds, and was answered as no data"
    );
}

#[cfg(test)]
mod tests {
    use super::{
        EMERGENCY_LIMIT, EXCEPTION_SYSTEM, MISMATCH, MISSING, REFUSAL_SYSTEM, SECURITY_GENERIC,
        Service,
    };

    /// The published `CodeSystem` definitions the face's codes belong to.
    const REFUSAL: &str = include_str!("definition/CodeSystem-refusal.json");
    const EXCEPTION: &str = include_str!("definition/CodeSystem-myhealth-eu-exception.json");

    #[test]
    fn every_code_the_face_sends_is_in_its_published_code_system() {
        assert!(REFUSAL.contains(&format!("\"url\": \"{REFUSAL_SYSTEM}\"")));
        assert!(EXCEPTION.contains(&format!("\"url\": \"{EXCEPTION_SYSTEM}\"")));
        for code in [MISMATCH, MISSING, EMERGENCY_LIMIT] {
            assert!(REFUSAL.contains(&format!("\"code\": \"{code}\"")), "{code}");
        }
        assert!(EXCEPTION.contains(&format!("\"code\": \"{SECURITY_GENERIC}\"")));
        for service in [
            Service::PatientSummary,
            Service::Prescription,
            Service::OriginalDocument,
        ] {
            let code = service.not_found();
            assert!(
                EXCEPTION.contains(&format!("\"code\": \"{code}\"")),
                "{code}"
            );
        }
    }
}
