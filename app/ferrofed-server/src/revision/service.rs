// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Validating, saving, applying and rolling back configuration revisions.
//!
//! Every revision, a file load or an operator's save, is checked as `config
//! check` checks the file: it parses as the configuration with the file's
//! bootstrap core beside it, [`Config::resolve`] resolves it, and
//! [`AppState::check`] builds what a start would build, so a revision is
//! refused with the errors `config check` reports. An apply moves the current
//! mark from the revision it read and then hands the configuration to the
//! reload path ([`Reloader::reload_config`]); a reload that refuses it moves
//! the mark back, so the mark and the running gateway agree. A rollback saves
//! the content of the revision before the current one as a new revision and
//! applies it. Every call blocks, so it is made where a thread may. No
//! specification governs this: our own design.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use jiff::Timestamp;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::error::parse::Stage;
use crate::config::revisions::RevisionsFault;
use crate::config::settings::Settings;
use crate::reload::{ReloadError, Reloader};
use crate::revision::tree::{core_keys, inline_secrets, join, split};
use crate::revision::{
    Author, Current, Draft, Origin, Outcome, Revision, RevisionId, RevisionStore, Saved,
    StoreError, Swapped,
};
use crate::state::{AppState, StateError};

/// The longest reason a revision records, in characters.
pub const REASON_LIMIT: usize = 1_000;

/// How many times a file load is recorded again when another replica saved
/// or moved the mark between the read and the write.
const FILE_ATTEMPTS: usize = 3;

/// Why a revision's configuration is refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Refused {
    /// The configuration is refused as `config check` refuses it.
    #[error(transparent)]
    Config(#[from] Error),
    /// What the configuration describes cannot be built, as `config check`
    /// reports it.
    #[error(transparent)]
    State(#[from] StateError),
    /// The revision sets a key of the bootstrap core, which only the file
    /// sets.
    #[error("{key} is part of the bootstrap core, which only the configuration file sets")]
    Core {
        /// The top-level key.
        key: String,
    },
    /// The reason is empty or longer than [`REASON_LIMIT`].
    #[error("a revision records why it is made: a reason of 1 to {REASON_LIMIT} characters")]
    Reason,
}

/// Why a revision cannot be saved, applied or rolled back now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Stale {
    /// The base a save names is not the newest revision.
    #[error("the save is based on {}, and the newest revision is {}", shown(*.named), shown(*.head))]
    Base {
        /// The base the save named.
        named: Option<RevisionId>,
        /// The newest revision.
        head: Option<RevisionId>,
    },
    /// Only the newest revision applies, and a later one supersedes it.
    #[error("revision {id} is superseded by revision {}; only the newest revision applies", shown(*.head))]
    Superseded {
        /// The revision named.
        id: RevisionId,
        /// The newest revision.
        head: Option<RevisionId>,
    },
    /// The revision named is already the one the gateway runs.
    #[error("revision {0} is already the running revision")]
    Running(RevisionId),
    /// The current mark is not where the caller read it.
    #[error("the running revision is {}, not {}", shown(*.current), shown(*.expected))]
    Current {
        /// The revision the caller read as current.
        expected: Option<RevisionId>,
        /// The revision the store marks current.
        current: Option<RevisionId>,
    },
}

/// Renders an optional revision, `none` when there is none.
fn shown(id: Option<RevisionId>) -> String {
    id.map_or_else(|| String::from("none"), |id| id.to_string())
}

