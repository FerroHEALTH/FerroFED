// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A caller's `patient/` grant confined to one patient.

use std::collections::BTreeMap;

use ferrofed_registry::id::{EhrId, EndpointId};

/// A caller's `patient/` grant confined to one patient: that patient's own
/// `ehr_id` at each endpoint the grant reaches, and no other endpoint.
///
/// The node-local `ehr_id` is the one value N33 lets locate a node, so it is
/// the patient context a node is told, never an identifier of the patient.
/// No specification defines a patient-confined grant across nodes, so the
/// confinement is FerroFED's own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confinement {
    bound: EndpointId,
    at: BTreeMap<EndpointId, EhrId>,
}

impl Confinement {
    /// The confinement to `at`, the patient's `ehr_id` at each endpoint the
    /// grant reaches, of a token issued at the member `bound` reaches.
    #[must_use]
    pub fn new(bound: EndpointId, at: BTreeMap<EndpointId, EhrId>) -> Self {
        Self { bound, at }
    }

    /// The endpoint of the member whose platform issued the token, where
    /// the token's own `ehrId` names the patient's EHR.
    #[must_use]
    pub fn bound(&self) -> &EndpointId {
        &self.bound
    }

    /// The patient's `ehr_id` at the node `endpoint` reaches, or `None` when
    /// the grant does not reach it.
    #[must_use]
    pub fn ehr_id_at(&self, endpoint: &EndpointId) -> Option<&EhrId> {
        self.at.get(endpoint)
    }

    /// Whether the grant reaches `ehr_id` at the node `endpoint` reaches:
    /// the pair, never the `ehr_id` alone, since one `ehr_id` can name
    /// another patient's EHR at another node (§12.5, §12.5.2).
    #[must_use]
    pub fn admits(&self, endpoint: &EndpointId, ehr_id: &EhrId) -> bool {
        self.ehr_id_at(endpoint) == Some(ehr_id)
    }

    /// Every endpoint whose node holds `ehr_id` for the patient.
    pub fn holding<'a>(&'a self, ehr_id: &'a EhrId) -> impl Iterator<Item = &'a EndpointId> {
        self.at
            .iter()
            .filter(move |(_, held)| *held == ehr_id)
            .map(|(endpoint, _)| endpoint)
    }
}
