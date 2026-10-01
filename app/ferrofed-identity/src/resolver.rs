// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolver seam: under which local `ehr_id` each member knows a patient
//! (N3, §5.2, docs/architecture.md section 6).

use std::collections::BTreeMap;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_registry::id::{EhrId, NodeId};
use thiserror::Error;

use crate::patient::PatientRef;

/// Why a resolver could not answer for a member.
///
/// A resolver that could not answer is not a patient who is unknown: the
/// member is reported `not-resolved` with the error, `complete` is cleared,
/// and under the all-or-nothing default the query fails (decision A17, §11.1).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ResolverError {
    /// The resolution budget ran out before the cross-reference answered.
    #[error("the cross-reference did not answer within its budget")]
    DeadlineExceeded,
    /// The cross-reference service failed: an outage, a refusal, or an
    /// answer that could not be read.
    #[error("the cross-reference service failed")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// The outcome of resolving one patient at one member.
#[derive(Debug)]
pub enum Resolution {
    /// The member knows the patient under this `ehr_id`.
    Resolved(EhrId),
    /// The member does not know the patient (N6: this does not fail the
    /// query).
    Unknown,
    /// The resolver could not answer for this member.
    Unavailable(ResolverError),
}

/// Resolves a patient to each member's local `ehr_id`.
///
/// Exactly one resolver is active, chosen in configuration. It never returns
/// an error to the core: a failure is [`Resolution::Unavailable`] for the
/// members it concerns. The answer holds one outcome for every member asked,
/// and none for a member not asked.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolves `patient` at each of `members` before `deadline`.
    async fn resolve(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        deadline: Instant,
    ) -> BTreeMap<NodeId, Resolution>;
}
