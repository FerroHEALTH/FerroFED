// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Localization as every undirected patient route applies it: the federated
//! query and the read of an EHR by subject (N4, N10, §14.1).
//!
//! The configured localizer is asked which members might hold the patient's
//! data, within its own budget. A member it does not name is no candidate. A
//! localizer that does not answer leaves no candidate under the default
//! fail-closed policy, and every member under a declared `ask-all`; either
//! way its failure is kept, for the route to report. Each call is recorded
//! as the localizer's state on `GET /health/dependencies` and in the
//! localizer call metrics.

use std::collections::BTreeSet;
use std::time::Instant;

use ferrofed_identity::localizer::{Localization, LocalizerError, OnFailure};
use ferrofed_identity::patient::PatientRef;
use ferrofed_registry::id::NodeId;
use openehr_federation::outcome::{ErrorDetail, Outcome};

use crate::federation::Federation;
use crate::health::dependencies::Observed;

/// What localization left of the members asked (§14.1).
#[derive(Debug)]
pub(crate) struct Localized {
    /// The members localization named, or `None` when every member is a
    /// candidate.
    candidates: Option<BTreeSet<NodeId>>,
    /// The error every member it did not name carries: the localizer's
    /// failure, under fail-closed.
    error: Option<ErrorDetail>,
    /// The localizer's failure, carried in `meta.federation` too.
    pub(crate) failure: Option<ErrorDetail>,
}

impl Localized {
    /// Every member a candidate: no localizer, or one that is not consulted.
    pub(crate) fn everyone() -> Self {
        Self {
            candidates: None,
            error: None,
            failure: None,
        }
    }

    /// No member a candidate, each carrying `error` when there is one.
    fn nobody(error: Option<ErrorDetail>) -> Self {
        Self {
            candidates: Some(BTreeSet::new()),
            failure: error.clone(),
            error,
        }
    }

    /// Whether `member` is a candidate.
    pub(crate) fn admits(&self, member: &NodeId) -> bool {
        self.candidates
            .as_ref()
            .is_none_or(|candidates| candidates.contains(member))
    }

    /// Whether the localizer failed and the deployment kept to fail-closed,
    /// so no member is a candidate because of the failure.
    pub(crate) fn failed_closed(&self) -> bool {
        self.error.is_some()
    }

    /// The status of a member that is not a candidate (§11.1).
    pub(crate) fn not_localized(&self) -> Outcome {
        Outcome::NotLocalized {
            error: self.error.clone(),
        }
    }
}

/// Asks the federation's localizer which of `members` might hold
/// `patient`'s data, within the localizer's budget and before `deadline`
/// (§14.1, N4), and records what it showed of itself.
///
/// Without a configured localizer, or with no member, every member is a
/// candidate. A localizer still silent at the end of its budget did not
/// answer, and the failure policy applies as to any other failure.
pub(crate) async fn localize(
    federation: &Federation,
    patient: &PatientRef,
    members: &[NodeId],
    deadline: Instant,
) -> Localized {
    let policy = federation.localization();
    let Some(localizer) = policy.localizer() else {
        return Localized::everyone();
    };
    if members.is_empty() {
        return Localized::everyone();
    }
    let until = Instant::now()
        .checked_add(policy.timeout())
        .map_or(deadline, |at| at.min(deadline));
    let answer = tokio::time::timeout_at(
        tokio::time::Instant::from_std(until),
        localizer.localize(patient, members, until),
    )
    .await
    .unwrap_or(Localization::Unavailable(LocalizerError::DeadlineExceeded));
    if let Some(observed) = Observed::of_localization(&answer) {
        federation.dependencies().localizer(observed);
    }
    federation.requests().localized(&answer);
    match answer {
        Localization::NotConfigured => Localized::everyone(),
        Localization::Candidates(named) => Localized {
            candidates: Some(named),
            error: None,
            failure: None,
        },
        Localization::NoRecords => Localized::nobody(None),
        Localization::Unavailable(error) => {
            let cause = crate::chain(&error);
            tracing::warn!(
                error = %cause,
                on_failure = %policy.on_failure(),
                "the localizer did not answer"
            );
            let failure = ErrorDetail::Text(format!("the localizer could not answer: {cause}"));
            match policy.on_failure() {
                OnFailure::Closed => Localized::nobody(Some(failure)),
                OnFailure::AskAll => Localized {
                    candidates: None,
                    error: None,
                    failure: Some(failure),
                },
            }
        }
    }
}
