// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Recording the configuration file as a revision, with the resources the
//! operator console owns carried over from the running revision. No
//! specification governs this: our own design.

use std::sync::PoisonError;

use ferrofed_registry::resource::Resource;
use ferrofed_registry::revision::{Author, Current, Origin, Revision};
use jiff::Timestamp;

use super::{
    APPLIED, FILE_ATTEMPTS, FileRevision, Loaded, REFUSED, Refused, RevisionError, Revisions,
    SAVED, Stale, held_table, shown,
};
use crate::config::Config;
use crate::config::error::Error;
use crate::reload::ReloadError;
use crate::revision::resources::{remove, set, value_at};
use crate::revision::tree::{console_keys, split, with_console_of, without_console};
use crate::revision::{Draft, Saved, Swapped};

impl Revisions {
    /// Records the configuration the file gave at a start, `tree`, as a
    /// revision when it differs from the newest one the file gave that ran,
    /// marking it running, and returns the configuration to run: the file's
    /// when it changed, and the running revision's beside the file's core
    /// when it did not.
    ///
    /// A changed file's revision carries the resources the operator console
    /// owns over from the running revision ([`with_console_of`]), so a
    /// console member, credential or binding section survives a reload of
    /// the file. A member the file now provides too is the file's, and the
    /// registry names the console's copy as shadowed; a credential or a
    /// binding section the file now provides is the file's, and the
    /// console's copy is left out of the file's revision, logged by its
    /// table. A running revision that awaits its confirmation when the file
    /// changed is rolled back first. The core is taken from `tree` for every
    /// later revision.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Refused`] when the file's configuration, or the
    /// running revision beside the file's core, is refused, and the store's
    /// errors.
    pub fn file_loaded(&self, tree: &toml::Table) -> Result<Loaded, RevisionError> {
        self.file_read(tree, Marking::Now)
    }

    /// Records the configuration the file gave at a reload, `tree`, as
    /// [`Revisions::file_loaded`] does, without marking it running: the
    /// reload path marks it once it applied it ([`Revisions::file_applied`]),
    /// so a revision the reload refuses never becomes the one every replica
    /// follows ([`FileRevision::refused`]).
    ///
    /// # Errors
    ///
    /// Every error of [`Revisions::file_loaded`].
    pub fn file_reloaded(&self, tree: &toml::Table) -> Result<Loaded, RevisionError> {
        self.file_read(tree, Marking::Applied)
    }

