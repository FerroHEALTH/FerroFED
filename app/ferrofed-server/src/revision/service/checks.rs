// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checks every save, apply and rollback shares: the reason a revision
//! records, the outcome of a store's save, and the configuration a revision
//! makes checked as `config check` checks it. No specification governs
//! this: our own design.

use std::collections::BTreeMap;

use ferrofed_registry::resource::Resource;
use ferrofed_registry::revision::{Origin, Revision, RevisionId};

use super::{REASON_LIMIT, Refused, RevisionError, Revisions, Stale, held_table};
use crate::config::Config;
use crate::config::error::Error;
use crate::config::settings::Settings;
use crate::revision::Saved;
use crate::revision::resources::{present, value_at};
use crate::state::{AppState, StateError};

/// `reason`, trimmed, when it is 1 to [`REASON_LIMIT`] characters.
pub(super) fn reason_of(reason: &str) -> Result<String, Refused> {
    let trimmed = reason.trim();
    if trimmed.is_empty() || trimmed.chars().count() > REASON_LIMIT {
        return Err(Refused::Reason);
    }
    Ok(trimmed.to_owned())
}

/// The revision `saved` stored over `base`.
pub(super) fn stored(saved: Saved, base: Option<RevisionId>) -> Result<Revision, RevisionError> {
    match saved {
        Saved::Stored(revision) => Ok(revision),
        Saved::Stale { head } => Err(Stale::Base { named: base, head }.into()),
        Saved::Exhausted => Err(RevisionError::Exhausted),
    }
}

/// The configuration `tree` writes, read as the file is read with no
/// environment beside it.
pub(super) fn configuration(tree: &toml::Table) -> Result<Config, Error> {
    let text = toml::to_string(tree).map_err(|source| Error::assemble(source))?;
    Config::from_sources(Some(&text), &BTreeMap::new())
}

/// Checks `settings` as `config check` does past resolving them.
pub(super) fn checked(settings: &Settings) -> Result<(), StateError> {
    AppState::check(settings)?;
    crate::state::admits_callers(settings, settings.federates())
}

impl Revisions {
    /// The credentials and binding sections the configuration file
    /// provides: those the newest revision the file gave sets as its own.
    ///
    /// # Errors
    ///
    /// The store's errors.
    pub fn file_owned(&self) -> Result<Vec<Resource>, RevisionError> {
        let history = self.store.history()?;
        let Some(file) = history
            .iter()
            .rev()
            .find(|revision| revision.origin == Origin::File)
        else {
            return Ok(Vec::new());
        };
        Ok(present(&held_table(file)?)
            .into_iter()
            .filter(|resource| !file.console.contains(resource))
            .collect())
    }

    /// The credentials and binding sections the file provides, once `table`
    /// sets each of them as the newest revision the file gave does.
    ///
    /// # Errors
    ///
    /// [`RevisionError::FileOwned`] for the first one `table` changes or
    /// leaves out, and the store's errors.
    pub(super) fn file_untouched(
        &self,
        table: &toml::Table,
    ) -> Result<Vec<Resource>, RevisionError> {
        let file_owned = self.file_owned()?;
        if file_owned.is_empty() {
            return Ok(file_owned);
        }
        let history = self.store.history()?;
        let file = history
            .iter()
            .rev()
            .find(|revision| revision.origin == Origin::File)
            .map(held_table)
            .transpose()?
            .unwrap_or_default();
        match file_owned
            .iter()
            .find(|resource| value_at(table, resource) != value_at(&file, resource))
        {
            Some(resource) => Err(RevisionError::FileOwned(resource.clone())),
            None => Ok(file_owned),
        }
    }
}
