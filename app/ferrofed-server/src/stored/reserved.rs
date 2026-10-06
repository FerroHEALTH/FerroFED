// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's own stored queries, held read-only beside a deployment's
//! store under the reserved namespace.
//!
//! [`Reserved`] wraps whatever backend the deployment configures, embedded,
//! PostgreSQL or files, and adds the patient summary's section queries to
//! every read, at their one immutable version (§12.7, N44). A store can hold a
//! row no `PUT` wrote, from a restore, a manual insert or a definition file,
//! so a read that finds one in the reserved namespace is refused: a
//! deployment can never shadow or add to the gateway's own queries. An insert
//! there is refused as an insert into a read-only store. No specification
//! governs the reservation: our own design.

use std::fmt;

use ferrofed_eehrxf::reserved;
use ferrofed_registry::definition::store::{DefinitionStore, Insertion, StoreError};
use ferrofed_registry::definition::{QueryName, QueryVersion, StoredDefinition};

use crate::facade::security;

/// A definition a store holds in the reserved namespace, named by its
/// qualified name and version, its text never quoted.
#[derive(Debug, thiserror::Error)]
#[error(
    "the stored-query store holds {name} at {version}, in the namespace {} the gateway reserves for its own read-only queries; remove it from the store",
    reserved::NAMESPACE
)]
pub struct Shadowing {
    /// The qualified name the store holds the definition under.
    pub name: QueryName,
    /// The version it holds it at.
    pub version: QueryVersion,
}

/// A backend with the gateway's own read-only definitions added to every
/// read, and the reserved namespace closed to the backend.
pub struct Reserved {
    store: Box<dyn DefinitionStore>,
    held: Vec<StoredDefinition>,
}

impl Reserved {
    /// Wraps `store`, adding the patient summary's section queries.
    ///
    /// # Errors
    ///
    /// [`StoreError::Corrupt`] when a section query has no qualified name,
    /// which no section meets.
    pub fn new(store: Box<dyn DefinitionStore>) -> Result<Self, StoreError> {
        let held = ferrofed_eehrxf::patient_summary::definitions()
            .map_err(|refused| StoreError::Corrupt(Box::new(refused)))?;
        Ok(Self { store, held })
    }

    /// `loaded`, when none of it sits in the reserved namespace.
    fn unshadowed(loaded: Vec<StoredDefinition>) -> Result<Vec<StoredDefinition>, StoreError> {
        match loaded
            .iter()
            .find(|definition| reserved::reserves(definition.name()))
        {
            Some(shadowing) => {
                security::held_definition_refused(shadowing.name(), shadowing.version());
                Err(StoreError::Corrupt(Box::new(Shadowing {
                    name: shadowing.name().clone(),
                    version: shadowing.version(),
                })))
            }
            None => Ok(loaded),
        }
    }
}

impl DefinitionStore for Reserved {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        if reserved::reserves(definition.name()) {
            return Err(StoreError::ReadOnly);
        }
        self.store.insert_if_absent(definition)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        let mut loaded = Self::unshadowed(self.store.load()?)?;
        loaded.extend(self.held.iter().cloned());
        Ok(loaded)
    }

    fn load_named(&self, name: &QueryName) -> Result<Vec<StoredDefinition>, StoreError> {
        let mut loaded = Self::unshadowed(self.store.load_named(name)?)?;
        loaded.extend(
            self.held
                .iter()
                .filter(|definition| definition.name() == name)
                .cloned(),
        );
        Ok(loaded)
    }

    fn is_shared(&self) -> bool {
        self.store.is_shared()
    }

    fn is_read_only(&self) -> bool {
        self.store.is_read_only()
    }
}

impl fmt::Debug for Reserved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reserved")
            .field("store", &self.store)
            .field("held", &self.held.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use ferrofed_eehrxf::patient_summary::Section;
    use ferrofed_eehrxf::reserved::{SAVED, VERSION};
    use ferrofed_registry::definition::store::{DefinitionStore, Insertion, StoreError};
    use ferrofed_registry::definition::{QueryName, QueryVersion, StoredDefinition};

    use super::{Reserved, Shadowing};

    /// A store that holds what it is given.
    #[derive(Debug, Default)]
    struct Memory(Mutex<Vec<StoredDefinition>>);

    impl DefinitionStore for Memory {
        fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
            self.0.lock().unwrap().push(definition.clone());
            Ok(Insertion::Stored)
        }

        fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    fn held(name: &str) -> StoredDefinition {
        StoredDefinition::new(
            QueryName::new(name).unwrap(),
            QueryVersion::new(1, 0, 0),
            "SELECT c FROM EHR e CONTAINS COMPOSITION c".to_owned(),
            SAVED,
        )
    }

    #[test]
    fn every_read_adds_the_section_queries_at_their_one_version() {
        let store = Memory::default();
        store.0.lock().unwrap().push(held("org.example::mine"));
        let reserved = Reserved::new(Box::new(store)).unwrap();
        let loaded = reserved.load().unwrap();
        assert_eq!(Section::ALL.len() + 1, loaded.len());
        let problems = Section::Problems.name().unwrap();
        let named = reserved.load_named(&problems).unwrap();
        assert_eq!(1, named.len());
        assert_eq!(VERSION, named[0].version());
        assert_eq!(Section::Problems.aql(), named[0].aql());
    }

    #[test]
    fn a_store_row_in_the_reserved_namespace_refuses_the_read() {
        for name in [
            "eu.ferrofed.eehrxf::patient-summary-problems",
            "eu.ferrofed.eehrxf::added",
            "EU.FERROFED.EEHRXF::patient-summary-problems",
        ] {
            let store = Memory::default();
            store.0.lock().unwrap().push(held(name));
            let reserved = Reserved::new(Box::new(store)).unwrap();
            let refused = reserved.load().unwrap_err();
            let StoreError::Corrupt(source) = &refused else {
                panic!("{name}: {refused:?}");
            };
            let shadowing = source.downcast_ref::<Shadowing>().unwrap();
            assert_eq!(name, shadowing.name.as_str());
            let named = QueryName::new(name).unwrap();
            assert!(reserved.load_named(&named).is_err(), "{name}");
        }
    }

    #[test]
    fn an_insert_into_the_reserved_namespace_is_refused_and_reaches_no_backend() {
        let reserved = Reserved::new(Box::new(Memory::default())).unwrap();
        let refused = reserved
            .insert_if_absent(&held("eu.ferrofed.eehrxf::patient-summary-problems"))
            .unwrap_err();
        assert!(matches!(refused, StoreError::ReadOnly), "{refused:?}");
        assert_eq!(Section::ALL.len(), reserved.load().unwrap().len());
        assert_eq!(
            Insertion::Stored,
            reserved
                .insert_if_absent(&held("org.example::mine"))
                .unwrap()
        );
    }
}
