// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Validating, saving, applying and rolling back configuration revisions.
//!
//! Every revision, a file load or an operator's save, is checked as `config
//! check` checks the file: it parses as the configuration with the file's
//! bootstrap core beside it, [`Config::resolve`] resolves it, and
//! [`AppState::check`](crate::state::AppState::check) builds what a start would build, so a revision is
//! refused with the errors `config check` reports. An apply moves the current
//! mark from the revision it read and then hands the configuration to the
//! reload path ([`Reloader::reload_config`]); a reload that refuses it moves
//! the mark back, so the mark and the running gateway agree. A reload of the
//! file, or a start, marks the revision the file's load records running
//! only once the gateway runs it, so no replica follows one it refused. A rollback saves
//! the content of the revision before the current one as a new revision and
//! applies it. Every call blocks, so it is made where a thread may. No
//! specification governs this: our own design.

use std::sync::{Mutex, PoisonError};

use jiff::Timestamp;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::revisions::RevisionsFault;
use crate::config::settings::Settings;
use crate::reload::{ReloadError, Reloader};
use crate::revision::policy::Policy;
use crate::revision::resources::present;
use crate::revision::tree::{core_keys, inline_secrets, join, split};
use crate::revision::{Draft, RevisionStore, StoreError, Swapped};
use crate::state::StateError;
use ferrofed_registry::resource::Resource;
use ferrofed_registry::revision::{Author, Current, Origin, Outcome, Revision, RevisionId};

use checks::{checked, configuration, reason_of, stored};
use confirm::Reviewed;

mod checks;
pub mod confirm;
mod file;

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
    /// The revision changes or removes a credential or a binding section
    /// the configuration file provides.
    #[error("{0} is provided by the configuration file, so only the file changes it")]
    FileOwned(Resource),
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
    /// The revision changes resources that take a second operator's
    /// approval, and no operator other than its author approved it.
    #[error(
        "revision {id} changes {} and takes the approval of an operator other than its author first",
        resources.join(", ")
    )]
    ApprovalRequired {
        /// The revision.
        id: RevisionId,
        /// The resources that take the approval, as their tables are written.
        resources: Vec<String>,
    },
    /// The revision's author approved it, which only another operator may.
    #[error("revision {0} is approved by an operator other than its author")]
    OwnApproval(RevisionId),
    /// The revision changes no resource that takes an approval.
    #[error("revision {0} changes no resource that takes an approval")]
    ApprovalNotRequired(RevisionId),
    /// A confirmation or an approval came from no verified operator.
    #[error(
        "a confirmation or an approval names its operator, and a loopback peer with no credential names none"
    )]
    Unverified,
    /// The revision named does not await a confirmation now.
    #[error(transparent)]
    Unconfirmable(#[from] Unconfirmable),
    /// The running revision awaits its confirmation, so no other applies
    /// until it is confirmed or rolled back.
    #[error(
        "revision {0} runs awaiting its confirmation; confirm it or roll it back before another applies"
    )]
    AwaitingConfirmation(RevisionId),
    /// The confirmation deadline does not fit a timestamp.
    #[error("the confirmation deadline does not fit a timestamp")]
    Deadline(#[source] jiff::Error),
}

/// Why a revision cannot be confirmed now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Unconfirmable {
    /// Another revision runs.
    #[error("revision {id} does not run; revision {} does", shown(*.running))]
    NotRunning {
        /// The revision named.
        id: RevisionId,
        /// The revision that runs.
        running: Option<RevisionId>,
    },
    /// The revision runs and awaits no confirmation.
    #[error("revision {0} awaits no confirmation")]
    NotAwaiting(RevisionId),
    /// The deadline passed, and the gateway rolls it back.
    #[error("the confirmation of revision {0} is past its deadline; the gateway rolls it back")]
    Expired(RevisionId),
}

/// The configuration revisions of a managed gateway: the store, and the
/// bootstrap core the file sets, which every revision runs beside.
pub struct Revisions {
    store: Box<dyn RevisionStore>,
    core: Mutex<toml::Table>,
    serial: Mutex<()>,
    replica: Mutex<Replica>,
    policy: Policy,
}

/// Where this replica stands against the store's running mark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Replica {
    /// The revision this replica runs, once it runs one.
    pub running: Option<RevisionId>,
    /// The revision another replica marked running that this replica
    /// refused, while it does not run it.
    pub refused: Option<RevisionId>,
}

