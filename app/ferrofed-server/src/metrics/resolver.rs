// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The cross-reference resolver, counted and timed.
//!
//! One call is one question to the resolver the deployment configured, a
//! PIX Manager under the IHE binding (Annex A §A.1) or the development
//! cross-reference, for every member it was asked about.
//!
//! The outcome of a call is a closed set read from its answers:
//! `time-out` when the budget ran out for a member, `unavailable` when the
//! service failed for one, `resolved` when a member knows the patient, and
//! `not-resolved` otherwise (§5.2, N6). Nothing of the patient reaches a
//! label (§5.4.1, N33). No specification governs metrics: our own design.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::patient::PatientRef;
use ferrofed_identity::role::resolver::{Resolution, Resolver, ResolverError};
use ferrofed_registry::id::NodeId;

use crate::metrics::nodes::Instruments;

/// How one call to the resolver ended, the `outcome` label of
/// [`RESOLVER_REQUESTS`](crate::metrics::RESOLVER_REQUESTS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// At least one member knows the patient, and the service answered for
    /// every member.
    Resolved,
    /// The service answered for every member, and none knows the patient.
    NotResolved,
    /// The service failed for at least one member.
    Unavailable,
    /// The budget ran out before the service answered for a member.
    TimeOut,
}

impl Outcome {
    /// Every outcome, in declaration order.
    pub const ALL: [Self; 4] = [
        Self::Resolved,
        Self::NotResolved,
        Self::Unavailable,
        Self::TimeOut,
    ];

    /// The label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::NotResolved => "not-resolved",
            Self::Unavailable => "unavailable",
            Self::TimeOut => "time-out",
        }
    }

    /// The outcome of a call that answered `answers`.
    #[must_use]
    pub fn of(answers: &BTreeMap<NodeId, Resolution>) -> Self {
        let mut outcome = Self::NotResolved;
        for answer in answers.values() {
            match answer {
                Resolution::Unavailable(ResolverError::DeadlineExceeded) => return Self::TimeOut,
                Resolution::Unavailable(_) => outcome = Self::Unavailable,
                Resolution::Resolved(_) if outcome == Self::NotResolved => {
                    outcome = Self::Resolved;
                }
                Resolution::Resolved(_) | Resolution::Unknown => {}
            }
        }
        outcome
    }
}

/// A resolver that counts and times every call it passes on.
pub struct Metered {
    inner: Arc<dyn Resolver>,
    instruments: Instruments,
}

impl Metered {
    /// Returns `inner`, counted and timed through `instruments`.
    #[must_use]
    pub fn new(inner: Arc<dyn Resolver>, instruments: Instruments) -> Self {
        Self { inner, instruments }
    }
}

impl std::fmt::Debug for Metered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metered").finish_non_exhaustive()
    }
}

#[async_trait]
impl Resolver for Metered {
    async fn resolve(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> BTreeMap<NodeId, Resolution> {
        let started = Instant::now();
        let answers = self
            .inner
            .resolve(patient, members, on_behalf, deadline)
            .await;
        self.instruments
            .resolved(Outcome::of(&answers), started.elapsed());
        answers
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ferrofed_identity::role::resolver::{Resolution, ResolverError};
    use ferrofed_registry::id::{EhrId, NodeId};

    use super::Outcome;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_failure_outranks_an_answer_and_a_time_out_outranks_both() -> TestResult {
        let (a, b) = (NodeId::new("node-a")?, NodeId::new("node-b")?);
        let ehr_id = EhrId::new("7d44b88c-4199-4bad-97dc-d78268e01398")?;
        let mut answers = BTreeMap::new();
        assert_eq!(Outcome::NotResolved, Outcome::of(&answers));
        answers.insert(a.clone(), Resolution::Unknown);
        assert_eq!(Outcome::NotResolved, Outcome::of(&answers));
        answers.insert(b.clone(), Resolution::Resolved(ehr_id));
        assert_eq!(Outcome::Resolved, Outcome::of(&answers));
        answers.insert(
            a.clone(),
            Resolution::Unavailable(ResolverError::Backend("synthetic".into())),
        );
        assert_eq!(Outcome::Unavailable, Outcome::of(&answers));
        answers.insert(b, Resolution::Unavailable(ResolverError::DeadlineExceeded));
        assert_eq!(Outcome::TimeOut, Outcome::of(&answers));
        Ok(())
    }
}
