// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration revisions, under `config.ui = "managed"`.
//!
//! A revision is a complete configuration less its bootstrap core
//! ([`tree::CORE`]), held as TOML text with every secret only as a `_file`
//! reference ([`tree::inline_secrets`]). Each one records who saved it,
//! when and why, and the revision it was based on; the store keeps them all
//! as history and marks the one the gateway runs as [`Current`].
//!
//! A save names the revision it is based on and is refused when another
//! revision was saved since ([`Saved::Stale`]), and the current mark moves
//! only from the revision the writer read ([`Swapped::Stale`]), so two
//! operators, or two replicas sharing one store, never overwrite each other.
//! The embedded `redb` file serves one gateway process ([`embedded`]); a
//! PostgreSQL database serves replicas that share it (`postgres`, behind the
//! `postgres` feature). [`service::Revisions`] validates, applies and rolls
//! back, [`routes`] serves them on the admin listener, and [`watch`] applies
//! on each replica the revision another one marked running. No
//! specification governs this: our own design.

use std::error::Error;
use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::admin::Operator;
use crate::config::revisions::Store;

pub mod embedded;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod routes;
pub mod service;
pub mod tree;
pub mod watch;

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

impl From<&Operator> for Author {
    fn from(operator: &Operator) -> Self {
        match (operator.issuer(), operator.subject()) {
            (Some(issuer), Some(subject)) => Self::Operator {
                issuer: issuer.to_owned(),
                subject: subject.to_owned(),
            },
            _ => Self::DevelopmentLoopback,
        }
    }
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

/// A revision to save, before the store numbers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    /// Where it comes from.
    pub origin: Origin,
    /// The revision whose content a rollback restores.
    pub restores: Option<RevisionId>,
    /// Who makes it.
    pub author: Author,
    /// When it is saved.
    pub at: Timestamp,
    /// Why.
    pub reason: String,
    /// The configuration less its bootstrap core, as TOML text.
    pub toml: String,
}

impl Draft {
    /// The revision this draft is once the store numbers it `id` over
    /// `base`.
    #[must_use]
    pub fn numbered(self, id: RevisionId, base: Option<RevisionId>) -> Revision {
        Revision {
            id,
            base,
            origin: self.origin,
            restores: self.restores,
            author: self.author,
            at: self.at,
            reason: self.reason,
            toml: self.toml,
        }
    }
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

/// What a save did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub enum Saved {
    /// The revision is stored.
    Stored(Revision),
    /// The base named is not the newest revision: another one was saved
    /// since, and nothing is stored.
    Stale {
        /// The newest revision the store holds.
        head: Option<RevisionId>,
    },
    /// The numbers are exhausted, and nothing is stored.
    Exhausted,
}

/// What a move of the current mark did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub enum Swapped {
    /// The mark moved.
    Swapped,
    /// The mark is not where the writer read it, and it did not move.
    Stale {
        /// The revision the store marks current.
        current: Option<RevisionId>,
    },
}

/// A revision store that could not be read or written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// The backend could not read or write.
    #[error("the configuration revision store could not be read or written")]
    Backend(#[source] Box<dyn Error + Send + Sync>),
    /// A held row does not read as a revision: the store is damaged.
    #[error("the configuration revision store holds a row that is not a revision")]
    Corrupt(#[source] Box<dyn Error + Send + Sync>),
    /// The store records a newer layout than this binary knows.
    #[error(
        "the configuration revision store is at schema version {found}, and this FerroFED knows versions up to {supported}: a newer FerroFED wrote it"
    )]
    Newer {
        /// The version the store records.
        found: u32,
        /// The newest version this binary knows.
        supported: u32,
    },
}

/// The durable store of configuration revisions.
///
/// Every call may block, so it is made where a thread may.
pub trait RevisionStore: fmt::Debug + Send + Sync {
    /// Stores `draft` as the revision after `base`, when `base` is the
    /// newest revision the store holds (none for an empty store), in one
    /// atomic step.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the store cannot be read or written.
    fn save(&self, base: Option<RevisionId>, draft: Draft) -> Result<Saved, StoreError>;

    /// The revision numbered `id`, when the store holds it.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the store cannot be read or holds a damaged row.
    fn revision(&self, id: RevisionId) -> Result<Option<Revision>, StoreError>;

    /// Every revision, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the store cannot be read or holds a damaged row.
    fn history(&self) -> Result<Vec<Revision>, StoreError>;

    /// The newest revision, when the store holds one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the store cannot be read.
    fn head(&self) -> Result<Option<RevisionId>, StoreError>;

    /// The revision marked current, when one is.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the store cannot be read or holds a damaged row.
    fn current(&self) -> Result<Option<Current>, StoreError>;

    /// Marks `next` current when the store marks `expected` current now
    /// (none for a store with no mark), in one atomic step.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the store cannot be read or written.
    fn swap_current(
        &self,
        expected: Option<RevisionId>,
        next: &Current,
    ) -> Result<Swapped, StoreError>;
}

/// Opens the store `store` names.
///
/// # Errors
///
/// The [`StoreError`] of the backend's own `open`, and
/// [`StoreError::Backend`] for a PostgreSQL store in a build without the
/// `postgres` feature, which configuration refuses first.
pub fn open(store: &Store) -> Result<Box<dyn RevisionStore>, StoreError> {
    match store {
        Store::Redb(path) => Ok(Box::new(embedded::RedbRevisions::open(path)?)),
        #[cfg(feature = "postgres")]
        Store::Postgres(url) => Ok(Box::new(postgres::PostgresRevisions::open(url)?)),
        #[cfg(not(feature = "postgres"))]
        Store::Postgres(_) => Err(StoreError::Backend(Box::new(
            crate::config::revisions::RevisionsFault::Unavailable {
                backend: store.backend(),
            },
        ))),
    }
}
