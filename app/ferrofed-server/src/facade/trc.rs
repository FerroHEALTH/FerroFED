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
//! request on the FHIR face is then held to it: a patient it names must be
//! the TRC's own identifier, and a resource it reads or writes must sit in
//! one of those pairs. A patient the cross-reference knows nowhere holds no
//! pair, so a request for it is answered as one for another patient
//! ([`crate::fhir`]). No specification defines the binding across members:
//! our own design over the XCA rule that NCP-A compares the patients.

use std::collections::BTreeMap;
use std::time::Instant;

use ferrofed_registry::id::{EhrId, NodeId};

use super::confined::Unconfined;
use crate::auth::caller::Caller;
use crate::auth::contact_point::TrcPatient;
use crate::federation::Federation;

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
}

/// The binding of `caller`'s TRC patient, resolved on the caller's behalf
/// before `deadline`, or `None` for a caller whose token names no TRC
/// patient.
///
/// # Errors
///
/// Returns an [`Unconfined`] when no resolver is configured, the identifier
/// does not form a patient reference, or the cross-reference could not
/// answer for a member, since a member it could not answer for may hold the
/// patient.
pub(crate) async fn bind(
    federation: &Federation,
    caller: &Caller,
    deadline: Instant,
) -> Result<Option<Binding>, Unconfined> {
    let Some(trc) = caller.trc_patient() else {
        return Ok(None);
    };
    let patient = trc.patient().map_err(Unconfined::Patient)?;
    let held =
        super::confined::everywhere(federation, &patient, &caller.on_behalf(), deadline).await?;
    Ok(Some(Binding {
        patient: trc.clone(),
        held,
    }))
}
