// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checks every save, apply and rollback shares: the reason a revision
//! records, the outcome of a store's save, and the configuration a revision
//! makes checked as `config check` checks it. No specification governs
//! this: our own design.

use std::collections::BTreeMap;

use ferrofed_registry::revision::{Revision, RevisionId};

use super::{REASON_LIMIT, Refused, RevisionError, Stale};
use crate::config::Config;
use crate::config::error::Error;
use crate::config::settings::Settings;
use crate::revision::Saved;
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
    let text = toml::to_string(tree).map_err(|source| Error::Assemble { source })?;
    Config::from_sources(Some(&text), &BTreeMap::new())
}

/// Checks `settings` as `config check` does past resolving them.
pub(super) fn checked(settings: &Settings) -> Result<(), StateError> {
    AppState::check(settings)?;
    crate::state::admits_callers(settings, settings.federates())
}
