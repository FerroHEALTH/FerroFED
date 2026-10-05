// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit recorders of the IHE transactions.
//!
//! The DICOM audit message of ITI-55 goes over syslog ([`atna`]), and the
//! BALP `AuditEvent` of the FHIR profiles over the FHIR Feed of ITI-20 or to
//! the log ([`balp`]).

pub mod atna;
pub mod balp;

/// The `tracing` target the log recorders write to: the ITI-55 recorder
/// [`LogAudit`](crate::ihe::xcpd::LogAudit) and the BALP recorder
/// [`LogFeedAudit`](balp::LogFeedAudit).
pub const AUDIT_TARGET: &str = "ferrofed::audit";
