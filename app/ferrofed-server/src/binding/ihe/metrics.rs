// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The IHE binding's instruments on the metrics surface.
//!
//! They count the ITI-93 messages of the PMIR identity feed, by `result`, and
//! read the ITI-20 audit spools, with no label at all. No specification
//! governs metrics: our own design.

use std::fmt;

use ihe_iti::atna::forwarder::Status;
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Meter, ObservableCounter, ObservableGauge};

use crate::metrics::Metrics;

/// The ITI-93 messages the identity feed received, by `result` (`applied`,
/// `refused` or `unauthenticated`); Prometheus
/// `ferrofed_identity_feed_messages_total`.
pub const IDENTITY_FEED_MESSAGES: &str = "ferrofed.identity_feed.messages";

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

/// How an ITI-93 message ended, the `result` label of
/// [`IDENTITY_FEED_MESSAGES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedResult {
    /// The message was applied to the resolution bindings.
    Applied,
    /// The message does not hold to the PMIR profiles, and nothing was
    /// applied.
    Refused,
    /// The message did not carry the feed token, and nothing was applied.
    Unauthenticated,
    /// The message's audit record could not be stored, and nothing was
    /// applied (PMIR §2:3.93.5.1).
    AuditFailed,
}

impl FeedResult {
    /// Every result, in declaration order.
    pub const ALL: [Self; 4] = [
        Self::Applied,
        Self::Refused,
        Self::Unauthenticated,
        Self::AuditFailed,
    ];

    /// The label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Refused => "refused",
            Self::Unauthenticated => "unauthenticated",
            Self::AuditFailed => "audit-failed",
        }
    }
}

/// The IHE binding's instruments, created once over the metrics surface's
/// meter and kept for the life of its provider.
pub struct Instruments {
    identity_feed: Counter<u64>,
    /// Kept for the life of the provider: their callbacks read the audit
    /// spools at each collection.
    _audit: AuditInstruments,
}

impl fmt::Debug for Instruments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instruments").finish_non_exhaustive()
    }
}

/// The audit spool instruments, kept for the life of the provider.
struct AuditInstruments {
    _events: ObservableGauge<u64>,
    _bytes: ObservableGauge<u64>,
    _quarantined: ObservableGauge<u64>,
    _delivered: ObservableCounter<u64>,
    _retries: ObservableCounter<u64>,
    _refused: ObservableCounter<u64>,
}

impl Instruments {
    /// Returns the instruments over `meter`, the identity feed's counter
    /// started at zero for every result.
    pub(crate) fn new(meter: &Meter) -> Self {
        let identity_feed = meter
            .u64_counter(IDENTITY_FEED_MESSAGES)
            .with_description("ITI-93 messages the identity feed received, by result")
            .build();
        for result in FeedResult::ALL {
            identity_feed.add(0, &[KeyValue::new("result", result.as_str())]);
        }
        Self {
            identity_feed,
            _audit: audit_instruments(meter),
        }
    }
}

impl Metrics {
    /// Counts an ITI-93 message that ended as `result`.
    pub fn identity_feed(&self, result: FeedResult) {
        self.bindings()
            .ihe
            .identity_feed
            .add(1, &[KeyValue::new("result", result.as_str())]);
    }
}

/// The audit spool instruments, which read every running audit trail at
/// each collection, summed, with no label.
fn audit_instruments(meter: &Meter) -> AuditInstruments {
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
    AuditInstruments {
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

/// `read` summed over every running audit trail, or `None` with none.
fn summed(read: fn(&Status) -> u64) -> Option<u64> {
    let statuses = super::audit::statuses();
    (!statuses.is_empty()).then(|| statuses.iter().map(read).fold(0, u64::saturating_add))
}
