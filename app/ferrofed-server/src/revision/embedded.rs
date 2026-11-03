// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The embedded revision store, over `redb`, for one gateway process.
//!
//! One table maps a revision number to the revision as JSON, a second holds
//! the current mark as JSON, a third the approvals of each revision, and a
//! fourth records the schema version of the layout. A save reads the newest number and writes the next in one write
//! transaction, and a move of the current mark reads the mark and writes it
//! in one, so neither races; `redb` is an ACID embedded store
//! (<https://docs.rs/redb/4.3.0/redb/>). The file holds configuration text
//! with every secret only as a `_file` reference, and never a patient
//! identifier. No specification governs the storage: our own design.

use std::path::{Path, PathBuf};

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction};

use ferrofed_operator_api::revision::{Current, Revision, RevisionId, Signed};

use crate::revision::{Corruption, Draft, RevisionStore, Saved, StoreError, Swapped};
use crate::stored::embedded::open_prepared;

/// The revision number to the revision, as JSON.
const REVISIONS: TableDefinition<'static, u64, &str> = TableDefinition::new("config_revisions");

/// The one key [`MARK`] holds, to the current mark, as JSON.
const CURRENT: TableDefinition<'static, &str, &str> = TableDefinition::new("config_current");

/// The revision number to the approvals it holds, as a JSON array.
const APPROVALS: TableDefinition<'static, u64, &str> = TableDefinition::new("config_approvals");

/// The key the schema table records the revisions' layout under.
pub const LAYOUT: &str = "config_revisions";

/// The schema version of the layout this binary writes and reads.
pub const VERSION: u32 = 2;

/// The key of the current mark.
const MARK: &str = "current";

/// The configuration revisions in one `redb` file.
#[derive(Debug)]
pub struct RedbRevisions {
    database: Database,
    path: PathBuf,
}

impl RedbRevisions {
    /// Opens the store at `path`, creating the file and its tables when they
    /// do not exist.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the file cannot be opened or created, or
    /// is held open by another process, and [`StoreError::Newer`] when it
    /// records a newer layout than this binary knows; the file is then left
    /// as it was.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let database = open_prepared(path, LAYOUT, prepare, StoreError::backend)?;
        Ok(Self {
            database,
            path: path.to_path_buf(),
        })
    }

    /// The file the store was opened at.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Creates the tables over the layout at version `found`, refusing a newer
/// one, and returns the version it writes.
fn prepare(write: &WriteTransaction, found: u32) -> Result<u32, StoreError> {
    if found > VERSION {
        return Err(StoreError::Newer {
            found,
            supported: VERSION,
        });
    }
    write
        .open_table(REVISIONS)
        .map(drop)
        .map_err(StoreError::backend)?;
    write
        .open_table(CURRENT)
        .map(drop)
        .map_err(StoreError::backend)?;
    write
        .open_table(APPROVALS)
        .map(drop)
        .map_err(StoreError::backend)?;
    Ok(VERSION)
}

/// The newest revision number `table` holds.
fn newest(table: &impl ReadableTable<u64, &'static str>) -> Result<Option<RevisionId>, StoreError> {
    match table.last().map_err(StoreError::backend)? {
        None => Ok(None),
        Some((key, _)) => RevisionId::new(key.value())
            .map(Some)
            .ok_or(StoreError::Corrupt(Corruption::ZeroNumber)),
    }
}

impl RevisionStore for RedbRevisions {
    fn save(&self, base: Option<RevisionId>, draft: Draft) -> Result<Saved, StoreError> {
        let write = self.database.begin_write().map_err(StoreError::backend)?;
        let saved = {
            let mut table = write.open_table(REVISIONS).map_err(StoreError::backend)?;
            let head = newest(&table)?;
            if head == base {
                match RevisionId::after(head) {
                    Some(id) => {
                        let revision = draft.numbered(id, base);
                        let json = serde_json::to_string(&revision).map_err(StoreError::backend)?;
                        table
                            .insert(id.get(), json.as_str())
                            .map_err(StoreError::backend)?;
                        Saved::Stored(revision)
                    }
                    None => Saved::Exhausted,
                }
            } else {
                Saved::Stale { head }
            }
        };
        match saved {
            Saved::Stored(_) => write.commit().map_err(StoreError::backend)?,
            Saved::Stale { .. } | Saved::Exhausted => write.abort().map_err(StoreError::backend)?,
        }
        Ok(saved)
    }

