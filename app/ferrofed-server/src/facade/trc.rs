// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient a national contact point's treatment relationship
//! confirmation binds, resolved at every member before a request reads or
//! writes anything (eHDSI XCA Profile 8.0.1 §4.2.1 and §4.2.2; SAML Profile
//! 10.0.0 §4).
//!
//! The TRC names one patient by its `subject-id`, which the contact point's
//! token carries ([`TrcPatient`]). The gateway resolves it through the
//! cross-reference at every member, as it resolves the patient of a
//! confined grant (§5.2), to the `{node, ehr_id}` pairs of that patient. A
//! request on either face is then held to it: a patient it names must be
//! the TRC's own identifier, and a resource it reads or writes must sit in
//! one of those pairs. A patient the cross-reference knows nowhere holds no
//! pair, so a request for it is answered as one for another patient
//! ([`crate::fhir`], [`NO_DATA`]). No specification defines the binding
//! across members: our own design over the XCA rule that NCP-A compares the
//! patients.
//!
//! A break of the binding is a security event ([`recorded`]) naming the
//! professional, with what was reached and the TRC patient each by a
//! pseudonym (eHDSI Section I Security Policies 10.0.0 SR 5).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::LazyLock;
use std::time::Instant;

use aws_lc_rs::hmac;
use aws_lc_rs::rand::SystemRandom;
use axum::response::Response;
use ferrofed_engine::conveyance::relayed::Relayed;
use ferrofed_registry::id::{EhrId, NodeId};
use secrecy::ExposeSecret;

use super::confined::Unconfined;
use crate::auth::caller::Caller;
use crate::auth::contact_point::TrcPatient;
use crate::error::{self, Code};
use crate::federation::Federation;
use crate::security::{Event, TARGET};

/// The patient a contact point's TRC binds and the `ehr_id` it has at each
/// member that holds it.
#[derive(Debug)]
pub(crate) struct Binding {
    patient: TrcPatient,
    held: BTreeMap<NodeId, EhrId>,
}

impl Binding {
    /// The TRC's patient.
    pub(crate) fn patient(&self) -> &TrcPatient {
        &self.patient
    }

    /// Whether `system`|`value`, the patient a request names, is the TRC's
    /// own identifier, and some member holds that patient.
    pub(crate) fn names(&self, system: &str, value: &str) -> bool {
        self.patient.is(system, value) && !self.held.is_empty()
    }

    /// Whether `ehr_id` at `node` is the TRC patient's own EHR there.
    pub(crate) fn holds(&self, node: &NodeId, ehr_id: &EhrId) -> bool {
        self.held.get(node) == Some(ehr_id)
    }

    /// The members at which `ehr_id` is the TRC patient's own EHR, in
    /// `node_id` order.
    pub(crate) fn holding(&self, ehr_id: &EhrId) -> Vec<NodeId> {
        self.held
            .iter()
            .filter(|(_, held)| *held == ehr_id)
            .map(|(node, _)| node.clone())
            .collect()
    }
}

/// Why the patient a contact point's TRC binds could not be resolved, so
/// nothing is sent: the cross-reference failure, answered as the one of a
/// confined grant and logged as the TRC's own security event.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub(crate) struct Unresolved(Unconfined);

impl Unresolved {
    /// The answer to the request: the `424` a confined grant's unresolved
    /// patient gets, logged under the gateway's `logged` id as the
    /// `trc-patient-unresolved` security event with the cause.
    pub(crate) fn respond(&self, request_id: &str, logged: &str) -> Response {
        Event::TrcPatientUnresolved.record();
        tracing::error!(
            target: TARGET,
            event = Event::TrcPatientUnresolved.as_str(),
            error = crate::chain(&self.0),
            request_id = logged,
            "the patient a national contact point's treatment relationship confirmation binds could not be resolved, so nothing was sent"
        );
        self.0.answer(request_id)
    }
}

/// The binding of `caller`'s TRC patient, resolved on the caller's behalf
/// before `deadline`, or `None` for a caller whose token names no TRC
/// patient.
///
/// # Errors
///
/// Returns an [`Unresolved`] when no resolver is configured, the identifier
/// does not form a patient reference, or the cross-reference could not
/// answer for a member, since a member it could not answer for may hold the
/// patient.
pub(crate) async fn bind(
    federation: &Federation,
    caller: &Caller,
    deadline: Instant,
) -> Result<Option<Binding>, Unresolved> {
    let Some(trc) = caller.trc_patient() else {
        return Ok(None);
    };
    let patient = trc
        .patient()
        .map_err(|invalid| Unresolved(Unconfined::Patient(invalid)))?;
    let held = super::confined::everywhere(federation, &patient, &caller.on_behalf(), deadline)
        .await
        .map_err(Unresolved)?;
    Ok(Some(Binding {
        patient: trc.clone(),
        held,
    }))
}

