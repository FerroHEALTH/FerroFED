// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Step-1 consent pre-filter as every patient route applies it: the
//! federated query and the read of an EHR by subject (N27a, §13.2.1).
//!
//! The pre-filter is asked about the candidates that localization left, before
//! resolution. A candidate it denies is never resolved and never sent a
//! request; a candidate it does not deny is asked, and its node checks consent
//! itself (N26, N27, §14.3). A pre-filter that cannot answer leaves every
//! candidate to its node ([`ON_UNAVAILABLE`]), and the outage is made visible
//! three ways: in the answer's `meta.federation.consent.error` where the route
//! has one, as the pre-filter's state on `GET /health/dependencies`, and in the
//! pre-filter call metrics. No specification names a carrier for the outage;
//! the `meta.federation` member mirrors the localizer's of §14.1, as our own
//! design.

use std::collections::BTreeSet;
use std::time::Instant;

use ferrofed_identity::consent::{ConsentDecision, ConsentError, ON_UNAVAILABLE};
use ferrofed_identity::patient::PatientRef;
use ferrofed_registry::id::NodeId;
use openehr_federation::outcome::ErrorDetail;
use tracing::Instrument as _;

use crate::federation::Federation;
use crate::health::dependencies::Observed;

/// What the consent pre-filter decided about one request's candidates.
#[derive(Debug, Default)]
pub(crate) struct Prefiltered {
    /// The candidates it denied: each is `consent-denied` with no
    /// `latency_ms` (N27a, N40).
    pub(crate) denied: BTreeSet<NodeId>,
    /// Why it could not answer, when it could not.
    pub(crate) unavailable: Option<ErrorDetail>,
}

/// Asks the federation's consent pre-filter which of `candidates` may not be
/// asked about `patient` before `deadline`, and records what it showed of
/// itself on the health record and in the metrics.
///
/// Without a configured pre-filter, or with no candidate, nothing is asked
/// and nothing is denied. Only a candidate can be denied: a member the
/// pre-filter names that was never a candidate is left as it is.
pub(crate) async fn prefilter(
    federation: &Federation,
    patient: &PatientRef,
    candidates: &[NodeId],
    deadline: Instant,
) -> Prefiltered {
    let Some(prefilter) = federation.consent_prefilter() else {
        return Prefiltered::default();
    };
    if candidates.is_empty() {
        return Prefiltered::default();
    }
    let span = tracing::info_span!("consent_prefilter", members = candidates.len());
    let decision = prefilter
        .prefilter(patient, candidates, deadline)
        .instrument(span)
        .await;
    federation
        .dependencies()
        .consent(Observed::of_consent(&decision));
    federation.requests().prefiltered(&decision);
    match decision {
        ConsentDecision::Denied(refused) => Prefiltered {
            denied: among(refused, candidates),
            unavailable: None,
        },
        ConsentDecision::NoSignal => Prefiltered::default(),
        ConsentDecision::Unavailable(failure) => {
            // NOTE: N26, N27, N27a, §13.2.1: no consent signal leaves the node as the sole
            // gate, so every candidate is asked; the event names the failure, never a value.
            tracing::warn!(
                error = %failure,
                policy = ON_UNAVAILABLE,
                "the consent pre-filter could not answer; every candidate is asked"
            );
            Prefiltered {
                denied: BTreeSet::new(),
                unavailable: Some(unanswered(&failure)),
            }
        }
        ConsentDecision::Partial { denied, failure } => {
            // NOTE: N26, N27, N27a, §13.2.1: a candidate the pre-filter could not answer
            // for has no consent signal, so it is asked and its node is the sole gate.
            tracing::warn!(
                error = %failure,
                policy = ON_UNAVAILABLE,
                denied = denied.len(),
                "the consent pre-filter could not answer for every candidate; the others are asked"
            );
            Prefiltered {
                denied: among(denied, candidates),
                unavailable: Some(unanswered(&failure)),
            }
        }
    }
}

/// The members of `refused` that are candidates.
fn among(refused: BTreeSet<NodeId>, candidates: &[NodeId]) -> BTreeSet<NodeId> {
    refused
        .into_iter()
        .filter(|member| candidates.contains(member))
        .collect()
}

/// The `consent.error` of a pre-filter that could not answer for `failure`.
fn unanswered(failure: &ConsentError) -> ErrorDetail {
    ErrorDetail::Text(format!(
        "the consent pre-filter could not answer: {failure}"
    ))
}
