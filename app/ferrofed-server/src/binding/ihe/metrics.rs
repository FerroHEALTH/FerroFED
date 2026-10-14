// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The IHE binding's instruments on the metrics surface.
//!
//! They count the ITI-93 messages of the PMIR identity feed, by `result`; the
//! audit spools are read by the instruments every build carries
//! ([`crate::audit::metrics`]). No specification governs metrics: our own
//! design.

use std::fmt;

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Meter};

use crate::metrics::Metrics;

/// The ITI-93 messages the identity feed received, by `result` (`applied`,
/// `refused` or `unauthenticated`); Prometheus
/// `ferrofed_identity_feed_messages_total`.
pub const IDENTITY_FEED_MESSAGES: &str = "ferrofed.identity_feed.messages";

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
}

impl fmt::Debug for Instruments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instruments").finish_non_exhaustive()
    }
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
        Self { identity_feed }
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
