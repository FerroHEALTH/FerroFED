// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The durable store of stored-query definitions, and the in-memory view
//! every read goes through.
//!
//! A stored version is immutable and outlives the process, because a client
//! that registered it invokes it by name after a restart, and a second `PUT`
//! refused before a restart is refused after it too (§12.7, N44). So the
//! store's write is [`DefinitionStore::insert_if_absent`], which refuses
//! a held name and version atomically, or, for a definition stored with no
//! version, [`DefinitionStore::insert_assigned`], which stores it at the next
//! version and never replaces one (§12.7 stored-query-versioning), and its
//! read loads every definition
//! into [`Definitions`] when the gateway starts. A version never changes once
//! held, so the in-memory view never needs invalidating. A store several
//! gateway processes share can gain versions another process inserted, so
//! the view over it reads the store again before it answers
//! ([`Definitions::refresh`], [`Definitions::refresh_named`]), and only ever
//! adds what it did not hold. A read-only store refuses every insert. This
//! crate holds the interface only: no storage implementation is reachable
//! from it (no specification governs the storage: our own design).

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock};

use jiff::Timestamp;

use crate::definition::{QueryName, QueryVersion, StoredDefinition, VersionPattern};

/// How many versions [`DefinitionStore::insert_assigned`] assigns, each
/// after another writer stored the one before, before it answers
/// [`Assignment::Contended`].
// NOTE: no specification governs this: our own design; a bound keeps a name
// many writers race on from holding a blocking thread without end.
pub const ASSIGNMENT_ATTEMPTS: usize = 8;

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

/// What an insert at a version the store assigns did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum Assignment {
    /// The definition is stored at the version the store assigned.
    Stored(StoredDefinition),
    /// Another writer stored each version the store assigned first, in each
    /// of its [`ASSIGNMENT_ATTEMPTS`]; nothing is stored.
    Contended,
    /// The patch of the newest held version is the largest a version holds,
    /// so no next version exists; nothing is stored.
    Exhausted,
}

/// A store failure, carrying the backend's own error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// The backend could not read or write.
    #[error("the stored-query store could not be read or written")]
    Backend(#[source] Box<dyn Error + Send + Sync>),
    /// A held definition the gateway refuses: the store is damaged.
    ///
    /// The cause is a refusal of the gateway's own, which names the
    /// definition and the rule it breaks, never its text.
    #[error("the stored-query store holds a row that is not a stored definition")]
    Corrupt(#[source] Box<dyn Error + Send + Sync>),
    /// A field of a held row does not read: the store is damaged.
    #[error("the stored-query store holds a row that does not read: {0}")]
    Unreadable(RowFault),
    /// The store is read-only, so nothing is inserted.
    #[error("the stored-query store is read-only")]
    ReadOnly,
}

/// Which field of a held row a [`RowFault`] is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The qualified query name.
    Name,
    /// The version.
    Version,
    /// The instant it was saved.
    Saved,
    /// The AQL text.
    Aql,
    /// The schema version the store's layout records.
    Layout,
}

/// What is wrong with the field a [`RowFault`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFault {
    /// The text does not read as the field's form.
    Form,
    /// The column does not hold the field's type.
    Type,
    /// The number is outside the field's range.
    Range,
}

/// Where a held row fails to read and how.
///
/// It names the field and the kind of fault only, never the row's text or
/// a reader error that quotes it, so it is safe in a log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowFault {
    /// The field that does not read.
    pub field: Field,
    /// What is wrong with it.
    pub kind: FieldFault,
}

impl fmt::Display for RowFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let field = match self.field {
            Field::Name => "the name",
            Field::Version => "the version",
            Field::Saved => "the saved instant",
            Field::Aql => "the AQL text",
            Field::Layout => "the layout's schema version",
        };
        let kind = match self.kind {
            FieldFault::Form => "does not read as its form",
            FieldFault::Type => "is not of its type",
            FieldFault::Range => "is outside its range",
        };
        write!(f, "{field} {kind}")
    }
}

impl StoreError {
    /// Returns the damaged-store error of `field`, which has the fault
    /// `kind`.
    #[must_use]
    pub fn unreadable(field: Field, kind: FieldFault) -> Self {
        Self::Unreadable(RowFault { field, kind })
    }
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
    /// is stored then. [`StoreError::ReadOnly`] from a read-only store.
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError>;