    fn revision(&self, id: RevisionId) -> Result<Option<Revision>, StoreError> {
        let read = self.database.begin_read().map_err(StoreError::backend)?;
        let table = read.open_table(REVISIONS).map_err(StoreError::backend)?;
        let held = table.get(id.get()).map_err(StoreError::backend)?;
        held.map(|json| serde_json::from_str(json.value()).map_err(StoreError::corrupt))
            .transpose()
    }

    fn history(&self) -> Result<Vec<Revision>, StoreError> {
        let read = self.database.begin_read().map_err(StoreError::backend)?;
        let table = read.open_table(REVISIONS).map_err(StoreError::backend)?;
        let mut history = Vec::new();
        for row in table.iter().map_err(StoreError::backend)? {
            let (_, json) = row.map_err(StoreError::backend)?;
            history.push(serde_json::from_str(json.value()).map_err(StoreError::corrupt)?);
        }
        Ok(history)
    }

    fn head(&self) -> Result<Option<RevisionId>, StoreError> {
        let read = self.database.begin_read().map_err(StoreError::backend)?;
        let table = read.open_table(REVISIONS).map_err(StoreError::backend)?;
        newest(&table)
    }

    fn current(&self) -> Result<Option<Current>, StoreError> {
        let read = self.database.begin_read().map_err(StoreError::backend)?;
        let table = read.open_table(CURRENT).map_err(StoreError::backend)?;
        let held = table.get(MARK).map_err(StoreError::backend)?;
        held.map(|json| serde_json::from_str(json.value()).map_err(StoreError::corrupt))
            .transpose()
    }

    fn swap_current(
        &self,
        expected: Option<RevisionId>,
        next: &Current,
    ) -> Result<Swapped, StoreError> {
        let json = serde_json::to_string(next).map_err(StoreError::backend)?;
        let write = self.database.begin_write().map_err(StoreError::backend)?;
        let swapped = {
            let mut table = write.open_table(CURRENT).map_err(StoreError::backend)?;
            let held = table
                .get(MARK)
                .map_err(StoreError::backend)?
                .map(|json| {
                    serde_json::from_str::<Current>(json.value()).map_err(StoreError::corrupt)
                })
                .transpose()?
                .map(|current| current.revision);
            if held == expected {
                table
                    .insert(MARK, json.as_str())
                    .map_err(StoreError::backend)?;
                Swapped::Swapped
            } else {
                Swapped::Stale { current: held }
            }
        };
        match swapped {
            Swapped::Swapped => write.commit().map_err(StoreError::backend)?,
            Swapped::Stale { .. } => write.abort().map_err(StoreError::backend)?,
        }
        Ok(swapped)
    }

    fn approvals(&self, id: RevisionId) -> Result<Vec<Signed>, StoreError> {
        let read = self.database.begin_read().map_err(StoreError::backend)?;
        let table = read.open_table(APPROVALS).map_err(StoreError::backend)?;
        let held = table.get(id.get()).map_err(StoreError::backend)?;
        held.map(|json| serde_json::from_str(json.value()).map_err(StoreError::corrupt))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    fn approve(&self, id: RevisionId, approval: &Signed) -> Result<(), StoreError> {
        let write = self.database.begin_write().map_err(StoreError::backend)?;
        {
            let mut table = write.open_table(APPROVALS).map_err(StoreError::backend)?;
            let mut approvals: Vec<Signed> = table
                .get(id.get())
                .map_err(StoreError::backend)?
                .map(|json| serde_json::from_str(json.value()).map_err(StoreError::corrupt))
                .transpose()?
                .unwrap_or_default();
            if !approvals.iter().any(|held| held.by == approval.by) {
                approvals.push(approval.clone());
                let json = serde_json::to_string(&approvals).map_err(StoreError::backend)?;
                table
                    .insert(id.get(), json.as_str())
                    .map_err(StoreError::backend)?;
            }
        }
        write.commit().map_err(StoreError::backend)
    }
}
