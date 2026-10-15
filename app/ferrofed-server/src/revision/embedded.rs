// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The embedded revision store, over `redb`, for one gateway process.
//!
//! One table maps a revision number to the revision as JSON, a second holds
//! the current mark as JSON, and a third records the schema version of the
//! layout. A save reads the newest number and writes the next in one write
//! transaction, and a move of the current mark reads the mark and writes it
//! in one, so neither races; `redb` is an ACID embedded store
//! (<https://docs.rs/redb/4.3.0/redb/>). The file holds configuration text
//! with every secret only as a `_file` reference, and never a patient
//! identifier. No specification governs the storage: our own design.

use std::path::{Path, PathBuf};

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction};

use ferrofed_registry::revision::{Current, Revision, RevisionId};

use crate::revision::{Draft, RevisionStore, Saved, StoreError, Swapped};

/// The revision number to the revision, as JSON.
const REVISIONS: TableDefinition<'static, u64, &str> = TableDefinition::new("config_revisions");

/// The one key [`MARK`] holds, to the current mark, as JSON.
const CURRENT: TableDefinition<'static, &str, &str> = TableDefinition::new("config_current");

/// The schema version of each layout the file holds, by name.
const SCHEMA: TableDefinition<'static, &str, u32> = TableDefinition::new("ferrofed_schema");

/// The key the schema table records the revisions' layout under.
pub const LAYOUT: &str = "config_revisions";

/// The schema version of the layout this binary writes and reads.
pub const VERSION: u32 = 1;

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
        let database = Database::create(path).map_err(backend)?;
        let write = database.begin_write().map_err(backend)?;
        match prepare(&write) {
            Ok(()) => write.commit().map_err(backend)?,
            Err(error) => {
                write.abort().map_err(backend)?;
                return Err(error);
            }
        }
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

/// Creates the tables and records the layout's version, refusing a newer
/// one.
fn prepare(write: &WriteTransaction) -> Result<(), StoreError> {
    let mut versions = write.open_table(SCHEMA).map_err(backend)?;
    let found = versions
        .get(LAYOUT)
        .map_err(backend)?
        .map_or(0, |version| version.value());
    if found > VERSION {
        return Err(StoreError::Newer {
            found,
            supported: VERSION,
        });
    }
    write.open_table(REVISIONS).map(drop).map_err(backend)?;
    write.open_table(CURRENT).map(drop).map_err(backend)?;
    versions.insert(LAYOUT, VERSION).map_err(backend)?;
    Ok(())
}

/// The newest revision number `table` holds.
fn newest(table: &impl ReadableTable<u64, &'static str>) -> Result<Option<RevisionId>, StoreError> {
    match table.last().map_err(backend)? {
        None => Ok(None),
        Some((key, _)) => RevisionId::new(key.value())
            .map(Some)
            .ok_or_else(|| corrupt(ZeroNumber)),
    }
}

/// A row numbered zero, which no save writes.
#[derive(Debug, thiserror::Error)]
#[error("a revision is numbered zero")]
struct ZeroNumber;

impl RevisionStore for RedbRevisions {
    fn save(&self, base: Option<RevisionId>, draft: Draft) -> Result<Saved, StoreError> {
        let write = self.database.begin_write().map_err(backend)?;
        let saved = {
            let mut table = write.open_table(REVISIONS).map_err(backend)?;
            let head = newest(&table)?;
            if head == base {
                match RevisionId::after(head) {
                    Some(id) => {
                        let revision = draft.numbered(id, base);
                        let json = serde_json::to_string(&revision).map_err(backend)?;
                        table.insert(id.get(), json.as_str()).map_err(backend)?;
                        Saved::Stored(revision)
                    }
                    None => Saved::Exhausted,
                }
            } else {
                Saved::Stale { head }
            }
        };
        match saved {
            Saved::Stored(_) => write.commit().map_err(backend)?,
            Saved::Stale { .. } | Saved::Exhausted => write.abort().map_err(backend)?,
        }
        Ok(saved)
    }

    fn revision(&self, id: RevisionId) -> Result<Option<Revision>, StoreError> {
        let read = self.database.begin_read().map_err(backend)?;
        let table = read.open_table(REVISIONS).map_err(backend)?;
        let held = table.get(id.get()).map_err(backend)?;
        held.map(|json| serde_json::from_str(json.value()).map_err(corrupt))
            .transpose()
    }

    fn history(&self) -> Result<Vec<Revision>, StoreError> {
        let read = self.database.begin_read().map_err(backend)?;
        let table = read.open_table(REVISIONS).map_err(backend)?;
        let mut history = Vec::new();
        for row in table.iter().map_err(backend)? {
            let (_, json) = row.map_err(backend)?;
            history.push(serde_json::from_str(json.value()).map_err(corrupt)?);
        }
        Ok(history)
    }

    fn head(&self) -> Result<Option<RevisionId>, StoreError> {
        let read = self.database.begin_read().map_err(backend)?;
        let table = read.open_table(REVISIONS).map_err(backend)?;
        newest(&table)
    }

    fn current(&self) -> Result<Option<Current>, StoreError> {
        let read = self.database.begin_read().map_err(backend)?;
        let table = read.open_table(CURRENT).map_err(backend)?;
        let held = table.get(MARK).map_err(backend)?;
        held.map(|json| serde_json::from_str(json.value()).map_err(corrupt))
            .transpose()
    }

    fn swap_current(
        &self,
        expected: Option<RevisionId>,
        next: &Current,
    ) -> Result<Swapped, StoreError> {
        let json = serde_json::to_string(next).map_err(backend)?;
        let write = self.database.begin_write().map_err(backend)?;
        let swapped = {
            let mut table = write.open_table(CURRENT).map_err(backend)?;
            let held = table
                .get(MARK)
                .map_err(backend)?
                .map(|json| serde_json::from_str::<Current>(json.value()).map_err(corrupt))
                .transpose()?
                .map(|current| current.revision);
            if held == expected {
                table.insert(MARK, json.as_str()).map_err(backend)?;
                Swapped::Swapped
            } else {
                Swapped::Stale { current: held }
            }
        };
        match swapped {
            Swapped::Swapped => write.commit().map_err(backend)?,
            Swapped::Stale { .. } => write.abort().map_err(backend)?,
        }
        Ok(swapped)
    }
}

/// The backend failure `error`.
fn backend(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Backend(Box::new(error))
}

/// A held row that does not read as a revision, for `error`.
fn corrupt(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Corrupt(Box::new(error))
}