    /// Stores `aql` under `name`, saved at `saved`, at the version the store
    /// assigns: `1.0.0` for a name it holds no version of, else the next
    /// patch of the newest it holds ([`QueryVersion::assigned_after`];
    /// §12.7 stored-query-versioning). It never replaces a held version.
    ///
    /// The default reads the versions of `name` and inserts at the next one
    /// with [`DefinitionStore::insert_if_absent`]; when another writer stored
    /// that version first, it reads again and assigns the next, at most
    /// [`ASSIGNMENT_ATTEMPTS`] times. Two writers racing on one name so store
    /// two versions, or one of them answers [`Assignment::Contended`]. A
    /// backend that reads and inserts in one transaction overrides it.
    ///
    /// # Errors
    ///
    /// The [`StoreError`] of [`DefinitionStore::load_named`] or
    /// [`DefinitionStore::insert_if_absent`]; nothing is stored then.
    fn insert_assigned(
        &self,
        name: &QueryName,
        aql: &str,
        saved: Timestamp,
    ) -> Result<Assignment, StoreError> {
        for _ in 0..ASSIGNMENT_ATTEMPTS {
            let newest = self
                .load_named(name)?
                .iter()
                .map(StoredDefinition::version)
                .max();
            let Some(version) = QueryVersion::assigned_after(newest) else {
                return Ok(Assignment::Exhausted);
            };
            let definition = StoredDefinition::new(name.clone(), version, aql.to_owned(), saved);
            match self.insert_if_absent(&definition)? {
                Insertion::Stored => return Ok(Assignment::Stored(definition)),
                Insertion::Held => {}
            }
        }
        Ok(Assignment::Contended)
    }

    /// Every definition the store holds.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the backend cannot be read, or a held row does
    /// not read as a definition.
    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError>;

    /// Every version of `name` the store holds.
    ///
    /// The default reads every definition and keeps those of `name`; a
    /// backend that can select by name overrides it.
    ///
    /// # Errors
    ///
    /// The [`StoreError`] of [`DefinitionStore::load`].
    fn load_named(&self, name: &QueryName) -> Result<Vec<StoredDefinition>, StoreError> {
        let mut definitions = self.load()?;
        definitions.retain(|definition| definition.name() == name);
        Ok(definitions)
    }

    /// Whether another process may insert into the store while this one
    /// holds it open, so a view over it reads it again before answering.
    fn is_shared(&self) -> bool {
        false
    }

    /// Whether the store refuses every insert.
    fn is_read_only(&self) -> bool {
        false
    }
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
        add(&mut held, store.load()?);
        Ok(Self {
            store,
            held: RwLock::new(held),
        })
    }

    /// Whether another process may insert into the store, so a read calls
    /// [`Definitions::refresh`] or [`Definitions::refresh_named`] first.
    #[must_use]
    pub fn is_shared(&self) -> bool {
        self.store.is_shared()
    }

    /// Whether the store refuses every insert, so the registry answers no
    /// `PUT`.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.store.is_read_only()
    }

    /// Reads every definition from a shared store again, and adds each
    /// version the view does not hold; a store that is not shared is not
    /// read.
    ///
    /// A held version never changes (§12.7, N44), so nothing the view holds
    /// is replaced. This call reads the store: a caller on an asynchronous
    /// runtime makes it on a thread that may block.
    ///
    /// # Errors
    ///
    /// The [`StoreError`] of [`DefinitionStore::load`]; the view stays as it
    /// was.
    pub fn refresh(&self) -> Result<(), StoreError> {
        if self.is_shared() {
            let loaded = self.store.load()?;
            add(&mut self.write(), loaded);
        }
        Ok(())
    }

    /// Reads every version of `name` from a shared store again, as
    /// [`Definitions::refresh`] reads every definition.
    ///
    /// # Errors
    ///
    /// The [`StoreError`] of [`DefinitionStore::load_named`]; the view stays
    /// as it was.
    pub fn refresh_named(&self, name: &QueryName) -> Result<(), StoreError> {
        if self.is_shared() {
            let loaded = self.store.load_named(name)?;
            add(&mut self.write(), loaded);
        }
        Ok(())
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

    /// Stores `aql` under `name`, saved at `saved`, at the version the store
    /// assigns (§12.7 stored-query-versioning).
    ///
    /// The store decides the version, so two writers never store one. This
    /// call writes to the store: a caller on an asynchronous runtime makes it
    /// on a thread that may block.
    ///
    /// # Errors
    ///
    /// The [`StoreError`] of [`DefinitionStore::insert_assigned`].
    pub fn insert_assigned(
        &self,
        name: &QueryName,
        aql: &str,
        saved: Timestamp,
    ) -> Result<Assignment, StoreError> {
        let assignment = self.store.insert_assigned(name, aql, saved)?;
        if let Assignment::Stored(definition) = &assignment {
            self.write()
                .entry(definition.name().clone())
                .or_default()
                .insert(definition.version(), Arc::new(definition.clone()));
        }
        Ok(assignment)
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

/// Adds each of `definitions` that `held` does not hold yet.
fn add(held: &mut Held, definitions: Vec<StoredDefinition>) {
    for definition in definitions {
        held.entry(definition.name().clone())
            .or_default()
            .entry(definition.version())
            .or_insert_with(|| Arc::new(definition));
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
