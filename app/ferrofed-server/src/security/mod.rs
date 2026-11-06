// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the security events are logged and counted.
//!
//! The tracing target every lane writes them under, the directive that keeps
//! it on, and the closed set of events each lane counts as it logs one, which
//! the metrics surface reads ([`crate::metrics::security`]). No specification
//! governs this: our own design.

pub mod logged;

use std::sync::atomic::{AtomicU64, Ordering};

/// The tracing target of every security event, so an operator can route them
/// apart from the request log.
pub const TARGET: &str = "ferrofed::security";

/// The directive the console writes the security events through whatever
/// `telemetry.filter` says: [`TARGET`] at `info` and above.
pub const DIRECTIVE: &str = "ferrofed::security=info";

/// A security event the gateway counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// An issuer's key set could not be had, so its tokens cannot be
    /// verified.
    KeySetUnavailable,
    /// An issuer's introspection endpoint did not answer.
    IntrospectionUnavailable,
    /// A federated query was refused before dispatch (§5.4.3).
    AqlRefused,
    /// A patient predicate was consumed as resolution input and stripped.
    PatientPredicateStripped,
    /// The subject query parameters were consumed as resolution input.
    SubjectParametersConsumed,
    /// The outbound gate stopped a request that would have carried a
    /// patient identifier (§5.4.1, N33).
    OutboundGateStopped,
    /// A query parameter the operation does not admit was refused.
    QueryParameterRefused,
    /// A declared value that does not match its declared kind was refused.
    ParameterValueRefused,
    /// A read whose `ehr_id` is no UUID was refused the ask-all probe.
    EhrIdProbeRefused,
    /// A stored-query definition naming its patient by a literal was refused.
    DefinitionSubjectLiteral,
    /// A definition the store holds was refused on a read.
    HeldDefinitionRefused,
    /// A request reached beyond the patient its grant is confined to.
    PatientConfinement,
    /// The patient of a confined grant could not be resolved.
    PatientContextUnavailable,
    /// A write action on the admin listener was refused at its
    /// authentication.
    AdminWriteRefused,
    /// A scrape of the admin listener's `GET /metrics` carried no scrape
    /// token, or another one.
    ScrapeRefused,
    /// An admitted operator ran a write action on the admin listener.
    AdminWriteAdmitted,
    /// The record of a write a member was asked to make could not be stored
    /// in the place reserved for it before the write, so the write stands
    /// unrecorded (Regulation (EU) 2025/327 Annex II 3.2).
    WriteUnrecorded,
    /// A national contact point's request named a patient, or reached a
    /// resource, outside the patient its treatment relationship
    /// confirmation binds (eHDSI XCA Profile 8.0.1 §4.2.1, §4.2.2).
    TrcPatientMismatch,
    /// The patient a national contact point's treatment relationship
    /// confirmation binds could not be resolved at every member, so nothing
    /// was sent (eHDSI XCA Profile 8.0.1 §4.2.1).
    TrcPatientUnresolved,
    /// A health professional a national contact point relays reached the
    /// limit of emergency accesses in the window, so the access was denied
    /// (eHDSI Section II Security Services 10.0.0 §2.5; `NCPeH` Architecture
    /// Specifications 10.0.0 §5.1.9).
    EmergencyLimitReached,
}

impl Event {
    /// Every event, in declaration order.
    pub const ALL: [Self; 20] = [
        Self::KeySetUnavailable,
        Self::IntrospectionUnavailable,
        Self::AqlRefused,
        Self::PatientPredicateStripped,
        Self::SubjectParametersConsumed,
        Self::OutboundGateStopped,
        Self::QueryParameterRefused,
        Self::ParameterValueRefused,
        Self::EhrIdProbeRefused,
        Self::DefinitionSubjectLiteral,
        Self::HeldDefinitionRefused,
        Self::PatientConfinement,
        Self::PatientContextUnavailable,
        Self::AdminWriteRefused,
        Self::ScrapeRefused,
        Self::AdminWriteAdmitted,
        Self::WriteUnrecorded,
        Self::TrcPatientMismatch,
        Self::TrcPatientUnresolved,
        Self::EmergencyLimitReached,
    ];

    /// The label value, the `event` field of the security log line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeySetUnavailable => "key-set-unavailable",
            Self::IntrospectionUnavailable => "introspection-unavailable",
            Self::AqlRefused => "aql-refused",
            Self::PatientPredicateStripped => "patient-predicate-stripped",
            Self::SubjectParametersConsumed => "subject-parameters-consumed",
            Self::OutboundGateStopped => "outbound-gate-stopped",
            Self::QueryParameterRefused => "query-parameter-refused",
            Self::ParameterValueRefused => "parameter-value-refused",
            Self::EhrIdProbeRefused => "ehr-id-probe-refused",
            Self::DefinitionSubjectLiteral => "definition-subject-literal",
            Self::HeldDefinitionRefused => "held-definition-refused",
            Self::PatientConfinement => "patient-confinement",
            Self::PatientContextUnavailable => "patient-context-unavailable",
            Self::AdminWriteRefused => "admin-write-refused",
            Self::ScrapeRefused => "scrape-refused",
            Self::AdminWriteAdmitted => "admin-write-admitted",
            Self::WriteUnrecorded => "write-unrecorded",
            Self::TrcPatientMismatch => "trc-patient-mismatch",
            Self::TrcPatientUnresolved => "trc-patient-unresolved",
            Self::EmergencyLimitReached => "emergency-limit-reached",
        }
    }

    /// Counts one event of this kind.
    pub fn record(self) {
        if let Some(counter) = Self::ALL
            .iter()
            .position(|kind| *kind == self)
            .and_then(|at| EVENTS.get(at))
        {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Returns how many events of this kind the process counted.
    #[must_use]
    pub fn counted(self) -> u64 {
        Self::ALL
            .iter()
            .position(|kind| *kind == self)
            .and_then(|at| EVENTS.get(at))
            .map_or(0, |counter| counter.load(Ordering::Relaxed))
    }
}

/// The process-wide count of each [`Event`], in [`Event::ALL`] order.
static EVENTS: [AtomicU64; Event::ALL.len()] = [const { AtomicU64::new(0) }; Event::ALL.len()];

#[cfg(test)]
mod tests {
    use super::Event;

    #[test]
    fn a_recorded_event_is_counted_under_its_own_kind() {
        let (before, other) = (
            Event::OutboundGateStopped.counted(),
            Event::AqlRefused.counted(),
        );
        Event::OutboundGateStopped.record();
        assert_eq!(before + 1, Event::OutboundGateStopped.counted());
        assert_eq!(other, Event::AqlRefused.counted());
    }
}
