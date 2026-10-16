// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The count of the dispensations the FHIR face received, by how each ended
//! (Regulation (EU) 2025/327 Art 14(1)(c)).
//!
//! Each dispensation that reached the owner lookup is counted once under
//! its [`Outcome`], a closed set, so a dispensation no member owns is
//! counted with no value: no prescription or patient identifier and no
//! member reaches the label. The counts are read by an observable counter
//! at each collection, as the security events are. No specification governs
//! metrics: our own design.

use std::sync::atomic::{AtomicU64, Ordering};

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Meter, ObservableCounter};

/// The dispensations received, by `outcome`; Prometheus
/// `ferrofed_fhir_dispensations_total`.
pub const DISPENSATIONS: &str = "ferrofed.fhir.dispensations";

/// How a received dispensation ended once its lookup was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The owner created it.
    Created,
    /// No member holds its patient or its prescription, or none may say
    /// so.
    Unowned,
    /// Two members hold its prescription.
    Claimed,
    /// One member holds its prescription twice.
    Several,
    /// Its prescriptions are held by different members, or a reference and
    /// an identifier name different ones.
    Unroutable,
    /// A member that could own it did not answer the lookup.
    Silent,
    /// The owner refused it.
    Refused,
    /// The owner did not answer, so whether it wrote it is unknown.
    Unknown,
    /// It failed for another reason, with nothing written.
    Failed,
}

impl Outcome {
    /// Every outcome, in declaration order.
    pub const ALL: [Self; 9] = [
        Self::Created,
        Self::Unowned,
        Self::Claimed,
        Self::Several,
        Self::Unroutable,
        Self::Silent,
        Self::Refused,
        Self::Unknown,
        Self::Failed,
    ];

    /// The label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Unowned => "unowned",
            Self::Claimed => "claimed",
            Self::Several => "several",
            Self::Unroutable => "unroutable",
            Self::Silent => "silent",
            Self::Refused => "refused",
            Self::Unknown => "unknown",
            Self::Failed => "failed",
        }
    }

    /// Counts one dispensation that ended so.
    pub fn record(self) {
        if let Some(counter) = self.counter() {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Returns how many dispensations the process counted that ended so.
    #[must_use]
    pub fn counted(self) -> u64 {
        self.counter()
            .map_or(0, |counter| counter.load(Ordering::Relaxed))
    }

    /// The process-wide counter of this outcome.
    fn counter(self) -> Option<&'static AtomicU64> {
        Self::ALL
            .iter()
            .position(|outcome| *outcome == self)
            .and_then(|at| COUNTS.get(at))
    }
}

/// The process-wide count of each [`Outcome`], in [`Outcome::ALL`] order.
static COUNTS: [AtomicU64; Outcome::ALL.len()] = [const { AtomicU64::new(0) }; Outcome::ALL.len()];

/// Registers the observable counter of every outcome on `meter`, each at
/// `0` from the first collection.
#[must_use]
pub fn observe(meter: &Meter) -> ObservableCounter<u64> {
    meter
        .u64_observable_counter(DISPENSATIONS)
        .with_description(
            "Dispensations the FHIR face received that reached the owner lookup, by outcome",
        )
        .with_callback(|observer| {
            for outcome in Outcome::ALL {
                observer.observe(
                    outcome.counted(),
                    &[KeyValue::new("outcome", outcome.as_str())],
                );
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::Outcome;

    #[test]
    fn every_outcome_is_distinct_and_counted_alone() {
        let labels: BTreeSet<&str> = Outcome::ALL
            .iter()
            .map(|outcome| outcome.as_str())
            .collect();
        assert_eq!(Outcome::ALL.len(), labels.len());
        let (before, other) = (Outcome::Unowned.counted(), Outcome::Created.counted());
        Outcome::Unowned.record();
        assert_eq!(before + 1, Outcome::Unowned.counted());
        assert_eq!(other, Outcome::Created.counted());
    }
}
