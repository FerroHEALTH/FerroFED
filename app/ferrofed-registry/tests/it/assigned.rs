// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A definition stored with no version: the registry assigns `1.0.0` to a
//! new name and the next patch of the newest version otherwise, by number,
//! and never replaces a held version, so two writers racing on one name
//! store two versions or one of them is refused (§12.7
//! stored-query-versioning, N44; ITS-REST Definition API,
//! `definition_query_store`).

use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};

use ferrofed_registry::definition::store::{
    ASSIGNMENT_ATTEMPTS, Assignment, DefinitionStore, Definitions, Insertion, StoreError,
};
use ferrofed_registry::definition::{QueryName, QueryVersion, StoredDefinition};
use jiff::Timestamp;

type TestResult = Result<(), Box<dyn Error>>;

/// A store in memory whose insert refuses a held name and version.
#[derive(Debug, Clone, Default)]
struct Memory(Arc<Mutex<Vec<StoredDefinition>>>);

impl Memory {
    /// Every row, in insertion order.
    fn rows(&self) -> Vec<StoredDefinition> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl DefinitionStore for Memory {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        let mut rows = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if rows
            .iter()
            .any(|row| row.name() == definition.name() && row.version() == definition.version())
        {
            return Ok(Insertion::Held);
        }
        rows.push(definition.clone());
        Ok(Insertion::Stored)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        Ok(self.rows())
    }
}

/// A store where a rival writer stores each version just before this one
/// inserts it, `rivals` times, and then stands aside.
#[derive(Debug)]
struct Raced {
    memory: Memory,
    rivals: Mutex<usize>,
}

impl DefinitionStore for Raced {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        let mut rivals = self.rivals.lock().unwrap_or_else(PoisonError::into_inner);
        if *rivals > 0 {
            *rivals -= 1;
            let rival = StoredDefinition::new(
                definition.name().clone(),
                definition.version(),
                "SELECT 'rival'".to_owned(),
                Timestamp::UNIX_EPOCH,
            );
            assert_eq!(
                Insertion::Stored,
                self.memory.insert_if_absent(&rival)?,
                "the rival stores first"
            );
        }
        self.memory.insert_if_absent(definition)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        self.memory.load()
    }
}

/// A store that refuses every insert.
#[derive(Debug, Default)]
struct ReadOnly;

