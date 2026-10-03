// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query registry's stores (§12.7, N44).
//!
//! The embedded `redb` file serves one gateway process ([`embedded`]), a
//! PostgreSQL database serves several replicas that share it (`postgres`,
//! behind the `postgres` feature), and read-only definition files serve
//! instances that share no database ([`files`]).
//!
//! Each holds names, versions, instants and parameterised AQL, and never a
//! patient identifier: a definition names its patient through a
//! `$parameter`, and an invocation's values are never written (§5.4.1, N33).
//! No specification governs the storage: our own design.

use ferrofed_registry::definition::store::{DefinitionStore, StoreError};
use openehr_federation::aql::Context;

use crate::config::stored_queries::Store;

pub mod embedded;
pub mod files;
#[cfg(feature = "postgres")]
pub mod postgres;

/// Opens the store `store` names; `context` admits each definition a
/// read-only directory holds, as a `PUT` would admit it.
///
/// # Errors
///
/// The [`StoreError`] of the backend's own `open`, and
/// [`StoreError::Backend`] for a PostgreSQL store in a build without the
/// `postgres` feature, which configuration refuses first.
pub fn open(store: &Store, context: &Context) -> Result<Box<dyn DefinitionStore>, StoreError> {
    Ok(match store {
        Store::Redb(path) => Box::new(embedded::RedbStore::open(path)?),
        Store::Files(path) => Box::new(files::FilesStore::open(path, context)?),
        #[cfg(feature = "postgres")]
        Store::Postgres(url) => Box::new(postgres::PostgresStore::open(url)?),
        #[cfg(not(feature = "postgres"))]
        Store::Postgres(_) => {
            return Err(StoreError::Backend(Box::new(
                crate::config::error::Error::StoreBackendUnavailable {
                    backend: store.backend(),
                },
            )));
        }
    })
}
