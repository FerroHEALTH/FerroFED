// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of a read by subject at its candidates, and the one member
//! that holds the subject's EHR (§5.2, §12.5.2).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use ferrofed_identity::behalf::OnBehalfOf;
use ferrofed_identity::patient::PatientRef;
use ferrofed_identity::resolver::Resolution;
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::Endpoint;

use crate::facade::subject::unserved::Unserved;
use crate::federation::Federation;
use crate::health::dependencies;

/// What the cross-reference said about each candidate.
#[derive(Debug)]
pub(super) struct Resolved<'a> {
    /// The candidates that know the subject, with its local `ehr_id` there.
    pub(super) holders: Vec<(&'a Endpoint, EhrId)>,
    /// The candidates the cross-reference could not answer for.
    silent: Vec<EndpointId>,
    /// The candidates the consent pre-filter denied, never resolved or
    /// contacted (N27a), in a deployment that discloses consent exclusions.
    denied: Vec<EndpointId>,
}

impl<'a> Resolved<'a> {
    /// The one endpoint that holds the subject's EHR, with its `ehr_id`.
    ///
    /// Two holders are a `409` whatever the others answered. Otherwise a
    /// candidate the cross-reference could not answer for may hold it too,
    /// so neither one holder nor none is an answer then (§11.5). A candidate
    /// the consent pre-filter denied was never asked, so with no holder among
    /// the others the answer names the denial and never claims no EHR exists
    /// (N27a); with one holder among them, that holder answers.
    ///
    /// # Errors
    ///
    /// Returns [`Unserved::Several`], [`Unserved::Unresolved`],
    /// [`Unserved::ConsentDenied`] or [`Unserved::Nowhere`].
    pub(super) fn settled(self) -> Result<(&'a Endpoint, EhrId), Unserved> {
        let Self {
            mut holders,
            silent,
            denied,
        } = self;
        if holders.len() > 1 {
            // NOTE: §12.5.2: the gateway never breaks a tie by where the patient
            // resolved, so several holders are listed and none is chosen.
            let endpoints = holders
                .iter()
                .map(|(endpoint, _)| endpoint.id().clone())
                .collect();
            return Err(Unserved::Several(endpoints));
        }
        if !silent.is_empty() {
            return Err(Unserved::Unresolved(silent));
        }
        if holders.is_empty() && !denied.is_empty() {
            return Err(Unserved::ConsentDenied(denied));
        }
        // NOTE: ITS-REST 1.1.0 answers 404 for a subject with no EHR; this is
        // one EHR resource, never the §11.3 result set, so §11.3's 200 does not apply.
        holders.pop().ok_or(Unserved::Nowhere)
    }
}

/// Resolves `patient` at every endpoint of `candidates` on behalf of
/// `on_behalf` before `deadline`, except where the consent pre-filter
/// `denied` the member and the deployment discloses consent exclusions.
///
/// Without a cross-reference service, every candidate is unanswered: the
/// gateway fails closed, as a federated query does. Where the deployment does
/// not disclose consent exclusions, a denied member is resolved with the
/// others and is never a holder, so it is answered for as a member that does
/// not know the subject, or as one the cross-reference could not answer for.
pub(super) async fn resolve<'a>(
    federation: &Federation,
    candidates: Vec<&'a Endpoint>,
    (patient, denied, on_behalf): (&PatientRef, &BTreeSet<NodeId>, &OnBehalfOf),
    deadline: Instant,
) -> Resolved<'a> {
    let disclosed = federation.discloses_consent();
    let (refused, candidates): (Vec<&Endpoint>, Vec<&Endpoint>) = candidates
        .into_iter()
        .partition(|endpoint| disclosed && denied.contains(endpoint.node()));
    let members: Vec<NodeId> = candidates
        .iter()
        .map(|endpoint| endpoint.node().clone())
        .collect();
    let mut answers = match federation.resolver() {
        Some(resolver) if !members.is_empty() => {
            resolver
                .resolve(patient, &members, on_behalf, deadline)
                .await
        }
        Some(_) | None => BTreeMap::new(),
    };
    if let Some(observed) = dependencies::of_resolutions(&answers) {
        federation.dependencies().resolver(observed);
    }
    let mut resolved = Resolved {
        holders: Vec::new(),
        silent: Vec::new(),
        denied: refused
            .iter()
            .map(|endpoint| endpoint.id().clone())
            .collect(),
    };
    for endpoint in candidates {
        match answers.remove(endpoint.node()) {
            Some(Resolution::Resolved(ehr_id)) if !denied.contains(endpoint.node()) => {
                resolved.holders.push((endpoint, ehr_id));
            }
            Some(Resolution::Resolved(_) | Resolution::Unknown) => {}
            Some(Resolution::Unavailable(_)) | None => {
                resolved.silent.push(endpoint.id().clone());
            }
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::Resolved;
    use crate::facade::subject::unserved::Unserved;
    use ferrofed_registry::id::EndpointId;

    fn endpoint(id: &str) -> EndpointId {
        EndpointId::new(id).unwrap()
    }

    #[test]
    fn nothing_resolved_and_nothing_silent_is_nowhere() {
        let resolved = Resolved {
            holders: Vec::new(),
            silent: Vec::new(),
            denied: Vec::new(),
        };
        assert!(matches!(resolved.settled(), Err(Unserved::Nowhere)));
    }

    #[test]
    fn a_silent_candidate_leaves_the_holder_unknown() {
        let resolved = Resolved {
            holders: Vec::new(),
            silent: vec![endpoint("node-b-pub")],
            denied: Vec::new(),
        };
        assert!(matches!(resolved.settled(), Err(Unserved::Unresolved(_))));
    }

    #[test]
    fn denied_candidates_and_no_holder_is_consent_denied() {
        let resolved = Resolved {
            holders: Vec::new(),
            silent: Vec::new(),
            denied: vec![endpoint("node-b-pub")],
        };
        assert!(matches!(
            resolved.settled(),
            Err(Unserved::ConsentDenied(ref endpoints)) if endpoints == &[endpoint("node-b-pub")]
        ));
    }
}
