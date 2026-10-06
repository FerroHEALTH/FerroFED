// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The demographics seam: the master identity a patient identifier names,
//! asked of a demographics service before localization and resolution (Annex
//! A §A.2 and §A.7, §4.2, §5.2).
//!
//! A client may name the patient by an identifier in a namespace the
//! cross-reference service does not map. The demographics service finds
//! which person is meant and answers with the identifier the person carries
//! in the master domain, which the gateway then localizes and resolves as it
//! would the client's own. The identifier reaches the demographics service
//! only; the master identifier it answers with is held exactly as the
//! client's is, and neither reaches a node (§5.4.1, N33).

use std::time::Instant;

use async_trait::async_trait;
use thiserror::Error;

use crate::role::behalf::OnBehalfOf;
use crate::role::header::HeaderAnswer;
use crate::role::patient::{IdentifierNamespace, PatientRef};

/// Why a demographics service could not answer.
///
/// A service that could not answer is not a patient it does not know: the
/// gateway reports the failure on every member and never reads it as no
/// match (Annex A §A.2; no specification governs the mapping: our own
/// design).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DemographicsError {
    /// The budget ran out before the service answered.
    #[error("the demographics service did not answer within its budget")]
    DeadlineExceeded,
    /// The service could not be reached, or gave no answer that could be
    /// read.
    #[error("the demographics service failed")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// The service answered `status` with a failure.
    #[error("the demographics service answered {status}")]
    Answered {
        /// The HTTP status the service answered with.
        status: http::StatusCode,
        /// What the binding read of the failure.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The exchange took place, but its audit record could not be recorded,
    /// so its answer is not used; unlike every other failure, this one never
    /// widens to ask-all.
    #[error("the demographics exchange could not be audited")]
    AuditFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl DemographicsError {
    /// The HTTP status the service answered with, when it answered.
    #[must_use]
    pub fn status(&self) -> Option<http::StatusCode> {
        match self {
            Self::Answered { status, .. } => Some(*status),
            Self::DeadlineExceeded | Self::Backend(_) | Self::AuditFailed(_) => None,
        }
    }
}

/// Why a demographics answer names no one master identity, so the gateway
/// refuses to resolve and never picks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum Ambiguity {
    /// The service matched more than one patient.
    #[error(
        "the demographics service matched more than one patient, and the gateway never picks one"
    )]
    SeveralPatients,
    /// The one patient matched carries more than one identifier in the
    /// master domain.
    #[error(
        "the patient the demographics service matched carries more than one master identifier, and the gateway never picks one"
    )]
    SeveralIdentifiers,
    /// The service matched one patient without being certain of the match.
    #[error(
        "the demographics service is not certain of its match, and the gateway resolves only a certain one"
    )]
    Uncertain,
}

/// The answer of a demographics service about one patient identifier.
#[derive(Debug)]
#[non_exhaustive]
pub enum Identification {
    /// The service knows the patient under exactly one master identifier,
    /// which the gateway localizes and resolves in place of the client's.
    Identified(PatientRef),
    /// The service knows no patient for the identifier, or none that carries
    /// a master identifier: the patient is `not-resolved` everywhere, which
    /// fails nothing (N6).
    NoMatch,
    /// The service's answer names no one master identity.
    Ambiguous(Ambiguity),
    /// The service could not answer.
    Unavailable(DemographicsError),
}

/// Finds the master identity of a patient identifier the cross-reference
/// does not map (Annex A §A.2).
///
/// At most one demographics service is active, chosen in configuration. It
/// never returns an error to the core: a failure is
/// [`Identification::Unavailable`].
#[async_trait]
pub trait Demographics: Send + Sync {
    /// Whether an identifier issued in `namespace` is taken to the service
    /// before resolution.
    fn handles(&self, namespace: &IdentifierNamespace) -> bool;

    /// Asks the service which master identity `patient` names, on behalf of
    /// `on_behalf`, before `deadline`.
    async fn identify(
        &self,
        patient: &PatientRef,
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> Identification;

    /// Asks the service what it holds of `patient` for a summary header
    /// (eHN PS A.1.1, A.1.2), on behalf of `on_behalf`, before `deadline`.
    ///
    /// The service is asked by the identifier as the client gave it, in any
    /// namespace it can name, and never by a node's `ehr_id`. It never
    /// returns an error to the core: a failure is
    /// [`HeaderAnswer::Unavailable`].
    async fn header(
        &self,
        patient: &PatientRef,
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> HeaderAnswer;
}
