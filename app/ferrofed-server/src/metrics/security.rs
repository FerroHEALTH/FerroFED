// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The security event counts: every refusal at client authentication by its
//! reason, and every identifier-hygiene event the security log records, by
//! its kind.
//!
//! Each event is counted in the process where it is logged, under the same
//! closed name the log line under `ferrofed::security` carries, and read by
//! an observable counter at each collection, as the integrity incidents are.
//! The labels are an [`Event`] name and a [`Refusal::reason`], both closed
//! sets, so no request value, identifier or token reaches the surface
//! (§5.4.1, §5.4.3, N33). No specification governs metrics: our own design.

use std::sync::atomic::{AtomicU64, Ordering};

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Meter, ObservableCounter};

use crate::auth::refusal::Refusal;

/// The security events, by `event` and, for a refused caller, `reason`;
/// Prometheus `ferrofed_security_events_total`.
pub const SECURITY_EVENTS: &str = "ferrofed.security.events";

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
}

impl Event {
    /// Every event, in declaration order.
    pub const ALL: [Self; 15] = [
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
        }
    }

    /// Counts one event of this kind.
    pub fn record(self) {
        if let Some(counter) = position(&Self::ALL, self).and_then(|at| EVENTS.get(at)) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Returns how many events of this kind the process counted.
    #[must_use]
    pub fn counted(self) -> u64 {
        position(&Self::ALL, self)
            .and_then(|at| EVENTS.get(at))
            .map_or(0, |counter| counter.load(Ordering::Relaxed))
    }
}

/// The `event` value of a refusal at client authentication.
pub const CALLER_REFUSED: &str = "caller-refused";

/// The process-wide count of each [`Event`], in [`Event::ALL`] order.
static EVENTS: [AtomicU64; Event::ALL.len()] = [const { AtomicU64::new(0) }; Event::ALL.len()];

/// The process-wide count of each refusal, in [`Refusal::ALL`] order.
static REFUSALS: [AtomicU64; Refusal::ALL.len()] =
    [const { AtomicU64::new(0) }; Refusal::ALL.len()];

/// Counts one request refused at client authentication for `refusal`.
pub fn refused(refusal: Refusal) {
    if let Some(counter) = position(&Refusal::ALL, refusal).and_then(|at| REFUSALS.get(at)) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Returns how many requests the process refused for `refusal`.
#[must_use]
pub fn refusals(refusal: Refusal) -> u64 {
    position(&Refusal::ALL, refusal)
        .and_then(|at| REFUSALS.get(at))
        .map_or(0, |counter| counter.load(Ordering::Relaxed))
}

/// The position of `item` in `all`, which lists every value of its type.
fn position<T: PartialEq + Copy>(all: &[T], item: T) -> Option<usize> {
    all.iter().position(|candidate| *candidate == item)
}

/// Registers the observable counter of every security event on `meter`,
/// each label value at `0` from the first collection.
#[must_use]
pub fn observe(meter: &Meter) -> ObservableCounter<u64> {
    meter
        .u64_observable_counter(SECURITY_EVENTS)
        .with_description(
            "Security events the gateway logged, by event and, for a refused caller, reason",
        )
        .with_callback(|observer| {
            for refusal in Refusal::ALL {
                observer.observe(
                    refusals(refusal),
                    &[
                        KeyValue::new("event", CALLER_REFUSED),
                        KeyValue::new("reason", refusal.reason()),
                    ],
                );
            }
            for event in Event::ALL {
                observer.observe(event.counted(), &[KeyValue::new("event", event.as_str())]);
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{CALLER_REFUSED, Event};
    use crate::auth::refusal::Refusal;

    #[test]
    fn every_event_and_reason_is_distinct() {
        let events: BTreeSet<&str> = Event::ALL.iter().map(|event| event.as_str()).collect();
        assert_eq!(Event::ALL.len(), events.len());
        assert!(!events.contains(CALLER_REFUSED));
        let reasons: BTreeSet<&str> = Refusal::ALL
            .iter()
            .map(|refusal| refusal.reason())
            .collect();
        assert_eq!(Refusal::ALL.len(), reasons.len());
    }

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
