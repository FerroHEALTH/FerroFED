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
//! invocation's values are never written. A second table records the
//! schema version the file holds ([`schema`]): an
//! older file is migrated forward when it is opened, and a newer one is
//! refused. No specification governs the storage: our own design.

use std::path::{Path, PathBuf};

use ferrofed_registry::definition::StoredDefinition;
use ferrofed_registry::definition::store::{DefinitionStore, Insertion, StoreError};
use jiff::Timestamp;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction};

use crate::stored::schema::{self, SchemaError};

/// The qualified name and the version, to the RFC 3339 instant the
/// definition was stored and its AQL.
const DEFINITIONS: TableDefinition<'static, (&str, &str), (&str, &str)> =
    TableDefinition::new("stored_query_definitions");

/// The schema version each layout the file holds is at, by the name of its
/// table.
const SCHEMA: TableDefinition<'static, &str, u32> = TableDefinition::new("ferrofed_schema");

/// The key [`SCHEMA`] records the definitions' layout under.
const LAYOUT: &str = "stored_query_definitions";

/// The stored-query definitions in one `redb` file.
#[derive(Debug)]
pub struct RedbStore {
    database: Database,
    path: PathBuf,
}

impl RedbStore {
    /// Opens the store at `path`, creating the file and its tables when they
    /// do not exist, and migrating an older layout forward.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the file cannot be opened or created, is
    /// held open by another process, or a table cannot be created, and
    /// [`StoreError::Backend`] over [`SchemaError::Newer`] when the file
    /// records a newer schema than this binary knows; the file is then left
    /// as it was.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let database = Database::create(path).map_err(backend)?;
        let write = database.begin_write().map_err(backend)?;
        let migrated = migrate(&write);
        match migrated {
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

/// Brings the layout `write` sees to [`schema::CURRENT`], recording the
/// version it reaches.
fn migrate(write: &WriteTransaction) -> Result<(), StoreError> {
    let mut versions = write.open_table(SCHEMA).map_err(backend)?;
    let found = versions
        .get(LAYOUT)
        .map_err(backend)?
        .map_or(0, |version| version.value());
    let pending =
        schema::pending("redb", found).map_err(|newer| StoreError::Backend(Box::new(newer)))?;
    for step in pending {
        apply(write, step)?;
        versions.insert(LAYOUT, step).map_err(backend)?;
    }
    Ok(())
}

/// Runs the migration that brings the layout to version `step`.
fn apply(write: &WriteTransaction, step: u32) -> Result<(), StoreError> {
    match step {
        // Version 1: the definitions table.
        1 => write.open_table(DEFINITIONS).map(drop).map_err(backend),
        version => Err(StoreError::Backend(Box::new(SchemaError::NoMigration {
            store: "redb",
            version,
        }))),
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
    use super::{DEFINITIONS, LAYOUT, RedbStore, SCHEMA};
    use crate::stored::schema::{self, SchemaError};
    use ferrofed_registry::definition::StoredDefinition;
    use ferrofed_registry::definition::store::StoreError;
    use ferrofed_registry::definition::store::{DefinitionStore, Insertion};
    use jiff::Timestamp;
    use redb::ReadableDatabase;

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

    /// The schema version `path` records, opened past the store.
    fn recorded(path: &std::path::Path) -> Option<u32> {
        let database = redb::Database::open(path).unwrap();
        let read = database.begin_read().unwrap();
        let table = read.open_table(SCHEMA).unwrap();
        table.get(LAYOUT).unwrap().map(|version| version.value())
    }

    #[test]
    fn a_new_file_records_the_current_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("definitions.redb");
        drop(RedbStore::open(&path).unwrap());
        assert_eq!(Some(schema::CURRENT), recorded(&path));
    }

    #[test]
    fn a_file_written_before_versions_were_recorded_is_migrated_and_keeps_its_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("definitions.redb");
        {
            let database = redb::Database::create(&path).unwrap();
            let write = database.begin_write().unwrap();
            {
                let mut table = write.open_table(DEFINITIONS).unwrap();
                table
                    .insert(
                        ("org.example::q", "1.0.0"),
                        ("1970-01-01T00:00:00Z", "SELECT 1"),
                    )
                    .unwrap();
            }
            write.commit().unwrap();
        }
        let store = RedbStore::open(&path).unwrap();
        assert_eq!(vec![definition("SELECT 1")], store.load().unwrap());
        drop(store);
        assert_eq!(Some(schema::CURRENT), recorded(&path));
    }

    #[test]
    fn a_file_a_newer_ferrofed_wrote_is_refused_and_left_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("definitions.redb");
        let newer = schema::CURRENT + 1;
        drop(RedbStore::open(&path).unwrap());
        {
            let database = redb::Database::open(&path).unwrap();
            let write = database.begin_write().unwrap();
            write
                .open_table(SCHEMA)
                .unwrap()
                .insert(LAYOUT, newer)
                .unwrap();
            write.commit().unwrap();
        }
        let refused = RedbStore::open(&path).unwrap_err();
        let StoreError::Backend(error) = &refused else {
            panic!("a backend refusal: {refused:?}");
        };
        assert_eq!(
            Some(&SchemaError::Newer {
                store: "redb",
                found: newer,
                supported: schema::CURRENT
            }),
            error.downcast_ref::<SchemaError>()
        );
        assert!(error.to_string().contains("newer FerroFED"), "{error}");
        assert_eq!(Some(newer), recorded(&path), "the file is not rewritten");
    }

    #[test]
    fn a_file_held_open_by_another_store_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("definitions.redb");
        let _held = RedbStore::open(&path).unwrap();
        assert!(RedbStore::open(&path).is_err(), "one process at a time");
    }
}
