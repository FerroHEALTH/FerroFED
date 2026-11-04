// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The security event counter: every refusal at client authentication by its
//! reason, and every identifier-hygiene event the security log records, by
//! its kind.
//!
//! Each event is counted in the process where it is logged ([`Event`] and
//! [`Refusal`] keep their own counts), under the same closed name the log
//! line under `ferrofed::security` carries, and read by an observable counter
//! at each collection, as the integrity incidents are.
//! The labels are an [`Event`] name and a [`Refusal::reason`], both closed
//! sets, so no request value, identifier or token reaches the surface
//! (§5.4.1, §5.4.3, N33). No specification governs metrics: our own design.

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Meter, ObservableCounter};

use crate::auth::refusal::Refusal;
use crate::security::Event;

/// The security events, by `event` and, for a refused caller, `reason`;
/// Prometheus `ferrofed_security_events_total`.
pub const SECURITY_EVENTS: &str = "ferrofed.security.events";

/// The `event` value of a refusal at client authentication.
pub const CALLER_REFUSED: &str = "caller-refused";

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
                    refusal.counted(),
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

    use super::CALLER_REFUSED;
    use crate::auth::refusal::Refusal;
    use crate::security::Event;

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
}
