// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query definitions in FerroFED's own store, held for every
//! replica (§12.7, N44).
//!
//! They live in the table `ferrofed.stored_query_definition` of the store
//! ([`crate::store`]), which every gateway replica uses. The primary key is the qualified name and the version. An insert is
//! `INSERT … ON CONFLICT (name, version) DO NOTHING`, and the affected-row
//! count says whether it stored the row or found the pair held, so two
//! replicas inserting one pair at once store exactly one of them, in one
//! statement, with no read before the write
//! (<https://www.postgresql.org/docs/18/sql-insert.html#SQL-ON-CONFLICT>).
//! A definition stored with no version takes the trait's own
//! [`DefinitionStore::insert_assigned`]: it reads the versions of its name
//! and inserts the next with that statement, and when another replica stored
//! that version first it reads again, a bounded number of times, so two
//! replicas racing on one name store two versions or one is refused, and
//! neither replaces the other's (§12.7 stored-query-versioning). The table is
//! the store's migration 1 ([`crate::store::migrations`]). No specification
//! governs the storage: our own design.

use std::sync::Arc;

use ferrofed_registry::definition::StoredDefinition;
use ferrofed_registry::definition::store::{
    DefinitionStore, Field, FieldFault, Insertion, StoreError,
};
use jiff::Timestamp;
use openehr_its::rest::stored_query::QualifiedQueryName;
use tokio_postgres::{Client, Row};

use crate::store::Store;
use crate::store::connection::backend;

/// The table, qualified by its schema.
pub const TABLE: &str = "ferrofed.stored_query_definition";

/// Stores a definition unless its name and version are held.
const INSERT: &str = "INSERT INTO ferrofed.stored_query_definition (name, version, saved, aql)
VALUES ($1, $2, $3, $4) ON CONFLICT (name, version) DO NOTHING";

/// Every definition.
const SELECT_ALL: &str = "SELECT name, version, saved, aql FROM ferrofed.stored_query_definition";

/// Every version of one name.
const SELECT_NAMED: &str =
    "SELECT name, version, saved, aql FROM ferrofed.stored_query_definition WHERE name = $1";

/// A failure of the definitions' own, beside the client's errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PostgresError {
    /// An insert reported a row count other than one or none.
    #[error("an insert reported {0} rows, where one or none is possible")]
    Rows(u64),
}

/// The stored-query definitions in FerroFED's own store.
#[derive(Debug)]
pub struct PostgresStore {
    store: Arc<Store>,
}

impl PostgresStore {
    /// The definitions in `store`, which opening migrated.
    #[must_use]
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}

impl DefinitionStore for PostgresStore {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        let definition = definition.clone();
        self.store
            .call(move |client| Box::pin(insert(client, definition)))
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        self.store.call(move |client| Box::pin(load(client, None)))
    }

    fn load_named(&self, name: &QualifiedQueryName) -> Result<Vec<StoredDefinition>, StoreError> {
        let name = Some(name.clone());
        self.store.call(move |client| Box::pin(load(client, name)))
    }

    fn is_shared(&self) -> bool {
        true
    }
}

/// Inserts `definition` unless its name and version are held.
async fn insert(client: &Client, definition: StoredDefinition) -> Result<Insertion, StoreError> {
    let name = definition.name().to_string();
    let version = definition.version().to_string();
    let saved = definition.saved().to_string();
    let rows = client
        .execute(
            INSERT,
            &[
                &name.as_str(),
                &version.as_str(),
                &saved.as_str(),
                &definition.aql(),
            ],
        )
        .await
        .map_err(backend)?;
    match rows {
        1 => Ok(Insertion::Stored),
        0 => Ok(Insertion::Held),
        other => Err(backend(PostgresError::Rows(other))),
    }
}

/// Every definition, or every version of `name`.
async fn load(
    client: &Client,
    name: Option<QualifiedQueryName>,
) -> Result<Vec<StoredDefinition>, StoreError> {
    let rows = match &name {
        Some(name) => client.query(SELECT_NAMED, &[&name.to_string()]).await,
        None => client.query(SELECT_ALL, &[]).await,
    }
    .map_err(backend)?;
    rows.iter().map(definition).collect()
}

/// The definition one row holds.
fn definition(row: &Row) -> Result<StoredDefinition, StoreError> {
    let text = |(index, field): (usize, Field)| {
        row.try_get::<usize, &str>(index)
            .map_err(fault(field, FieldFault::Type))
    };
    Ok(StoredDefinition::new(
        text((0, Field::Name))?
            .parse()
            .map_err(fault(Field::Name, FieldFault::Form))?,
        text((1, Field::Version))?
            .parse()
            .map_err(fault(Field::Version, FieldFault::Form))?,
        text((3, Field::Aql))?.to_owned(),
        text((2, Field::Saved))?
            .parse::<Timestamp>()
            .map_err(fault(Field::Saved, FieldFault::Form))?,
    ))
}

/// The damaged-store error of a held `field` with the fault `kind`, for any
/// reader error.
fn fault<E>(field: Field, kind: FieldFault) -> impl FnOnce(E) -> StoreError {
    // NOTE: no specification governs this: our own design; the reader's error
    // is dropped because it can quote the held text, and the fault names it.
    move |_| StoreError::unreadable(field, kind)
}