    /// Records `tree` as a file revision, marking it running when `marking`
    /// says.
    fn file_read(&self, tree: &toml::Table, marking: Marking) -> Result<Loaded, RevisionError> {
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
            // NOTE: no specification governs this: our own design; a file revision numbered past
            // the mark never ran, as a reload refused it, so the same file is recorded again.
            let unchanged = match (newest_file, &current) {
                (Some(revision), Some(mark)) if revision.id <= mark.revision => {
                    file_part(revision)? == rest
                }
                _ => false,
            };
            if unchanged && let Some(current) = &current {
                let running = self
                    .store
                    .revision(current.revision)?
                    .ok_or(RevisionError::Unknown(current.revision))?;
                return Ok(Loaded {
                    config: self.validate(&running.toml)?.1,
                    file: FileRevision {
                        revision: running.id,
                        recorded: false,
                        previous: Some(running.id),
                    },
                });
            }
            if let Some(mark) = &current
                && mark.awaiting.is_some()
            {
                let reason = format!(
                    "the configuration file changed while revision {} awaited its confirmation",
                    mark.revision
                );
                self.unconfirmed_back(mark, reason)?;
                continue;
            }
            let (toml, config, console) = self.carried(&rest, current.as_ref(), &text)?;
            let head = history.last().map(|revision| revision.id);
            let draft = Draft {
                origin: Origin::File,
                restores: None,
                author: Author::File,
                at: Timestamp::now(),
                reason: String::from("the configuration file was loaded"),
                toml,
                console,
            };
            let revision = match self.store.save(head, draft)? {
                Saved::Stored(revision) => revision,
                Saved::Stale { .. } => continue,
                Saved::Exhausted => return Err(RevisionError::Exhausted),
            };
            let running = current.map(|current| current.revision);
            tracing::info!(
                target: crate::facade::security::TARGET,
                event = SAVED,
                revision = %revision.id,
                origin = "file",
                "the configuration file was recorded as a revision"
            );
            let file = FileRevision {
                revision: revision.id,
                recorded: true,
                previous: running,
            };
            if marking == Marking::Applied
                || self.store.swap_current(running, &file.mark(&[]))? == Swapped::Swapped
            {
                return Ok(Loaded { config, file });
            }
        }
        Err(Stale::Current {
            expected: None,
            current: self.store.current()?.map(|current| current.revision),
        }
        .into())
    }

    /// Records that a reload of the file applied `file`, leaving
    /// `needs_restart` for the next start: a revision the load recorded is
    /// marked running from the mark the load read, pending a restart when
    /// anything waits for one, as an applied revision's is.
    ///
    /// # Errors
    ///
    /// The store's errors.
    pub fn file_applied(
        &self,
        file: FileRevision,
        needs_restart: &[&'static str],
    ) -> Result<(), RevisionError> {
        if !file.recorded {
            return Ok(());
        }
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        match self
            .store
            .swap_current(file.previous, &file.mark(needs_restart))?
        {
            Swapped::Swapped => tracing::info!(
                target: crate::facade::security::TARGET,
                event = APPLIED,
                revision = %file.revision,
                origin = "file",
                "the configuration file's revision was applied by a reload"
            ),
            Swapped::Stale { current } => tracing::warn!(
                revision = %file.revision,
                current = shown(current),
                "the running revision moved before the file's revision was marked running"
            ),
        }
        Ok(())
    }

    /// Checks the file's configuration `rest`, written as `text`, with the
    /// resources the operator console owns carried over from the running
    /// revision `current`, and returns it as [`Revisions::validate`] does,
    /// with the credentials and binding sections carried over.
    ///
    /// When the console's resources are refused beside the file's, as when
    /// the file drops the registry document they join, the file's
    /// configuration is checked alone and the resources left out are logged
    /// by key: the revision before it keeps them.
    fn carried(
        &self,
        rest: &toml::Table,
        current: Option<&Current>,
        text: &str,
    ) -> Result<(String, Config, Vec<Resource>), RevisionError> {
        let running = match current {
            Some(mark) => self.store.revision(mark.revision)?,
            None => None,
        };
        let Some(running) = running else {
            let (written, config) = self.validate(text)?;
            return Ok((written, config, Vec::new()));
        };
        let running_table = held_table(&running)?;
        let mut carried = with_console_of(rest, &running_table);
        let mut console = Vec::new();
        for resource in &running.console {
            if value_at(rest, resource).is_some() {
                tracing::warn!(
                    resource = %resource,
                    revision = %running.id,
                    "the configuration file now provides a resource the console held; the file's runs, and the console's copy is left out of the file's revision"
                );
            } else if let Some(value) = value_at(&running_table, resource) {
                set(&mut carried, resource, value.clone());
                console.push(resource.clone());
            }
        }
        if carried == *rest {
            let (written, config) = self.validate(text)?;
            return Ok((written, config, Vec::new()));
        }
        let written = toml::to_string(&carried)
            .map_err(|source| Refused::from(Error::Assemble { source }))?;
        match self.validate(&written) {
            Ok((written, config)) => Ok((written, config, console)),
            Err(refused) => {
                let keys: Vec<String> = console_keys(&carried)
                    .into_iter()
                    .chain(console.iter().map(ToString::to_string))
                    .collect();
                tracing::warn!(
                    keys = keys.join(","),
                    revision = %running.id,
                    error = crate::chain(&refused),
                    "the console's resources are refused beside the file's configuration and left out of its revision; the running revision still holds them"
                );
                let (written, config) = self.validate(text)?;
                Ok((written, config, Vec::new()))
            }
        }
    }
}

impl FileRevision {
    /// Records that a reload of the file refused this revision for `error`:
    /// the running mark stays where the load read it, so no replica follows
    /// it. The record names the class of the refusal, never a value.
    pub fn refused(&self, error: &ReloadError) {
        if !self.recorded {
            return;
        }
        tracing::warn!(
            target: crate::facade::security::TARGET,
            event = REFUSED,
            revision = %self.revision,
            origin = "file",
            class = error.class(),
            reason = %error,
            current = shown(self.previous),
            "the reload refused the configuration file's revision, and the running revision stays"
        );
    }
}

/// When a file load marks the revision it records running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marking {
    /// At once: a start runs it or does not start.
    Now,
    /// Once the reload path applied it.
    Applied,
}

/// The part of `revision`, one the file gave, that the file wrote: without
/// the members, credentials and binding sections the console owns in it.
fn file_part(revision: &Revision) -> Result<toml::Table, RevisionError> {
    let mut table = without_console(&held_table(revision)?);
    for resource in &revision.console {
        remove(&mut table, resource);
    }
    Ok(table)
}
