// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit spool instruments on the metrics surface.
//!
//! They read every running audit trail at each collection, summed, with no
//! label at all, so a record's content never reaches a metric. No
//! specification governs metrics: our own design.

use ihe_iti::atna::forwarder::Status;
use opentelemetry::metrics::{Meter, ObservableCounter, ObservableGauge};

/// The ITI-20 audit messages waiting in the spool for the audit repository;
/// Prometheus `ferrofed_audit_spool_events`.
pub const AUDIT_SPOOL_EVENTS: &str = "ferrofed.audit.spool.events";

/// The bytes of those messages; Prometheus `ferrofed_audit_spool_bytes`.
pub const AUDIT_SPOOL_BYTES: &str = "ferrofed.audit.spool.bytes";

/// The ITI-20 audit messages delivered to the audit repository; Prometheus
/// `ferrofed_audit_delivered_total`.
pub const AUDIT_DELIVERED: &str = "ferrofed.audit.delivered";

/// The failed attempts to deliver to the audit repository, each followed by
/// a backoff; Prometheus `ferrofed_audit_retries_total`.
pub const AUDIT_RETRIES: &str = "ferrofed.audit.retries";

/// The ITI-20 audit messages held in the spool's quarantine; Prometheus
/// `ferrofed_audit_quarantined`.
pub const AUDIT_QUARANTINED: &str = "ferrofed.audit.quarantined";

/// The ITI-20 audit messages the spool refused for want of room, the
/// messages queued for a write counted with those stored; Prometheus
/// `ferrofed_audit_refused_total`.
pub const AUDIT_REFUSED: &str = "ferrofed.audit.refused";

/// The audit spool instruments, kept for the life of the provider: their
/// callbacks read the audit spools at each collection.
pub(crate) struct Instruments {
    _events: ObservableGauge<u64>,
    _bytes: ObservableGauge<u64>,
    _quarantined: ObservableGauge<u64>,
    _delivered: ObservableCounter<u64>,
    _retries: ObservableCounter<u64>,
    _refused: ObservableCounter<u64>,
}

impl Instruments {
    /// Returns the instruments over `meter`.
    pub(crate) fn new(meter: &Meter) -> Self {
        let gauge = |name: &'static str, description: &'static str, read: fn(&Status) -> u64| {
            meter
                .u64_observable_gauge(name)
                .with_description(description)
                .with_callback(move |observer| {
                    if let Some(total) = summed(read) {
                        observer.observe(total, &[]);
                    }
                })
                .build()
        };
        let counter = |name: &'static str, description: &'static str, read: fn(&Status) -> u64| {
            meter
                .u64_observable_counter(name)
                .with_description(description)
                .with_callback(move |observer| {
                    if let Some(total) = summed(read) {
                        observer.observe(total, &[]);
                    }
                })
                .build()
        };
        Self {
            _events: gauge(
                AUDIT_SPOOL_EVENTS,
                "ITI-20 audit messages waiting in the spool for the audit repository",
                |status| u64::try_from(status.depth.waiting()).unwrap_or(u64::MAX),
            ),
            _bytes: gauge(
                AUDIT_SPOOL_BYTES,
                "Bytes of the ITI-20 audit messages the spool holds, quarantine included",
                |status| status.depth.bytes,
            ),
            _quarantined: gauge(
                AUDIT_QUARANTINED,
                "ITI-20 audit messages held in the spool's quarantine",
                |status| u64::try_from(status.depth.quarantined).unwrap_or(u64::MAX),
            ),
            _delivered: counter(
                AUDIT_DELIVERED,
                "ITI-20 audit messages delivered to the audit repository",
                |status| status.delivered,
            ),
            _retries: counter(
                AUDIT_RETRIES,
                "Failed attempts to deliver to the audit repository, each followed by a backoff",
                |status| status.retries,
            ),
            _refused: counter(
                AUDIT_REFUSED,
                "ITI-20 audit messages the spool refused for want of room, queued writes counted",
                |status| status.refused,
            ),
        }
    }
}

/// `read` summed over every running audit trail, or `None` with none.
fn summed(read: fn(&Status) -> u64) -> Option<u64> {
    let statuses = super::statuses();
    (!statuses.is_empty()).then(|| statuses.iter().map(read).fold(0, u64::saturating_add))
}
