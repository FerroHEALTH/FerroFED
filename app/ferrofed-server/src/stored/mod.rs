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
//! A store can hold a row the gateway never wrote, from a restore, a manual
//! insert or an older version, so [`open`] wraps every backend in
//! [`Admitted`]: every definition a backend reads passes the admission a
//! `PUT` passes ([`admit`]), and a read holding one that fails it is
//! refused, so that definition is never served or run. No specification
//! governs the storage: our own design.

use std::fmt;
use std::ops::Range;

use ferrofed_registry::definition::store::{DefinitionStore, Insertion, StoreError};
use ferrofed_registry::definition::{QueryName, QueryVersion, StoredDefinition};
use openehr_federation::aql::Context;
use openehr_federation::aql::definition::{Definition, SubjectOrigin};
use openehr_federation::aql::refusal::Refusal;

use crate::config::stored_queries::Store;
use crate::facade::security;

pub mod embedded;
pub mod files;
#[cfg(feature = "postgres")]
pub mod postgres;

/// Why the registry refuses a definition's text.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Inadmissible {
    /// The admission a `PUT` passes refuses the text (§12.7, §7.1).
    #[error("the definition is refused as a stored-query definition")]
    Refused(#[source] Refusal),
    /// The text names its patient by a literal, which the registry never
    /// holds (§5.4.1, N33).
    #[error(
        "the definition names its patient by a literal; a stored query names it through a $parameter"
    )]
    SubjectLiteral {
        /// Where the patient predicate was written, when the parser gave a
        /// position; never the value (§5.4.3).
        at: Option<Range<usize>>,
    },
}

/// A definition a store holds that the registry refuses to serve or run,
/// named by its qualified name and version, its text never quoted.
#[derive(Debug, thiserror::Error)]
#[error("the held definition {name} at {version} is refused")]
pub struct HeldRefused {
    /// The qualified name the definition is held under.
    pub name: QueryName,
    /// The version it is held at.
    pub version: QueryVersion,
    /// Why it is refused.
    #[source]
    pub reason: Inadmissible,
}

/// Admits `aql` as a stored-query definition under `context`, as a `PUT`
/// admits its body: it analyses as a façade query, and it names no patient
/// by a literal (§12.7, §5.4.1, N33).
///
/// # Errors
///
/// [`Inadmissible::Refused`] with the [`Refusal`] of [`Definition::admit`],
/// and [`Inadmissible::SubjectLiteral`] for a text that names its patient by
/// a literal.
pub fn admit(aql: &str, context: &Context) -> Result<Definition, Inadmissible> {
    let admitted = Definition::admit(aql, context).map_err(Inadmissible::Refused)?;
    if let Some(SubjectOrigin::Literal { at }) = admitted.subject() {
        return Err(Inadmissible::SubjectLiteral { at: at.clone() });
    }
    Ok(admitted)
}

/// A backend whose every read passes [`admit`], so no definition it holds
/// reaches the registry without the check a `PUT` passes.
pub struct Admitted {
    store: Box<dyn DefinitionStore>,
    context: Context,
}

impl Admitted {
    /// Wraps `store`, admitting each definition it reads under `context`.
    #[must_use]
    pub fn new(store: Box<dyn DefinitionStore>, context: Context) -> Self {
        Self { store, context }
    }

    /// `definitions`, when every one of them is admitted.
    fn checked(
        &self,
        definitions: Vec<StoredDefinition>,
    ) -> Result<Vec<StoredDefinition>, StoreError> {
        for definition in &definitions {
            if let Err(reason) = admit(definition.aql(), &self.context) {
                security::held_definition_refused(definition.name(), definition.version());
                return Err(StoreError::Corrupt(Box::new(HeldRefused {
                    name: definition.name().clone(),
                    version: definition.version(),
                    reason,
                })));
            }
        }
        Ok(definitions)
    }
}

impl DefinitionStore for Admitted {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        self.store.insert_if_absent(definition)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        self.checked(self.store.load()?)
    }

    fn load_named(&self, name: &QueryName) -> Result<Vec<StoredDefinition>, StoreError> {
        self.checked(self.store.load_named(name)?)
    }

    fn is_shared(&self) -> bool {
        self.store.is_shared()
    }

    fn is_read_only(&self) -> bool {
        self.store.is_read_only()
    }
}

impl fmt::Debug for Admitted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Admitted")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

/// Opens the store `store` names, every definition it reads admitted under
/// `context` as a `PUT` would admit it ([`Admitted`]).
///
/// # Errors
///
/// The [`StoreError`] of the backend's own `open`, and
/// [`StoreError::Backend`] for a PostgreSQL store in a build without the
/// `postgres` feature, which configuration refuses first.
pub fn open(store: &Store, context: &Context) -> Result<Box<dyn DefinitionStore>, StoreError> {
    let opened: Box<dyn DefinitionStore> = match store {
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
    };
    Ok(Box::new(Admitted::new(opened, context.clone())))
}