impl DefinitionStore for ReadOnly {
    fn insert_if_absent(&self, _definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        Err(StoreError::ReadOnly)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        Ok(Vec::new())
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

/// `memory` holding `name` at each of `versions`.
fn holding(memory: &Memory, name: &str, versions: &[&str]) -> TestResult {
    for version in versions {
        let held = StoredDefinition::new(
            name.parse()?,
            version.parse()?,
            format!("SELECT '{version}'"),
            Timestamp::UNIX_EPOCH,
        );
        assert_eq!(
            Insertion::Stored,
            memory.insert_if_absent(&held)?,
            "{version} is new"
        );
    }
    Ok(())
}

/// The version an assignment stored at, or the assignment itself.
fn stored_at(assignment: Assignment) -> Result<String, String> {
    match assignment {
        Assignment::Stored(definition) => Ok(definition.version().to_string()),
        other => Err(format!("stored: {other:?}")),
    }
}

#[test]
fn a_new_name_is_assigned_one_zero_zero_and_a_held_one_the_next_patch_by_number() -> TestResult {
    let newest = |versions: &[&str]| -> Result<Option<QueryVersion>, Box<dyn Error>> {
        let mut parsed = Vec::new();
        for version in versions {
            parsed.push(version.parse::<QueryVersion>()?);
        }
        Ok(parsed.into_iter().max())
    };
    for (held, assigned) in [
        (&[][..], "1.0.0"),
        (&["1.0.3"][..], "1.0.4"),
        (&["1.0.3", "1.2.0"][..], "1.2.1"),
        (&["1.10.0", "1.9.0"][..], "1.10.1"),
        (&["2.0.0", "1.99.99"][..], "2.0.1"),
    ] {
        assert_eq!(
            Some(assigned.parse()?),
            QueryVersion::assigned_after(newest(held)?),
            "§12.7: after {held:?}"
        );
    }
    assert_eq!(
        None,
        QueryVersion::assigned_after(Some(QueryVersion::new(1, 0, u64::MAX))),
        "no patch past the largest"
    );
    Ok(())
}

#[test]
fn an_unversioned_insert_stores_at_the_next_version_and_never_replaces_one() -> TestResult {
    for (held, assigned) in [
        (&[][..], "1.0.0"),
        (&["1.0.3"][..], "1.0.4"),
        (&["1.0.3", "1.2.0"][..], "1.2.1"),
        (&["1.9.0", "1.10.0"][..], "1.10.1"),
    ] {
        let memory = Memory::default();
        holding(&memory, "ns::q", held)?;
        holding(&memory, "ns::other", &["7.0.0"])?;
        let before = memory.rows();
        let name: QueryName = "ns::q".parse()?;
        let stored = memory.insert_assigned(&name, "SELECT 'new'", Timestamp::UNIX_EPOCH)?;
        assert_eq!(Ok(assigned.to_owned()), stored_at(stored), "after {held:?}");
        let rows = memory.rows();
        assert_eq!(before.len() + 1, rows.len(), "one row added");
        assert_eq!(
            before.as_slice(),
            rows.get(..before.len()).ok_or("the held rows")?,
            "§12.7, N44: every held version stands"
        );
    }
    Ok(())
}

#[test]
fn a_version_a_rival_stores_first_is_never_replaced_and_the_next_is_assigned() -> TestResult {
    let store = Raced {
        memory: Memory::default(),
        rivals: Mutex::new(2),
    };
    let name: QueryName = "ns::q".parse()?;
    let stored = store.insert_assigned(&name, "SELECT 'mine'", Timestamp::UNIX_EPOCH)?;
    assert_eq!(Ok("1.0.2".to_owned()), stored_at(stored));
    let rows: Vec<(String, String)> = store
        .memory
        .rows()
        .iter()
        .map(|row| (row.version().to_string(), row.aql().to_owned()))
        .collect();
    assert_eq!(
        vec![
            ("1.0.0".to_owned(), "SELECT 'rival'".to_owned()),
            ("1.0.1".to_owned(), "SELECT 'rival'".to_owned()),
            ("1.0.2".to_owned(), "SELECT 'mine'".to_owned()),
        ],
        rows,
        "the rival's versions stand, and this writer's is a version of its own"
    );
    Ok(())
}

#[test]
fn a_writer_that_loses_every_attempt_is_refused_and_stores_nothing() -> TestResult {
    let store = Raced {
        memory: Memory::default(),
        rivals: Mutex::new(ASSIGNMENT_ATTEMPTS),
    };
    let name: QueryName = "ns::q".parse()?;
    let assignment = store.insert_assigned(&name, "SELECT 'mine'", Timestamp::UNIX_EPOCH)?;
    assert_eq!(
        Assignment::Contended,
        assignment,
        "a refusal, never a winner"
    );
    let rows = store.memory.rows();
    assert_eq!(ASSIGNMENT_ATTEMPTS, rows.len(), "the rival's versions only");
    assert!(
        rows.iter().all(|row| row.aql() == "SELECT 'rival'"),
        "nothing of this writer is stored"
    );
    Ok(())
}

#[test]
fn writers_racing_on_one_name_store_distinct_versions_or_are_refused() -> TestResult {
    let memory = Memory::default();
    let name: QueryName = "ns::raced".parse()?;
    let writers = 16;
    let assignments: Vec<Result<Assignment, StoreError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..writers)
            .map(|writer| {
                let (memory, name) = (&memory, &name);
                scope.spawn(move || {
                    let aql = format!("SELECT {writer}");
                    memory.insert_assigned(name, &aql, Timestamp::UNIX_EPOCH)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or(Err(StoreError::ReadOnly)))
            .collect()
    });
    let mut versions = Vec::new();
    for assignment in assignments {
        match assignment? {
            Assignment::Stored(definition) => versions.push(definition.version()),
            Assignment::Contended => {}
            other @ Assignment::Exhausted => panic!("stored or contended: {other:?}"),
        }
    }
    let stored = versions.len();
    versions.sort();
    versions.dedup();
    assert_eq!(stored, versions.len(), "§12.7: no version is stored twice");
    assert_eq!(stored, memory.rows().len(), "every row is a writer's own");
    assert!(stored >= 1, "at least one writer stores");
    Ok(())
}

#[test]
fn the_newest_patch_at_its_largest_assigns_nothing() -> TestResult {
    let memory = Memory::default();
    let largest = format!("1.0.{}", u64::MAX);
    holding(&memory, "q", &[largest.as_str()])?;
    let assignment = memory.insert_assigned(&"q".parse()?, "SELECT 1", Timestamp::UNIX_EPOCH)?;
    assert_eq!(Assignment::Exhausted, assignment);
    assert_eq!(1, memory.rows().len(), "nothing is stored");
    Ok(())
}

#[test]
fn a_read_only_store_refuses_an_unversioned_insert() -> TestResult {
    let view = Definitions::open(Box::new(ReadOnly))?;
    let refused = view.insert_assigned(&"q".parse()?, "SELECT 1", Timestamp::UNIX_EPOCH);
    assert!(matches!(refused, Err(StoreError::ReadOnly)), "{refused:?}");
    assert!(view.is_empty(), "nothing is added");
    Ok(())
}

#[test]
fn the_view_serves_the_assigned_version_at_once() -> TestResult {
    let memory = Memory::default();
    holding(&memory, "q", &["1.9.0", "1.10.0"])?;
    let view = Definitions::open(Box::new(memory))?;
    let name: QueryName = "q".parse()?;
    let stored = view.insert_assigned(&name, "SELECT 'new'", Timestamp::UNIX_EPOCH)?;
    assert_eq!(Ok("1.10.1".to_owned()), stored_at(stored));
    let found = view.find(&name, None).ok_or("held")?;
    assert_eq!("1.10.1", found.version().to_string(), "the newest");
    assert_eq!("SELECT 'new'", found.aql());
    Ok(())
}