/// The binding a patient-data request on the ITS-REST face is held to,
/// for `caller`, resolved within the overall budget of a request that
/// arrived at `started`; `None` for a caller that is no contact point.
///
/// # Errors
///
/// The `403` `trc-missing` of a contact point whose token names no TRC
/// patient (eHDSI SAML Profile 10.0.0 §4.3 makes the TRC's `subject-id`
/// mandatory), and the `424` of a TRC patient that cannot be resolved, each
/// answered under `request_id` and logged under `logged` with nothing sent.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(crate) async fn admitted(
    federation: &Federation,
    caller: Option<&Caller>,
    started: Instant,
    (request_id, logged): (&str, &str),
) -> Result<Option<Binding>, Response> {
    let Some(caller) = caller.filter(|caller| caller.is_contact_point()) else {
        return Ok(None);
    };
    if caller.trc_patient().is_none() {
        return Err(error::fixed(Code::TrcMissing, request_id));
    }
    let deadline = started
        .checked_add(federation.budget().overall())
        .ok_or_else(|| Unresolved(Unconfined::Clock).respond(request_id, logged))?;
    // NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: the TRC's patient is resolved before any
    // member is asked, so every read and write of the request is held to it.
    bind(federation, caller, deadline)
        .await
        .map_err(|unresolved| unresolved.respond(request_id, logged))
}

/// The code of the ITS-REST answer to a contact point's request outside
/// its TRC patient, to one for a TRC patient no member holds, and to one
/// every member that holds it restricts: the `404` `subject-unavailable` a
/// subject no member it may read holds gets under withheld consent
/// exclusions, which a contact point's request always is (Regulation (EU)
/// 2025/327 Art 8), answered with its fixed message.
// NOTE: eHDSI XCA Profile 8.0.1 §4.2.1 returns an error and §4.2.2 "No Data" disclosing no
// existence; ITS-REST 1.1.0 answers an EHR it does not find `404`, so the three read as one.
pub(crate) const NO_DATA: Code = Code::SubjectUnavailable;

/// The `403` `trc-unbound` of a contact point's patient-data request that
/// names no patient or EHR a TRC can bind, of the kind `what` names: a
/// population query, the creation of an EHR, or a DEMOGRAPHIC request.
/// It is recorded as a break of `binding` by the contact point relaying
/// `relayed`, with nothing sent.
pub(crate) fn unbound(
    binding: &Binding,
    relayed: Option<&Relayed>,
    what: &'static str,
    (request_id, logged): (&str, &str),
) -> Response {
    recorded(relayed, binding, Reached::Unbound(what), logged);
    error::fixed(Code::TrcUnbound, request_id)
}

/// What a break of the binding reached for, as its security record names it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Reached<'a> {
    /// A patient named by `system`|`value`.
    Patient(&'a str, &'a str),
    /// A resource at the member `node`, in its EHR `ehr_id`.
    Resource(&'a NodeId, &'a EhrId),
    /// An EHR `ehr_id` no pair of the TRC patient holds, located at no
    /// member.
    Ehr(&'a EhrId),
    /// A request that names no patient or EHR, of the kind it names.
    Unbound(&'static str),
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

/// Records that the contact point relaying `relayed` reached `reached`
/// outside the patient `binding` holds, in the request the gateway logs as
/// `logged`: a security event naming the relayed professional, with the
/// patient or resource asked and the TRC patient each by its pseudonym
/// alone.
pub(crate) fn recorded(
    relayed: Option<&Relayed>,
    binding: &Binding,
    reached: Reached<'_>,
    logged: &str,
) {
    Event::TrcPatientMismatch.record();
    let professional = relayed.map(|relayed| &relayed.professional);
    let (what, asked) = match reached {
        Reached::Patient(system, value) => {
            ("patient", Some(pseudonym(&format!("{system}|{value}"))))
        }
        Reached::Resource(node, ehr_id) => (
            "resource",
            Some(pseudonym(&format!("{}/{}", node.as_str(), ehr_id.as_str()))),
        ),
        Reached::Ehr(ehr_id) => ("ehr", Some(pseudonym(ehr_id.as_str()))),
        Reached::Unbound(what) => (what, None),
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
        request_id = logged,
        "a national contact point's request reached outside the patient its treatment relationship confirmation binds, and nothing of it was served"
    );
}
