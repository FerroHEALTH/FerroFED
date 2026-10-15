// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's configuration revisions as its admin listener writes them
//! and the operator console reads them.
//!
//! One definition serves both sides, so a body the gateway answers and the
//! console reads cannot drift apart: a revision ([`Revision`]), the mark of
//! the running one ([`Current`]), the standing of the store and this
//! replica ([`Standing`]), the keys two revisions differ by ([`Diff`]), and
//! the answer to an apply or a rollback ([`Applied`]). No specification
//! governs this: our own design.

use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// The number of a revision: `1` for the first one a store holds, and one
/// more than its base for every later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RevisionId(NonZeroU64);

impl RevisionId {
    /// The first revision a store holds.
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    /// The revision numbered `number`, when it is not zero.
    #[must_use]
    pub const fn new(number: u64) -> Option<Self> {
        match NonZeroU64::new(number) {
            Some(number) => Some(Self(number)),
            None => None,
        }
    }

    /// The number of the revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// The revision saved on top of `base`: [`RevisionId::FIRST`] on none,
    /// and `None` past the last number.
    #[must_use]
    pub fn after(base: Option<Self>) -> Option<Self> {
        match base {
            None => Some(Self::FIRST),
            Some(base) => base.0.checked_add(1).map(Self),
        }
    }
}

impl fmt::Display for RevisionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RevisionId {
    type Err = std::num::ParseIntError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse::<NonZeroU64>().map(Self)
    }
}

/// Where a revision came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Origin {
    /// The configuration file and environment, read at a start or a reload.
    File,
    /// An operator's save on the admin listener.
    Operator,
    /// An operator's rollback, restoring an earlier revision's content.
    Rollback,
}

/// Who made a revision, or applied one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Author {
    /// The configuration file, read by the gateway itself.
    File,
    /// An operator the client authentication verified: the issuer and the
    /// subject it put in the token.
    Operator {
        /// The issuer that vouched for the operator.
        issuer: String,
        /// The operator's subject at that issuer.
        subject: String,
    },
    /// A loopback peer with no credential, admitted under
    /// `profile = "development"` alone.
    DevelopmentLoopback,
}

/// An instant, written as RFC 3339 text.
mod instant {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Writes `at` as RFC 3339 text.
    pub(super) fn serialize<S: Serializer>(
        at: &Timestamp,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(at)
    }

    /// Reads RFC 3339 text as an instant.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Timestamp, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// A revision a store holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    /// Its number.
    pub id: RevisionId,
    /// The revision it was saved on top of; none for the first.
    pub base: Option<RevisionId>,
    /// Where it came from.
    pub origin: Origin,
    /// The revision whose content a rollback restores.
    pub restores: Option<RevisionId>,
    /// Who made it.
    pub author: Author,
    /// When it was saved.
    #[serde(with = "instant")]
    pub at: Timestamp,
    /// Why, as its author wrote it.
    pub reason: String,
    /// The configuration less its bootstrap core, as TOML text.
    pub toml: String,
}

/// What applying a revision did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Outcome {
    /// Every setting it changes is in effect.
    Applied,
    /// The settings a reload applies are in effect, and the ones in
    /// [`Current::needs_restart`] take effect at the next restart.
    PendingRestart,
}

/// The revision the gateway runs, as the store marks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Current {
    /// The revision in effect.
    pub revision: RevisionId,
    /// The revision that was in effect before it, which a rollback restores.
    pub previous: Option<RevisionId>,
    /// What applying it did.
    pub outcome: Outcome,
    /// The changed settings that take effect only at a restart, by key.
    pub needs_restart: Vec<String>,
    /// Who applied it.
    pub applied_by: Author,
    /// When it was applied.
    #[serde(with = "instant")]
    pub applied_at: Timestamp,
}

/// How one key differs between two revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Change {
    /// The key is set only in the later revision.
    Added,
    /// The key is set only in the earlier revision.
    Removed,
    /// The key is set in both, to different values.
    Changed,
}

impl Change {
    /// The change as the body writes it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "changed",
        }
    }
}

/// One key that differs between two revisions, with its values as TOML
/// writes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Difference {
    /// The dotted key, an array of tables indexed as `table[0]`.
    pub key: String,
    /// How the key differs.
    pub change: Change,
    /// The value in the earlier revision, when it sets one.
    pub from: Option<String>,
    /// The value in the later revision, when it sets one.
    pub to: Option<String>,
}

/// The body of `GET /admin/config/revisions/{id}/diff/{other}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diff {
    /// The earlier revision.
    pub from: RevisionId,
    /// The later revision.
    pub to: RevisionId,
    /// The keys that differ, in key order.
    pub changes: Vec<Difference>,
}

/// The body of `GET /admin/config`: the store's standing and this
/// replica's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Standing {
    /// Always `managed`: the route exists only then.
    pub ui: String,
    /// The newest revision.
    pub head: Option<RevisionId>,
    /// The mark of the running revision.
    pub current: Option<Current>,
    /// The revision this replica runs, once it runs one.
    pub running: Option<RevisionId>,
    /// Whether this replica runs the revision the store marks running.
    pub in_step: bool,
    /// The revision marked running that this replica refused, while it
    /// does not run it.
    pub refused: Option<RevisionId>,
}

/// The answer to an apply or a rollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Applied {
    /// The revision applied.
    pub revision: Revision,
    /// The mark the store now holds.
    pub current: Current,
}
