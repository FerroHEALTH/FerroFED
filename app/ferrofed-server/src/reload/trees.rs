// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration trees a reloader keeps for the running configuration
//! ([`crate::running`]): the one the process started with, and the one the
//! last reload of the file applied. No specification governs this: our own
//! design.

use std::sync::{Arc, PoisonError};

use ferrofed_registry::revision::RevisionId;

use super::Reloader;
use crate::revision::service::Revisions;

impl Reloader {
    /// Returns this reloader with `tree`, the configuration tree the process
    /// started with, the file's and the environment's merged, which the
    /// running configuration reads under `config.ui = "off"`.
    #[must_use]
    pub fn with_tree(mut self, tree: toml::Table) -> Self {
        self.tree = tree;
        self
    }

    /// The configuration tree the process started with.
    #[must_use]
    pub fn boot_tree(&self) -> &toml::Table {
        &self.tree
    }

    /// The revision the process started on, under `config.ui = "managed"`.
    #[must_use]
    pub fn boot_revision(&self) -> Option<RevisionId> {
        self.boot_revision
    }

    /// The configuration tree the last reload of the file applied, under
    /// `config.ui = "off"`; `None` before the first.
    #[must_use]
    pub fn loaded_tree(&self) -> Option<toml::Table> {
        self.loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The configuration revisions, under `config.ui = "managed"`.
    #[must_use]
    pub fn revisions(&self) -> Option<&Arc<Revisions>> {
        self.revisions.as_ref()
    }
}
