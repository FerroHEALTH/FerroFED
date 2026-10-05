// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The localizer seam: which members might hold a patient's data, for an
//! undirected query (N4, N10, §14).
//!
//! A localization answer is candidacy, never clearance: a member it names is
//! a member to ask, and the member still decides whether to release (N27,
//! §14.3). A member it does not name is reported `not-localized` and never
//! contacted (§11.1).

use std::collections::BTreeSet;
use std::fmt;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_registry::id::NodeId;
use serde::Deserialize;
use thiserror::Error;

use crate::role::behalf::OnBehalfOf;
use crate::role::patient::PatientRef;

/// Why a localizer could not answer.
///
/// A localizer that could not answer is not a patient with no records: the
/// gateway fails closed by default, reporting every member `not-localized`
/// with this error, and widens to ask-all only where the deployment declared
/// it (§14.1, N4).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LocalizerError {
    /// The localization budget ran out before the localizer answered.
    #[error("the localizer did not answer within its budget")]
    DeadlineExceeded,
    /// The localization service could not be reached, or gave no answer.
    #[error("the localization service failed")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// The localization service answered `status` with a failure, or with an
    /// answer that could not be read.
    #[error("the localization service answered {status}")]
    Answered {
        /// The HTTP status the service answered with.
        status: http::StatusCode,
        /// What was wrong with the answer.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The localization exchange took place, but its audit message could
    /// not be recorded, so its answer is not used. Unlike every other
    /// failure, this one never widens to ask-all: the gateway lost its own
    /// audit trail, which no failure policy covers.
    #[error("the localization exchange could not be audited")]
    AuditFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl LocalizerError {
    /// The HTTP status the localization service answered with, when it
    /// answered.
    #[must_use]
    pub fn status(&self) -> Option<http::StatusCode> {
        match self {
            Self::Answered { status, .. } => Some(*status),
            Self::DeadlineExceeded | Self::Backend(_) | Self::AuditFailed(_) => None,
        }
    }
}

/// The answer of a localizer about one patient (§14.1).
#[derive(Debug)]
pub enum Localization {
    /// No localizer is configured: every member is a candidate, the ask-all
    /// node selection of a deployment with no localizer (§4.3, N4 last
    /// sentence).
    NotConfigured,
    /// The members that might hold the patient's data; every other member is
    /// `not-localized` (§11.1).
    Candidates(BTreeSet<NodeId>),
    /// The localizer answered that no member holds the patient's data: every
    /// member is `not-localized`, with no error (§11.1).
    NoRecords,
    /// The localizer could not answer (§14.1).
    Unavailable(LocalizerError),
}

/// Finds the members that might hold a patient's data (N4, §14.1).
///
/// Exactly one localizer is active, chosen in configuration. It never returns
/// an error to the core: a failure is [`Localization::Unavailable`]. Its
/// answer names members of `members` only; the core ignores any other.
#[async_trait]
pub trait Localizer: Send + Sync {
    /// Localizes `patient` among `members` on behalf of `on_behalf` before
    /// `deadline`.
    async fn localize(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> Localization;
}

/// What the gateway does when a configured localizer does not answer,
/// `federation.localization.on_failure` in `OPTIONS {base}/` (§14.1, §7a.2,
/// N4, N30).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnFailure {
    /// The default: no member is a candidate, every member is `not-localized`
    /// with the localization error, and nothing is dispatched.
    #[default]
    Closed,
    /// Every member is a candidate, as if no localizer were configured; a
    /// deployment offers it only by declaring it.
    AskAll,
}

impl OnFailure {
    /// The value `OPTIONS {base}/` declares, as §14.1 spells it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::AskAll => "ask-all",
        }
    }
}

impl fmt::Display for OnFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
