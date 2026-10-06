// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The demographics step as every patient route applies it, ahead of
//! localization and resolution (Annex A §A.2 and §A.7, §4.2).
//!
//! A patient named in a namespace the configured demographics service
//! handles is taken to it first, within its own budget. The master identity
//! it answers with replaces the client's identifier for localization, the
//! consent pre-filter and resolution, and the outbound gate withholds both
//! (§5.4.1, N33). An identifier it knows no patient for is `not-resolved`
//! everywhere, which fails nothing (N6). An answer that names no one master
//! identity is refused, never settled by picking one. A service that does not
//! answer is a failure of the step that feeds localization, so an undirected
//! route applies the localization failure policy to it (§14.1, N4). Each call
//! is recorded as the service's state on `GET {base}/operator/dependencies` and in
//! the demographics call metrics. No identifier reaches a log line, a metric
//! label or an error text.

use std::time::Instant;

use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::demographics::{DemographicsError, Identification};
use ferrofed_identity::role::header::{HeaderAnswer, PatientHeader};
use ferrofed_identity::role::patient::PatientRef;
use tracing::Instrument as _;

use crate::facade::localize::inside;
use crate::federation::Federation;
use crate::health::dependencies;

/// What the demographics step made of the patient a route names.
#[derive(Debug)]
pub(crate) enum Identified {
    /// No step applies: the patient is localized and resolved as named.
    AsNamed,
    /// The master identity, localized and resolved in place of the client's
    /// identifier.
    Master(PatientRef),
    /// The service knows no patient for the identifier: every member is
    /// `not-resolved` with this error, which fails nothing (N6).
    NoMatch(String),
    /// The service's answer names no one master identity, so resolution is
    /// refused: every member is `not-resolved` with this error.
    Ambiguous(String),
    /// The service could not answer.
    Unavailable {
        /// The failure, as every member reports it.
        failure: String,
        /// Whether the exchange could not be audited, which no failure
        /// policy widens.
        audit_failed: bool,
    },
}

/// Why the demographics binding gives no summary header.
#[derive(Debug)]
pub(crate) enum Unheaded {
    /// No demographics binding is set, or it is not asked about identifiers
    /// of the patient's namespace.
    NoBinding,
    /// The binding knows no patient for the identifier.
    NoMatch,
    /// The binding's answer names no one patient.
    Ambiguous(String),
    /// The patient the binding holds has no name.
    Unnamed,
    /// The binding could not answer: this failure, and whether it ran out of
    /// time.
    Unavailable {
        /// The failure, which names no value.
        failure: String,
        /// Whether the deadline passed.
        timed_out: bool,
    },
}

/// Asks the federation's demographics binding for the summary header of
/// `patient` on behalf of `on_behalf` before `deadline`, within the step's
/// own budget, and records what the service showed of itself.
///
/// Only a header that names the patient is answered; everything else is the
/// [`Unheaded`] that says why.
pub(crate) async fn header(
    federation: &Federation,
    (patient, on_behalf): (&PatientRef, &OnBehalfOf),
    deadline: Instant,
) -> Result<PatientHeader, Unheaded> {
    let Some(step) = federation.demographics() else {
        return Err(Unheaded::NoBinding);
    };
    let until = Instant::now()
        .checked_add(step.timeout())
        .map_or(deadline, |at| at.min(deadline));
    let started = Instant::now();
    let answer = tokio::time::timeout_at(
        tokio::time::Instant::from_std(until),
        step.step().header(patient, on_behalf, inside(until)),
    )
    .instrument(tracing::info_span!("header"))
    .await
    .unwrap_or(HeaderAnswer::Unavailable(
        DemographicsError::DeadlineExceeded,
    ));
    if matches!(answer, HeaderAnswer::NotHandled) {
        return Err(Unheaded::NoBinding);
    }
    federation
        .dependencies()
        .demographics(dependencies::of_header(&answer));
    federation.requests().headed(&answer, started.elapsed());
    match answer {
        HeaderAnswer::Found(header) if header.named() => Ok(*header),
        HeaderAnswer::Found(_) => Err(Unheaded::Unnamed),
        HeaderAnswer::Ambiguous(ambiguity) => {
            tracing::warn!(reason = %ambiguity, "the demographics service named no one patient for the summary header");
            Err(Unheaded::Ambiguous(ambiguity.to_string()))
        }
        HeaderAnswer::Unavailable(error) => {
            tracing::warn!(error = %crate::chain(&error), "the demographics service did not answer for the summary header");
            Err(Unheaded::Unavailable {
                timed_out: matches!(error, DemographicsError::DeadlineExceeded),
                failure: format!(
                    "the demographics service could not answer: {}",
                    crate::chain(&error)
                ),
            })
        }
        HeaderAnswer::NoMatch => Err(Unheaded::NoMatch),
        _ => Err(Unheaded::NoBinding),
    }
}

/// Takes `patient` to the federation's demographics step on behalf of
/// `on_behalf` before `deadline`, within the step's own budget, and records
/// what the service showed of itself.
///
/// The step is given a deadline [`inside`] its budget, so an exchange it
/// could not audit is reported as that before the budget ends.
pub(crate) async fn identify(
    federation: &Federation,
    (patient, on_behalf): (&PatientRef, &OnBehalfOf),
    deadline: Instant,
) -> Identified {
    let Some(step) = federation.demographics() else {
        return Identified::AsNamed;
    };
    if !step.step().handles(patient.namespace()) {
        return Identified::AsNamed;
    }
    let until = Instant::now()
        .checked_add(step.timeout())
        .map_or(deadline, |at| at.min(deadline));
    let started = Instant::now();
    let answer = tokio::time::timeout_at(
        tokio::time::Instant::from_std(until),
        step.step().identify(patient, on_behalf, inside(until)),
    )
    .instrument(tracing::info_span!("identify"))
    .await
    .unwrap_or(Identification::Unavailable(
        DemographicsError::DeadlineExceeded,
    ));
    federation
        .dependencies()
        .demographics(dependencies::of_identification(&answer));
    federation.requests().identified(&answer, started.elapsed());
    match answer {
        Identification::Identified(master) => Identified::Master(master),
        Identification::NoMatch => Identified::NoMatch(String::from(
            "the demographics service knows no patient for this identifier (Annex A §A.2)",
        )),
        Identification::Ambiguous(ambiguity) => {
            tracing::warn!(
                reason = %ambiguity,
                "the demographics service named no one master identity; the resolution is refused"
            );
            Identified::Ambiguous(format!(
                "the patient could not be identified: {ambiguity} (Annex A §A.2)"
            ))
        }
        Identification::Unavailable(error) => {
            let audit_failed = matches!(error, DemographicsError::AuditFailed(_));
            tracing::warn!(
                error = %crate::chain(&error),
                "the demographics service did not answer"
            );
            Identified::Unavailable {
                failure: format!(
                    "the demographics service could not answer: {}",
                    crate::chain(&error)
                ),
                audit_failed,
            }
        }
        other => {
            tracing::error!(answer = ?other, "the demographics service answered in an unknown form");
            Identified::Unavailable {
                failure: String::from(
                    "the demographics service answered in a form the gateway does not read",
                ),
                audit_failed: false,
            }
        }
    }
}
