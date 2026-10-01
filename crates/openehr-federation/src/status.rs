// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-query endpoint status vocabulary of §11.1 (N16).

use std::fmt;

use serde::{Deserialize, Serialize};

/// What happened to one endpoint during one query (§11.1, N16).
///
/// The specification fixes this set as closed, and the schema's
/// `endpointQueryStatus` enum carries exactly these values; the type is
/// `#[non_exhaustive]` because a later release of the specification may add
/// one. It is the per-query vocabulary, which is a different thing from the
/// standing membership status of the `OPTIONS` body (§7a.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum EndpointStatus {
    /// Queried and responded.
    Active,
    /// A known node that was not reachable.
    Offline,
    /// Reachable, but no response within the gateway's timeout.
    TimeOut,
    /// Reached, and answered with a failure instead of a result set.
    NodeError,
    /// The cross-reference service returned no local `ehr_id` for the patient
    /// at this node (§5, N6).
    NotResolved,
    /// Consent did not permit this node's inclusion, decided by a Step-1
    /// pre-filter (N27a) or by the node itself (N27).
    ConsentDenied,
    /// Ruled out by a decision about this node: a directive that did not name
    /// it, or an operator policy.
    Excluded,
    /// A registry member that localization did not return as a candidate
    /// (§14); nothing decided about it.
    NotLocalized,
}

impl EndpointStatus {
    /// Every status, in the order §11.1 lists them.
    pub const ALL: [Self; 8] = [
        Self::Active,
        Self::Offline,
        Self::TimeOut,
        Self::NodeError,
        Self::NotResolved,
        Self::ConsentDenied,
        Self::Excluded,
        Self::NotLocalized,
    ];

    /// The status as the wire spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Offline => "offline",
            Self::TimeOut => "time-out",
            Self::NodeError => "node-error",
            Self::NotResolved => "not-resolved",
            Self::ConsentDenied => "consent-denied",
            Self::Excluded => "excluded",
            Self::NotLocalized => "not-localized",
        }
    }

    /// Whether the endpoint was in scope for the query (§11.1 "What in scope
    /// means"): every status but `excluded` and `not-localized`.
    #[must_use]
    pub const fn is_in_scope(self) -> bool {
        !matches!(self, Self::Excluded | Self::NotLocalized)
    }

    /// Whether the outcome must carry `error` (§9.5, N40).
    #[must_use]
    pub const fn requires_error(self) -> bool {
        matches!(
            self,
            Self::Offline | Self::TimeOut | Self::NodeError | Self::NotResolved
        )
    }

    /// Whether a request was always dispatched to the endpoint, so the
    /// outcome must carry `latency_ms` (§9.5, N40).
    ///
    /// `consent-denied` is dispatched only when the node itself refused; a
    /// Step-1 pre-filter settles it before any request exists.
    #[must_use]
    pub const fn requires_latency(self) -> bool {
        matches!(
            self,
            Self::Active | Self::Offline | Self::TimeOut | Self::NodeError
        )
    }

    /// Whether the outcome fails the whole query under the all-or-nothing
    /// default (§11.1, §11.4, N37): an in-scope node that was asked and did
    /// not answer, or answered with a failure.
    #[must_use]
    pub const fn fails_all_or_nothing(self) -> bool {
        matches!(self, Self::Offline | Self::TimeOut | Self::NodeError)
    }
}

impl fmt::Display for EndpointStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
