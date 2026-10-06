// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolver seam: under which local `ehr_id` each member knows a patient
//! (N3, §5.2).

use std::collections::BTreeMap;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_registry::id::{EhrId, NodeId};
use thiserror::Error;

use crate::role::behalf::OnBehalfOf;
use crate::role::patient::{IdentifierNamespace, PatientRef};

/// Why a resolver could not answer for a member.
///
/// A resolver that could not answer is not a patient who is unknown: the
/// member is reported `not-resolved` with the error, `complete` is cleared,
/// and under the all-or-nothing default the query fails (§11.1; §11.3 covers
/// only an answered lookup, so no specification governs this: our own design).
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

/// What the identity service says of the patient a member holds under one
/// `ehr_id`: the reverse of a [`Resolution`].
#[derive(Debug)]
#[non_exhaustive]
pub enum Identification {
    /// The service holds these identifiers for the patient, each in a
    /// namespace asked.
    Named(Vec<PatientRef>),
    /// The service holds no identifier for the patient in a namespace asked.
    Unknown,
    /// The service could not answer.
    Unavailable(ResolverError),
    /// This resolver cannot name a patient by an `ehr_id`.
    Unsupported,
}

/// Resolves a patient to each member's local `ehr_id`.
///
/// Exactly one resolver is active, chosen in configuration. It never returns
/// an error to the core: a failure is [`Resolution::Unavailable`] for the
/// members it concerns. The answer holds one outcome for every member asked,
/// and none for a member not asked.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolves `patient` at each of `members` on behalf of `on_behalf`
    /// before `deadline`.
    async fn resolve(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> BTreeMap<NodeId, Resolution>;

    /// Names the patient `member` holds under `ehr_id`, by an identifier in
    /// each of `namespaces` the service holds one in, on behalf of
    /// `on_behalf` before `deadline`.
    ///
    /// A resolver that cannot answer this way answers
    /// [`Identification::Unsupported`]; one that wraps another passes the
    /// question on.
    async fn identify(
        &self,
        member: &NodeId,
        ehr_id: &EhrId,
        namespaces: &[IdentifierNamespace],
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> Identification;
}
