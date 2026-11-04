// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration revisions a reload reports to.
//!
//! Under `config.ui = "managed"` a reload records the file's configuration as
//! a revision, runs the one the store marks current, and marks what it ran.
//! The reload owns this seam and the configuration revisions implement it,
//! so the reload reads no revision store of its own. No specification
//! governs this: our own design.

use std::fmt;

use ferrofed_operator_api::revision::RevisionId;

use crate::config::Config;

/// Why the configuration revisions refused what a reload asked of them.
///
/// It reads as its cause and hands on the cause's own source, so the chain
/// reads as it did before the seam.
#[derive(Debug)]
pub struct LogError(Box<dyn std::error::Error + Send + Sync>);

impl LogError {
    /// The refusal `cause` stands for.
    pub fn new(cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Box::new(cause))
    }
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for LogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

/// The revision a file load gives the gateway to run, and the mark the load
/// read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileRevision {
    /// The revision.
    pub revision: RevisionId,
    /// Whether the load recorded it as a new revision by the file.
    pub recorded: bool,
    /// The revision the store marked running when the load read it, from
    /// which a reload that applies a recorded revision moves the mark.
    pub previous: Option<RevisionId>,
}

/// The configuration a file load gives the gateway to run.
#[derive(Debug)]
#[non_exhaustive]
pub struct FileLoaded {
    /// The configuration, the revision beside the file's bootstrap core.
    pub config: Config,
    /// The revision it is, and whether the load recorded it.
    pub file: FileRevision,
}

/// The configuration revisions a reload under `config.ui = "managed"`
/// records the file in and marks what it runs in.
pub trait RevisionLog: fmt::Debug + Send + Sync {
    /// The revision this replica runs, when it runs one.
    fn running(&self) -> Option<RevisionId>;

    /// Records the configuration the file gave, `tree`, and returns the
    /// configuration to run with the revision it is.
    ///
    /// # Errors
    ///
    /// The [`LogError`] of a configuration the revisions refuse or a store
    /// that cannot be read.
    fn file_loaded(&self, tree: &toml::Table) -> Result<FileLoaded, LogError>;

    /// Records that a reload applied `file`, leaving `needs_restart` for the
    /// next start.
    ///
    /// # Errors
    ///
    /// The [`LogError`] of a store that cannot be written.
    fn file_applied(
        &self,
        file: FileRevision,
        needs_restart: &[&'static str],
    ) -> Result<(), LogError>;

    /// Records that a reload refused `file`, for the failure of class
    /// `class` that `reason` states.
    fn file_refused(&self, file: FileRevision, class: &str, reason: &str);

    /// Records that this replica runs revision `id`.
    fn ran(&self, id: RevisionId);

    /// The configuration tree revision `id` holds.
    ///
    /// # Errors
    ///
    /// The [`LogError`] of a revision not held or a store that cannot be
    /// read.
    fn held(&self, id: RevisionId) -> Result<toml::Table, LogError>;
}