/// A revision that was not saved, applied or rolled back.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RevisionError {
    /// The store could not be read or written.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// No revision has the number named.
    #[error("no revision {0} is held")]
    Unknown(RevisionId),
    /// The revision's configuration is refused.
    #[error("the revision's configuration is refused")]
    Refused(#[from] Refused),
    /// Another save, apply or rollback came first.
    #[error(transparent)]
    Stale(#[from] Stale),
    /// The running revision has no earlier one to roll back to.
    #[error("the running revision has no earlier revision to roll back to")]
    NoPrevious,
    /// The store marks no revision as running, so nothing can be moved back
    /// to if the reload refuses; a restart records the file's.
    #[error("no running revision is recorded; restart the gateway to record the file's")]
    NoCurrent,
    /// The reload path refused the configuration; the running one stays.
    #[error("the reload refused the revision, and the running configuration stays")]
    Reload(#[source] ReloadError),
    /// The revision numbers are exhausted.
    #[error("the revision numbers are exhausted")]
    Exhausted,
    /// A path names no revision number.
    #[error("the path names no revision number")]
    NotANumber,
}

/// The configuration revisions of a managed gateway: the store, and the
/// bootstrap core the file sets, which every revision runs beside.
pub struct Revisions {
    store: Box<dyn RevisionStore>,
    core: Mutex<toml::Table>,
    serial: Mutex<()>,
}

impl std::fmt::Debug for Revisions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Revisions")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

impl Revisions {
    /// The revisions `store` holds, run beside `core`, the bootstrap core of
    /// the file.
    #[must_use]
    pub fn new(store: Box<dyn RevisionStore>, core: toml::Table) -> Self {
        Self {
            store,
            core: Mutex::new(core),
            serial: Mutex::new(()),
        }
    }

    /// The store.
    #[must_use]
    pub fn store(&self) -> &dyn RevisionStore {
        self.store.as_ref()
    }

    /// The bootstrap core every revision runs beside.
    fn core(&self) -> toml::Table {
        self.core
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Checks `text`, a revision's configuration less the bootstrap core, as
    /// `config check` checks the file, and returns it as TOML writes it with
    /// the configuration it makes beside the core.
    ///
    /// # Errors
    ///
    /// [`Refused::Config`] for text that is not TOML, a secret held inline
    /// ([`RevisionsFault::InlineSecret`]) and every refusal of the parse and
    /// of [`Config::resolve`]; [`Refused::Core`] for a key of the bootstrap
    /// core; [`Refused::State`] for what [`AppState::check`] and the caller
    /// check refuse.
    pub fn validate(&self, text: &str) -> Result<(String, Config), Refused> {
        let table: toml::Table =
            toml::from_str(text).map_err(|error| Error::parse(&error, text, Stage::File))?;
        if let Some(key) = core_keys(&table).into_iter().next() {
            return Err(Refused::Core { key });
        }
        if let Some(key) = inline_secrets(&table).into_iter().next() {
            return Err(Error::from(RevisionsFault::InlineSecret { key }).into());
        }
        let config = configuration(&join(&self.core(), &table))?;
        let settings = config.resolve()?;
        checked(&settings)?;
        let written = toml::to_string(&table).map_err(|source| Error::Assemble { source })?;
        Ok((written, config))
    }

    /// Saves `toml` as the revision after `base`, made by `author` for
    /// `reason`, once it passes [`Revisions::validate`]. The text is stored
    /// as TOML writes it, so no comment is kept.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Refused`] for a reason or a configuration refused,
    /// [`RevisionError::Stale`] when `base` is not the newest revision, and
    /// the store's errors.
    pub fn save(
        &self,
        base: Option<RevisionId>,
        (author, reason): (Author, String),
        toml: &str,
    ) -> Result<Revision, RevisionError> {
        let reason = reason_of(&reason)?;
        let (toml, _config) = self.validate(toml)?;
        let draft = Draft {
            origin: Origin::Operator,
            restores: None,
            author,
            at: Timestamp::now(),
            reason,
            toml,
        };
        stored(self.store.save(base, draft)?, base)
    }

    /// Applies revision `id` for `by` through `reloader`, and returns the
    /// mark the store now holds.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Unknown`] for a revision not held,
    /// [`RevisionError::Stale`] when it is not the newest revision, is
    /// already running, or the mark moved, [`RevisionError::NoCurrent`] when
    /// no revision is marked running, [`RevisionError::Refused`] and
    /// [`RevisionError::Reload`] when its configuration is refused, and the
    /// store's errors.
    pub fn apply(
        &self,
        reloader: &Reloader,
        id: RevisionId,
        by: Author,
    ) -> Result<Current, RevisionError> {
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        self.apply_held(reloader, id, by)
    }

    /// Rolls the running revision, which the caller read as `expected`,
    /// back to the one before it: saves that one's content as a new revision
    /// by `author` for `reason`, and applies it.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Stale`] when `expected` is not running or another
    /// save came first, [`RevisionError::NoPrevious`] when there is no
    /// earlier revision, and every error of [`Revisions::apply`].
    pub fn rollback(
        &self,
        reloader: &Reloader,
        expected: RevisionId,
        (author, reason): (Author, String),
    ) -> Result<(Revision, Current), RevisionError> {
        let reason = reason_of(&reason)?;
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        let current = self.store.current()?.ok_or(RevisionError::NoCurrent)?;
        if current.revision != expected {
            return Err(Stale::Current {
                expected: Some(expected),
                current: Some(current.revision),
            }
            .into());
        }
        let previous = current.previous.ok_or(RevisionError::NoPrevious)?;
        let target = self
            .store
            .revision(previous)?
            .ok_or(RevisionError::Unknown(previous))?;
        self.validate(&target.toml)?;
        let head = self.store.head()?;
        let draft = Draft {
            origin: Origin::Rollback,
            restores: Some(previous),
            author: author.clone(),
            at: Timestamp::now(),
            reason,
            toml: target.toml,
        };
        let revision = stored(self.store.save(head, draft)?, head)?;
        let current = self.apply_held(reloader, revision.id, author)?;
        Ok((revision, current))
    }

    /// Applies revision `id` for `by`, the serial lock held.
    fn apply_held(
        &self,
        reloader: &Reloader,
        id: RevisionId,
        by: Author,
    ) -> Result<Current, RevisionError> {
        let revision = self.store.revision(id)?.ok_or(RevisionError::Unknown(id))?;
        let head = self.store.head()?;
        if head != Some(id) {
            return Err(Stale::Superseded { id, head }.into());
        }
        let running = self.store.current()?.ok_or(RevisionError::NoCurrent)?;
        if running.revision == id {
            return Err(Stale::Running(id).into());
        }
        let (_, config) = self.validate(&revision.toml)?;
        let mark = Current {
            revision: id,
            previous: Some(running.revision),
            outcome: Outcome::Applied,
            needs_restart: Vec::new(),
            applied_by: by,
            applied_at: Timestamp::now(),
        };
        if let Swapped::Stale { current } =
            self.store.swap_current(Some(running.revision), &mark)?
        {
            return Err(Stale::Current {
                expected: Some(running.revision),
                current,
            }
            .into());
        }
        match reloader.reload_config(&config) {
            Ok(applied) => self.settle_held(mark, &applied.needs_restart),
            Err(error) => {
                if let Swapped::Stale { current } = self.store.swap_current(Some(id), &running)? {
                    tracing::warn!(
                        revision = %id,
                        current = shown(current),
                        "the running revision moved before a refused apply could restore it"
                    );
                }
                Err(RevisionError::Reload(error))
            }
        }
    }

    /// Records on `mark`, the mark of a revision the reload path applied,
    /// the changed settings that take effect only at a restart, marking it
    /// pending a restart when there are any, and returns the mark held.
    fn settle_held(
        &self,
        mark: Current,
        needs_restart: &[&'static str],
    ) -> Result<Current, RevisionError> {
        if needs_restart.is_empty() {
            return Ok(mark);
        }
        let id = mark.revision;
        let settled = Current {
            outcome: Outcome::PendingRestart,
            needs_restart: needs_restart.iter().map(|key| (*key).to_owned()).collect(),
            ..mark
        };
        if let Swapped::Stale { current } = self.store.swap_current(Some(id), &settled)? {
            tracing::warn!(
                revision = %id,
                current = shown(current),
                "the running revision moved before its pending restart was recorded"
            );
        }
        Ok(settled)
    }

    /// Records that a reload of the file applied revision `id`, which the
    /// file load recorded, leaving `needs_restart` for the next start: the
    /// mark is then pending a restart, as an applied revision's is.
    ///
    /// # Errors
    ///
    /// The store's errors.
    pub fn file_applied(
        &self,
        id: RevisionId,
        needs_restart: &[&'static str],
    ) -> Result<(), RevisionError> {
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        match self.store.current()? {
            Some(mark) if mark.revision == id => {
                self.settle_held(mark, needs_restart)?;
            }
            current => tracing::warn!(
                revision = %id,
                current = shown(current.map(|current| current.revision)),
                "the running revision moved before the file's pending restart was recorded"
            ),
        }
        Ok(())
    }

    /// Records the configuration the file gave, `tree`, as a revision when it
    /// differs from the newest one the file gave, marking it running, and
    /// returns the configuration to run: the file's when it changed, and the
    /// running revision's beside the file's core when it did not.
    ///
    /// The core is taken from `tree` for every later revision.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Refused`] when the file's configuration, or the
    /// running revision beside the file's core, is refused, and the store's
    /// errors.
    pub fn file_loaded(&self, tree: &toml::Table) -> Result<Loaded, RevisionError> {
        let (core, rest) = split(tree);
        *self.core.lock().unwrap_or_else(PoisonError::into_inner) = core;
        let text =
            toml::to_string(&rest).map_err(|source| Refused::from(Error::Assemble { source }))?;
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        for _ in 0..FILE_ATTEMPTS {
            let history = self.store.history()?;
            let current = self.store.current()?;
            let newest_file = history
                .iter()
                .rev()
                .find(|revision| revision.origin == Origin::File);
            let unchanged = newest_file.is_some_and(|revision| revision.toml == text);
            if unchanged && let Some(current) = &current {
                let running = self
                    .store
                    .revision(current.revision)?
                    .ok_or(RevisionError::Unknown(current.revision))?;
                return Ok(Loaded {
                    config: self.validate(&running.toml)?.1,
                    revision: running.id,
                    recorded: false,
                });
            }
            let (toml, config) = self.validate(&text)?;
            let head = history.last().map(|revision| revision.id);
            let draft = Draft {
                origin: Origin::File,
                restores: None,
                author: Author::File,
                at: Timestamp::now(),
                reason: String::from("the configuration file was loaded"),
                toml,
            };
            let revision = match self.store.save(head, draft)? {
                Saved::Stored(revision) => revision,
                Saved::Stale { .. } => continue,
                Saved::Exhausted => return Err(RevisionError::Exhausted),
            };
            let running = current.map(|current| current.revision);
            let mark = Current {
                revision: revision.id,
                previous: running,
                outcome: Outcome::Applied,
                needs_restart: Vec::new(),
                applied_by: Author::File,
                applied_at: Timestamp::now(),
            };
            if self.store.swap_current(running, &mark)? == Swapped::Swapped {
                tracing::info!(
                    target: crate::facade::security::TARGET,
                    event = SAVED,
                    revision = %revision.id,
                    origin = "file",
                    "the configuration file was recorded as a revision"
                );
                return Ok(Loaded {
                    config,
                    revision: revision.id,
                    recorded: true,
                });
            }
        }
        Err(Stale::Current {
            expected: None,
            current: self.store.current()?.map(|current| current.revision),
        }
        .into())
    }
}

/// The configuration a file load gives the gateway to run.
#[derive(Debug)]
#[non_exhaustive]
pub struct Loaded {
    /// The configuration, the revision beside the file's bootstrap core.
    pub config: Config,
    /// The revision it is.
    pub revision: RevisionId,
    /// Whether the load recorded it as a new revision by the file.
    pub recorded: bool,
}

/// The `event` of the record of a saved revision.
pub const SAVED: &str = "config-revision-saved";

/// The `event` of the record of an applied revision.
pub const APPLIED: &str = "config-revision-applied";

/// The `event` of the record of a rollback.
pub const ROLLED_BACK: &str = "config-rolled-back";

/// `reason`, trimmed, when it is 1 to [`REASON_LIMIT`] characters.
fn reason_of(reason: &str) -> Result<String, Refused> {
    let trimmed = reason.trim();
    if trimmed.is_empty() || trimmed.chars().count() > REASON_LIMIT {
        return Err(Refused::Reason);
    }
    Ok(trimmed.to_owned())
}

/// The revision `saved` stored over `base`.
fn stored(saved: Saved, base: Option<RevisionId>) -> Result<Revision, RevisionError> {
    match saved {
        Saved::Stored(revision) => Ok(revision),
        Saved::Stale { head } => Err(Stale::Base { named: base, head }.into()),
        Saved::Exhausted => Err(RevisionError::Exhausted),
    }
}

/// The configuration `tree` writes, read as the file is read with no
/// environment beside it.
fn configuration(tree: &toml::Table) -> Result<Config, Error> {
    let text = toml::to_string(tree).map_err(|source| Error::Assemble { source })?;
    Config::from_sources(Some(&text), &BTreeMap::new())
}

/// Checks `settings` as `config check` does past resolving them.
fn checked(settings: &Settings) -> Result<(), StateError> {
    AppState::check(settings)?;
    crate::state::admits_callers(settings, settings.federates())
}

/// Opens the revision store `settings` name and records the file's configuration.
///
/// When the configuration is managed, the file's configuration `tree` is
/// recorded ([`Revisions::file_loaded`]) and the settings to start with are
/// returned with the revisions; otherwise `settings` are returned unchanged
/// with none.
///
/// # Errors
///
/// [`RevisionError::Store`] when the store cannot be opened, and every
/// error of [`Revisions::file_loaded`].
pub fn boot(
    settings: Settings,
    tree: &toml::Table,
) -> Result<(Settings, Option<std::sync::Arc<Revisions>>), RevisionError> {
    let Some(store) = &settings.revisions else {
        return Ok((settings, None));
    };
    let revisions = Revisions::new(crate::revision::open(store)?, split(tree).0);
    let loaded = revisions.file_loaded(tree)?;
    let effective = loaded.config.resolve().map_err(Refused::from)?;
    Ok((effective, Some(std::sync::Arc::new(revisions))))
}
