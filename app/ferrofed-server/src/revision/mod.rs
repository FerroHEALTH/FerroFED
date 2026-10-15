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

use ferrofed_registry::resource::Resource;
use ferrofed_registry::revision::{Author, Current, Origin, Revision, RevisionId, Signed};
use jiff::Timestamp;

use crate::admin::Operator;
use crate::config::revisions::Store;

pub mod embedded;
pub mod policy;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod resources;
pub mod routes;
pub mod service;
pub mod tree;
pub mod watch;

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
    /// The credentials and binding sections the console owns in it.
    pub console: Vec<Resource>,
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
            console: self.console,
        }
    }
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

    /// The approvals revision `id` holds, each operator once.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the store cannot be read or holds a damaged row.
    fn approvals(&self, id: RevisionId) -> Result<Vec<Signed>, StoreError>;

    /// Records `approval` of revision `id`, once per operator: a second
    /// approval by the same operator changes nothing.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the store cannot be read or written.
    fn approve(&self, id: RevisionId, approval: &Signed) -> Result<(), StoreError>;
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
