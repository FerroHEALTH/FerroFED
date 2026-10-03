// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The optional Step-1 consent pre-filter: which candidate members may not be
//! asked about a patient (N27a, §13.2.1, §14.3).
//!
//! The pre-filter runs after localization and before resolution. A member it
//! denies is reported `consent-denied` with no `latency_ms`, is never
//! resolved and never sent a request (N27a, N40). A member it does not deny
//! is asked, and the node checks consent itself: absence from
//! [`ConsentDecision::Denied`] asserts nothing, and the pre-filter is never
//! the gate (N26, N27, §13.2.1).

use std::collections::BTreeSet;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_registry::id::NodeId;
use thiserror::Error;

use crate::patient::PatientRef;

/// Why a consent pre-filter could not answer.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConsentError {
    /// The pre-filter's budget ran out before the consent service answered.
    #[error("the consent service did not answer within its budget")]
    DeadlineExceeded,
    /// The consent service failed: an outage, a refusal, or an answer that
    /// could not be read.
    #[error("the consent service failed")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// What a consent pre-filter answered for one patient and a set of
/// candidate members.
#[derive(Debug)]
pub enum ConsentDecision {
    /// Consent does not permit asking these members. A candidate absent from
    /// the set is not cleared; it is only not ruled out (§14.3).
    Denied(BTreeSet<NodeId>),
    /// The service answered and carries no consent signal for any candidate.
    NoSignal,
    /// The service could not answer.
    Unavailable(ConsentError),
}

/// Answers which candidate members may not be asked about a patient.
///
/// At most one pre-filter is active, chosen in configuration, and a
/// deployment with none is fully conformant (N27a). It never returns an
/// error to the core: a failure is [`ConsentDecision::Unavailable`], under
/// which every candidate is asked ([`ON_UNAVAILABLE`]).
#[async_trait]
pub trait ConsentPrefilter: Send + Sync {
    /// Decides, before `deadline`, which of `candidates` may not be asked
    /// about `patient`.
    async fn prefilter(
        &self,
        patient: &PatientRef,
        candidates: &[NodeId],
        deadline: Instant,
    ) -> ConsentDecision;

    /// The name `OPTIONS {base}/` declares the pre-filter under.
    fn mode(&self) -> &'static str;
}

/// What the gateway does with the candidates when the consent pre-filter
/// could not answer, as `OPTIONS {base}/` declares it: every candidate is
/// asked, and each node applies its own consent check.
///
/// The node checks consent before it releases data in any case (N26, N27,
/// §13.2), so a pre-filter outage leaves Step 1 with no consent signal, which
/// is exactly the state of a deployment with no consent service, and that
/// deployment is fully conformant (N27a, §13.2.1). No specification governs
/// the policy: our own design.
pub const ON_UNAVAILABLE: &str = "pass-to-node";
