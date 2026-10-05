// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query registry's embedded store, over `redb`, for one gateway
//! process (§12.7, N44).
//!
//! One table maps a qualified query name and a version to the instant the
//! definition was stored and its AQL. An insert reads the key and writes it
//! in one write transaction, so a second `PUT` of a held name and version is
//! refused atomically, which is the immutability rule with no race; `redb` is
//! an ACID embedded store (<https://docs.rs/redb/4.3.0/redb/>). The file holds
//! names, versions, instants and parameterised AQL, and never a patient
//! identifier: a definition names its patient through a `$parameter`, and an
//! invocation's values are never written. No specification governs the
//! storage: our own design.

use std::path::{Path, PathBuf};

use ferrofed_registry::definition::StoredDefinition;
use ferrofed_registry::definition::store::{DefinitionStore, Insertion, StoreError};
use jiff::Timestamp;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

/// The qualified name and the version, to the RFC 3339 instant the
/// definition was stored and its AQL.
const DEFINITIONS: TableDefinition<'static, (&str, &str), (&str, &str)> =
    TableDefinition::new("stored_query_definitions");

/// The stored-query definitions in one `redb` file.
#[derive(Debug)]
pub struct RedbStore {
    database: Database,
    path: PathBuf,
}

impl RedbStore {
    /// Opens the store at `path`, creating the file and its table when they
    /// do not exist.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the file cannot be opened or created, is
    /// held open by another process, or the table cannot be created.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let database = Database::create(path).map_err(backend)?;
        let write = database.begin_write().map_err(backend)?;
        {
            let _table = write.open_table(DEFINITIONS).map_err(backend)?;
        }
        write.commit().map_err(backend)?;
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

impl DefinitionStore for RedbStore {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        let name = definition.name().as_str();
        let version = definition.version().to_string();
        let saved = definition.saved().to_string();
        let write = self.database.begin_write().map_err(backend)?;
        let insertion = {
            let mut table = write.open_table(DEFINITIONS).map_err(backend)?;
            if table
                .get((name, version.as_str()))
                .map_err(backend)?
                .is_some()
            {
                Insertion::Held
            } else {
                table
                    .insert((name, version.as_str()), (saved.as_str(), definition.aql()))
                    .map_err(backend)?;
                Insertion::Stored
            }
        };
        match insertion {
            Insertion::Stored => write.commit().map_err(backend)?,
            Insertion::Held => write.abort().map_err(backend)?,
        }
        Ok(insertion)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        let read = self.database.begin_read().map_err(backend)?;
        let table = read.open_table(DEFINITIONS).map_err(backend)?;
        let mut definitions = Vec::new();
        for row in table.iter().map_err(backend)? {
            let (key, value) = row.map_err(backend)?;
            let (name, version) = key.value();
            let (saved, aql) = value.value();
            definitions.push(StoredDefinition::new(
                name.parse().map_err(corrupt)?,
                version.parse().map_err(corrupt)?,
                aql.to_owned(),
                saved.parse::<Timestamp>().map_err(corrupt)?,
            ));
        }
        Ok(definitions)
    }
}

/// The backend failure `error`.
fn backend(error: impl Into<redb::Error>) -> StoreError {
    StoreError::Backend(Box::new(error.into()))
}

/// A held row that does not read as a definition, for `error`.
fn corrupt(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Corrupt(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::RedbStore;
    use ferrofed_registry::definition::StoredDefinition;
    use ferrofed_registry::definition::store::{DefinitionStore, Insertion};
    use jiff::Timestamp;

    fn definition(aql: &str) -> StoredDefinition {
        StoredDefinition::new(
            "org.example::q".parse().unwrap(),
            "1.0.0".parse().unwrap(),
            aql.to_owned(),
            Timestamp::UNIX_EPOCH,
        )
    }

    #[test]
    fn a_held_key_is_refused_and_the_first_definition_stands_after_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("definitions.redb");
        {
            let store = RedbStore::open(&path).unwrap();
            assert_eq!(path, store.path());
            assert_eq!(
                Insertion::Stored,
                store.insert_if_absent(&definition("SELECT 1")).unwrap()
            );
            assert_eq!(
                Insertion::Held,
                store.insert_if_absent(&definition("SELECT 2")).unwrap()
            );
        }
        let reopened = RedbStore::open(&path).unwrap();
        assert_eq!(vec![definition("SELECT 1")], reopened.load().unwrap());
        assert_eq!(
            Insertion::Held,
            reopened.insert_if_absent(&definition("SELECT 3")).unwrap(),
            "§12.7, N44: the refusal holds across a restart"
        );
    }

    #[test]
    fn a_file_held_open_by_another_store_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("definitions.redb");
        let _held = RedbStore::open(&path).unwrap();
        assert!(RedbStore::open(&path).is_err(), "one process at a time");
    }
}