/// What a read of the running mark did on this replica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub enum Followed {
    /// This replica runs the revision marked running, or none is marked.
    InStep,
    /// This replica applied the revision marked running, which another
    /// replica applied.
    Applied(RevisionId),
    /// This replica refused the revision marked running before, and keeps
    /// its running configuration.
    Drifting(RevisionId),
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
            replica: Mutex::new(Replica::default()),
            policy: Policy::default(),
        }
    }

    /// Returns these revisions with `policy`, what a revision that changes
    /// a credential or a binding section takes.
    #[must_use]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// What a revision that changes a credential or a binding section takes.
    #[must_use]
    pub fn policy(&self) -> &Policy {
        &self.policy
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
    /// core; [`Refused::State`] for what [`AppState::check`](crate::state::AppState::check) and the caller
    /// check refuse.
    pub fn validate(&self, text: &str) -> Result<(String, Config), Refused> {
        let table = crate::config::load::table(text)?;
        if let Some(key) = core_keys(&table).into_iter().next() {
            return Err(Refused::Core { key });
        }
        if let Some(key) = inline_secrets(&table).into_iter().next() {
            return Err(Error::from(RevisionsFault::InlineSecret { key }).into());
        }
        let config = configuration(&join(&self.core(), &table))?;
        let settings = config.resolve()?;
        checked(&settings)?;
        let written = toml::to_string(&table).map_err(|source| Error::assemble(source))?;
        Ok((written, config))
    }

    /// Saves `toml` as the revision after `base`, made by `author` for
    /// `reason`, once it passes [`Revisions::validate`]. The text is stored
    /// as TOML writes it, so no comment is kept.
    ///
    /// Every credential and binding section it sets that the file does not
    /// provide is the console's ([`Revisions::file_owned`]).
    ///
    /// # Errors
    ///
    /// [`RevisionError::Refused`] for a reason or a configuration refused,
    /// [`RevisionError::FileOwned`] for a credential or a binding section
    /// the file provides set otherwise than the file sets it,
    /// [`RevisionError::Stale`] when `base` is not the newest revision, and
    /// the store's errors.
    pub fn save(
        &self,
        base: Option<RevisionId>,
        (author, reason): (Author, String),
        toml: &str,
    ) -> Result<Revision, RevisionError> {
        self.save_from(Origin::Operator, base, (author, reason), toml)
    }

    /// Saves `toml` as [`Revisions::save`] does, recording `origin` as
    /// where it comes from: an operator's save or an import.
    ///
    /// # Errors
    ///
    /// As [`Revisions::save`].
    pub fn save_from(
        &self,
        origin: Origin,
        base: Option<RevisionId>,
        (author, reason): (Author, String),
        toml: &str,
    ) -> Result<Revision, RevisionError> {
        let reason = reason_of(&reason)?;
        let (toml, _config) = self.validate(toml)?;
        let table = crate::config::load::table(&toml).map_err(Refused::from)?;
        let file_owned = self.file_untouched(&table)?;
        let console = present(&table)
            .into_iter()
            .filter(|resource| !file_owned.contains(resource))
            .collect();
        let draft = Draft {
            origin,
            restores: None,
            author,
            at: Timestamp::now(),
            reason,
            toml,
            console,
        };
        stored(self.store.save(base, draft)?, base)
    }

    /// Applies revision `id` for `by` through `reloader`, and returns the
    /// mark the store now holds.
    ///
    /// A revision that changes a resource `config.approval` names applies
    /// only once an operator other than its author approved it; one that
    /// changes a credential or a binding section runs awaiting its
    /// confirmation until the deadline `config.confirm_ms` sets.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Unknown`] for a revision not held,
    /// [`RevisionError::Stale`] when it is not the newest revision, is
    /// already running, or the mark moved, [`RevisionError::NoCurrent`] when
    /// no revision is marked running, [`RevisionError::AwaitingConfirmation`]
    /// while the running revision awaits its confirmation,
    /// [`RevisionError::ApprovalRequired`] for one not yet approved,
    /// [`RevisionError::Refused`] and [`RevisionError::Reload`] when its
    /// configuration is refused, and the store's errors.
    pub fn apply(
        &self,
        reloader: &Reloader,
        id: RevisionId,
        by: Author,
    ) -> Result<Current, RevisionError> {
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        self.apply_held(reloader, id, by, Reviewed::Operator)
    }

    /// Rolls the running revision, which the caller read as `expected`,
    /// back to the one before it: saves that one's content as a new revision
    /// by `author` for `reason`, and applies it.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Stale`] when `expected` is not running or another
    /// save came first, [`RevisionError::NoPrevious`] when there is no
    /// earlier revision, and every error of [`Revisions::apply`] but those
    /// of the approval and the confirmation, which a rollback does not take.
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
        self.rollback_held(reloader, &current, (author, reason))
    }

    /// Rolls the running revision `current` back for `author`, the serial
    /// lock held.
    fn rollback_held(
        &self,
        reloader: &Reloader,
        current: &Current,
        (author, reason): (Author, String),
    ) -> Result<(Revision, Current), RevisionError> {
        let revision = self.restoring(current, (author.clone(), reason))?;
        let current = self.apply_held(reloader, revision.id, author, Reviewed::Rollback)?;
        Ok((revision, current))
    }

    /// Saves the content of the revision that ran before `current` as a new
    /// revision by `author` for `reason`, checked, and returns it unapplied.
    fn restoring(
        &self,
        current: &Current,
        (author, reason): (Author, String),
    ) -> Result<Revision, RevisionError> {
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
            author,
            at: Timestamp::now(),
            reason,
            toml: target.toml,
            console: target.console,
        };
        stored(self.store.save(head, draft)?, head)
    }

    /// Applies revision `id` for `by`, the serial lock held; an operator's
    /// apply is `reviewed` for its approval and its confirmation, and a
    /// rollback is not.
    fn apply_held(
        &self,
        reloader: &Reloader,
        id: RevisionId,
        by: Author,
        reviewed: Reviewed,
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
        let awaiting = match reviewed {
            Reviewed::Operator => {
                if running.awaiting.is_some() {
                    return Err(RevisionError::AwaitingConfirmation(running.revision));
                }
                let review = self.review_against(&revision, &running)?;
                if !review.approved {
                    return Err(RevisionError::ApprovalRequired {
                        id,
                        resources: review.approval,
                    });
                }
                self.awaiting(review.confirms)?
            }
            Reviewed::Rollback => None,
        };
        let (_, config) = self.validate(&revision.toml)?;
        let mark = Current {
            revision: id,
            previous: Some(running.revision),
            outcome: Outcome::Applied,
            needs_restart: Vec::new(),
            applied_by: by,
            applied_at: Timestamp::now(),
            awaiting,
            confirmed: None,
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
        match reloader.reload_config(&config, id) {
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
    /// Where this replica stands: the revision it runs, and one it refused.
    #[must_use]
    pub fn replica(&self) -> Replica {
        *self.replica.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records that this replica runs revision `id`.
    pub fn ran(&self, id: RevisionId) {
        *self.replica.lock().unwrap_or_else(PoisonError::into_inner) = Replica {
            running: Some(id),
            refused: None,
        };
    }

    /// Reads the store's running mark and, when it names a revision this
    /// replica neither runs nor refused before, applies it through
    /// `reloader`, so a revision another replica applied runs here too.
    ///
    /// A revision this replica refuses is recorded as refused and not tried
    /// again; the replica keeps its running configuration until it runs
    /// another revision, by a later apply or a `SIGHUP` reload.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Store`] when the store cannot be read, and, with the
    /// revision recorded as refused, [`RevisionError::Unknown`] for a mark
    /// that names no revision held, [`RevisionError::Refused`] and
    /// [`RevisionError::Reload`] when its configuration is refused here.
    pub fn follow(&self, reloader: &Reloader) -> Result<Followed, RevisionError> {
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(current) = self.store.current()? else {
            return Ok(Followed::InStep);
        };
        let id = current.revision;
        let replica = self.replica();
        if replica.running == Some(id) {
            return Ok(Followed::InStep);
        }
        if replica.refused == Some(id) {
            return Ok(Followed::Drifting(id));
        }
        let outcome = self
            .store
            .revision(id)?
            .ok_or(RevisionError::Unknown(id))
            .and_then(|revision| Ok(self.validate(&revision.toml)?.1))
            .and_then(|config| {
                reloader
                    .reload_config(&config, id)
                    .map_err(RevisionError::Reload)
            });
        match outcome {
            Ok(_applied) => Ok(Followed::Applied(id)),
            Err(error @ RevisionError::Store(_)) => Err(error),
            Err(refusal) => {
                self.replica
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .refused = Some(id);
                Err(refusal)
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
        let settled = pending(mark, needs_restart);
        if let Swapped::Stale { current } = self.store.swap_current(Some(id), &settled)? {
            tracing::warn!(
                revision = %id,
                current = shown(current),
                "the running revision moved before its pending restart was recorded"
            );
        }
        Ok(settled)
    }
}

/// `mark` pending a restart for `needs_restart`, when any setting waits for
/// one.
fn pending(mark: Current, needs_restart: &[&'static str]) -> Current {
    if needs_restart.is_empty() {
        return mark;
    }
    Current {
        outcome: Outcome::PendingRestart,
        needs_restart: needs_restart.iter().map(|key| (*key).to_owned()).collect(),
        ..mark
    }
}

/// The configuration `revision` holds, as a table.
///
/// # Errors
///
/// [`StoreError::Corrupt`] when the text the store holds is no TOML table.
pub fn held_table(revision: &Revision) -> Result<toml::Table, RevisionError> {
    toml::from_str(&revision.toml)
        .map_err(|error| RevisionError::Store(StoreError::configuration(&error, &revision.toml)))
}

/// The configuration a file load gives the gateway to run.
#[derive(Debug)]
#[non_exhaustive]
pub struct Loaded {
    /// The configuration, the revision beside the file's bootstrap core.
    pub config: Config,
    /// The revision it is, and whether the load recorded it.
    pub file: FileRevision,
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

/// The `event` of the record of a saved revision.
pub const SAVED: &str = "config-revision-saved";

/// The `event` of the record of an applied revision.
pub const APPLIED: &str = "config-revision-applied";

/// The `event` of the record of a file's revision a reload refused.
pub const REFUSED: &str = "config-revision-refused";

/// The `event` of the record of a rollback.
pub const ROLLED_BACK: &str = "config-rolled-back";

/// The revisions a managed gateway starts with, and the revision the file's
/// load gave it to run, which is marked running once the gateway runs it.
#[derive(Debug)]
#[non_exhaustive]
pub struct Booted {
    /// The revisions.
    pub revisions: std::sync::Arc<Revisions>,
    /// The revision the gateway starts on.
    pub file: FileRevision,
}

/// Opens the revision store `settings` name and records the file's configuration.
///
/// When the configuration is managed, the file's configuration `tree` is
/// recorded ([`Revisions::file_loaded`]) and the settings to start with are
/// returned with the revisions, whose revision is marked running once the
/// gateway built what it runs ([`Booted::started`]); otherwise `settings`
/// are returned unchanged with none.
///
/// # Errors
///
/// [`RevisionError::Store`] when the store cannot be opened, and every
/// error of [`Revisions::file_loaded`].
pub fn boot(
    settings: Settings,
    tree: &toml::Table,
) -> Result<(Settings, Option<Booted>), RevisionError> {
    let Some(store) = &settings.revisions else {
        return Ok((settings, None));
    };
    let (effective, booted) = boot_over(crate::revision::open(store)?, tree)?;
    Ok((effective, Some(booted)))
}

/// Records the file's configuration `tree` in `store`, as [`boot`] does with
/// the store the settings name, and returns the settings to start with and
/// the revisions.
///
/// A running revision past its confirmation deadline is rolled back first.
///
/// # Errors
///
/// Every error of [`Revisions::file_loaded`], and [`RevisionError::Refused`]
/// when the configuration to run does not resolve.
pub fn boot_over(
    store: Box<dyn RevisionStore>,
    tree: &toml::Table,
) -> Result<(Settings, Booted), RevisionError> {
    let revisions = Revisions::new(store, split(tree).0);
    revisions.expire_unapplied(Timestamp::now())?;
    let loaded = revisions.file_loaded(tree)?;
    let effective = loaded.config.resolve().map_err(Refused::from)?;
    let revisions = revisions.with_policy(effective.revision_policy.clone());
    revisions.ran(loaded.file.revision);
    Ok((
        effective,
        Booted {
            revisions: std::sync::Arc::new(revisions),
            file: loaded.file,
        },
    ))
}
