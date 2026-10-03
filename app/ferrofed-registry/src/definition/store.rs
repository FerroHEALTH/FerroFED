// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The durable store of stored-query definitions, and the in-memory view
//! every read goes through.
//!
//! A stored version is immutable and outlives the process, because a client
//! that registered it invokes it by name after a restart, and a second `PUT`
//! refused before a restart is refused after it too (§12.7, N44). So the
//! store's one write is [`DefinitionStore::insert_if_absent`], which refuses
//! a held name and version atomically, and its one read loads every
//! definition into [`Definitions`] when the gateway starts. A version never
//! changes once held, so the in-memory view never needs invalidating. This
//! crate holds the interface only: no storage implementation is reachable
//! from it (no specification governs the storage: our own design).

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock};

use crate::definition::{QueryName, QueryVersion, StoredDefinition, VersionPattern};

/// What an insert did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Insertion {
    /// The definition is stored.
    Stored,
    /// A definition of the same name and version is already held, and it
    /// stands unchanged (§12.7, N44).
    Held,
}

/// A store failure, carrying the backend's own error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// The backend could not read or write.
    #[error("the stored-query store could not be read or written")]
    Backend(#[source] Box<dyn Error + Send + Sync>),
    /// A held row does not read as a definition: the store is damaged.
    #[error("the stored-query store holds a row that is not a stored definition")]
    Corrupt(#[source] Box<dyn Error + Send + Sync>),
}

/// The durable store of stored-query definitions.
///
/// An implementation lives outside the clinical path: no type that executes
/// a federated query holds one.
pub trait DefinitionStore: fmt::Debug + Send + Sync {
    /// Stores `definition` unless a definition of its name and version is
    /// held, in one atomic step, so two inserts of one name and version
    /// store exactly one of them (§12.7, N44).
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the backend cannot complete the insert; nothing
    /// is stored then.
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError>;

    /// Every definition the store holds.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the backend cannot be read, or a held row does
    /// not read as a definition.
    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError>;
}

/// Every held definition, by name and then by version.
type Held = BTreeMap<QueryName, BTreeMap<QueryVersion, Arc<StoredDefinition>>>;

/// The stored-query definitions: the durable store, and the in-memory view
/// every read is answered from.
pub struct Definitions {
    store: Box<dyn DefinitionStore>,
    held: RwLock<Held>,
}

impl Definitions {
    /// Opens the definitions `store` holds.
    ///
    /// # Errors
    ///
    /// The [`StoreError`] of [`DefinitionStore::load`].
    pub fn open(store: Box<dyn DefinitionStore>) -> Result<Self, StoreError> {
        let mut held = Held::new();
        for definition in store.load()? {
            held.entry(definition.name().clone())
                .or_default()
                .insert(definition.version(), Arc::new(definition));
        }
        Ok(Self {
            store,
            held: RwLock::new(held),
        })
    }

    /// Stores `definition` unless its name and version are held (§12.7,
    /// N44).
    ///
    /// The store decides, so the refusal holds across a restart. This call
    /// writes to the store: a caller on an asynchronous runtime makes it on a
    /// thread that may block.
    ///
    /// # Errors
    ///
    /// The [`StoreError`] of [`DefinitionStore::insert_if_absent`].
    pub fn insert(&self, definition: StoredDefinition) -> Result<Insertion, StoreError> {
        let insertion = self.store.insert_if_absent(&definition)?;
        if insertion == Insertion::Stored {
            self.write()
                .entry(definition.name().clone())
                .or_default()
                .insert(definition.version(), Arc::new(definition));
        }
        Ok(insertion)
    }

    /// The definition of `name` that `version` selects: that version, the
    /// highest one a prefix matches, or with no version the highest held
    /// (ITS-REST Query API, the `version` path parameter).
    #[must_use]
    pub fn find(
        &self,
        name: &QueryName,
        version: Option<&VersionPattern>,
    ) -> Option<Arc<StoredDefinition>> {
        let held = self.read();
        let versions = held.get(name)?;
        versions
            .iter()
            .rev()
            .find(|(held, _)| version.is_none_or(|pattern| pattern.matches(held)))
            .map(|(_, definition)| Arc::clone(definition))
    }

    /// Every version of every definition whose name starts with `pattern`,
    /// by name and then by version (ITS-REST Definition API,
    /// `definition_query_list`).
    #[must_use]
    pub fn list(&self, pattern: &str) -> Vec<Arc<StoredDefinition>> {
        self.read()
            .iter()
            .filter(|(name, _)| name.as_str().starts_with(pattern))
            .flat_map(|(_, versions)| versions.values().map(Arc::clone))
            .collect()
    }

    /// How many definitions are held, every version counted.
    #[must_use]
    pub fn len(&self) -> usize {
        self.read().values().map(BTreeMap::len).sum()
    }

    /// Whether no definition is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.read().is_empty()
    }

    // NOTE: no specification governs this: our own design; a writer only
    // inserts a whole entry, so a poisoned view still holds whole entries.
    fn read(&self) -> std::sync::RwLockReadGuard<'_, Held> {
        self.held.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Held> {
        self.held.write().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for Definitions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Definitions")
            .field("store", &self.store)
            .field("held", &self.len())
            .finish()
    }
}
