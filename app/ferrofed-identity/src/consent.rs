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
use std::time::{Duration, Instant};

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
    /// The consent service could not be reached, or gave no answer.
    #[error("the consent service failed")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// The consent service answered `status` with a failure, or with an
    /// answer that could not be read.
    #[error("the consent service answered {status}")]
    Answered {
        /// The HTTP status the service answered with.
        status: http::StatusCode,
        /// What was wrong with the answer.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl ConsentError {
    /// The HTTP status the consent service answered with, when it answered.
    #[must_use]
    pub fn status(&self) -> Option<http::StatusCode> {
        match self {
            Self::Answered { status, .. } => Some(*status),
            Self::DeadlineExceeded | Self::Backend(_) => None,
        }
    }
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
    /// The service denied asking `denied` and could not answer for some
    /// other candidate, for the reason `failure` gives. A candidate it could
    /// not answer for is asked, as under [`ConsentDecision::Unavailable`].
    Partial {
        /// The candidates consent does not permit asking.
        denied: BTreeSet<NodeId>,
        /// Why the service could not answer for the others.
        failure: ConsentError,
    },
}

/// Who asks for the patient's data, as the verified caller's token states it
/// (§13.1, §13.4): the professional and their role, and the organisation and
/// its type.
///
/// Every value comes from the caller's own verified token, never from the
/// gateway's configuration. `Debug` shows the role and the organisation and
/// leaves out the professional's number.
#[derive(Clone, PartialEq, Eq)]
pub struct Requester {
    professional: String,
    role: String,
    organisation: String,
    organisation_type: String,
}

impl Requester {
    /// Returns the requester the four values name, or `None` when one is
    /// empty: a caller whose token does not state them all names no
    /// requester.
    #[must_use]
    pub fn new(
        professional: String,
        role: String,
        organisation: String,
        organisation_type: String,
    ) -> Option<Self> {
        let complete = [&professional, &role, &organisation, &organisation_type]
            .iter()
            .all(|value| !value.is_empty());
        complete.then_some(Self {
            professional,
            role,
            organisation,
            organisation_type,
        })
    }

    /// Returns the professional's identification number (UZI).
    #[must_use]
    pub fn professional(&self) -> &str {
        &self.professional
    }

    /// Returns the professional's role code (UZI role).
    #[must_use]
    pub fn role(&self) -> &str {
        &self.role
    }

    /// Returns the organisation's identifier (URA).
    #[must_use]
    pub fn organisation(&self) -> &str {
        &self.organisation
    }

    /// Returns the organisation's type.
    #[must_use]
    pub fn organisation_type(&self) -> &str {
        &self.organisation_type
    }
}

impl std::fmt::Debug for Requester {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Requester")
            .field("role", &self.role)
            .field("organisation", &self.organisation)
            .field("organisation_type", &self.organisation_type)
            .finish_non_exhaustive()
    }
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
    /// about `patient` on behalf of `requester`, the verified caller, when
    /// the caller's token names one.
    async fn prefilter(
        &self,
        patient: &PatientRef,
        requester: Option<&Requester>,
        candidates: &[NodeId],
        deadline: Instant,
    ) -> ConsentDecision;

    /// The name `OPTIONS {base}/` declares the pre-filter under.
    fn mode(&self) -> &'static str;

    /// The longest one pre-filter decision may take, a part of the overall
    /// budget, or `None` for a pre-filter that asks no remote service.
    fn budget(&self) -> Option<Duration>;
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
