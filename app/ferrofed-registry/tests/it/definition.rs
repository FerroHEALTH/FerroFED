// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query definitions: the ITS-REST qualified name and semver
//! version, the version lookup, and the in-memory view over a store whose
//! insert refuses a held name and version (§12.7, N44).

use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};

use ferrofed_registry::definition::store::{DefinitionStore, Definitions, Insertion, StoreError};
use ferrofed_registry::definition::{
    QueryName, QueryNameError, QueryVersion, QueryVersionError, StoredDefinition, VersionPattern,
};
use jiff::Timestamp;

type TestResult = Result<(), Box<dyn Error>>;

/// A store in memory, shared with the test that reads it back.
#[derive(Debug, Clone, Default)]
struct Memory(Arc<Mutex<Vec<StoredDefinition>>>);

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
        Ok(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }
}

fn definition(name: &str, version: &str, aql: &str) -> Result<StoredDefinition, Box<dyn Error>> {
    Ok(StoredDefinition::new(
        name.parse()?,
        version.parse()?,
        aql.to_owned(),
        Timestamp::UNIX_EPOCH,
    ))
}

#[test]
fn a_qualified_name_takes_an_optional_namespace() -> TestResult {
    for valid in [
        "org.openehr::my_compositions",
        "my_compositions",
        "ehr::all_influenza_vacc_candidates",
        "a-b.c_9",
    ] {
        assert_eq!(valid, QueryName::new(valid)?.as_str(), "ITS-REST examples");
    }
    assert_eq!(
        Some("org.openehr"),
        QueryName::new("org.openehr::q")?.namespace()
    );
    assert_eq!(None, QueryName::new("q")?.namespace());
    Ok(())
}

#[test]
fn a_name_outside_the_its_rest_form_is_refused() {
    for (refused, error) in [
        ("", QueryNameError::Empty),
        ("::q", QueryNameError::Empty),
        ("ns::", QueryNameError::Empty),
        ("a b", QueryNameError::Malformed),
        ("a/b", QueryNameError::Malformed),
        ("a::b::c", QueryNameError::Malformed),
        ("caf\u{e9}", QueryNameError::Malformed),
        ("aql", QueryNameError::Reserved),
        ("AQL", QueryNameError::Reserved),
        ("org.openehr::Aql", QueryNameError::Reserved),
    ] {
        assert_eq!(Err(error), QueryName::new(refused), "{refused:?}");
    }
}

#[test]
fn a_version_is_major_minor_patch_with_one_spelling() -> TestResult {
    assert_eq!(QueryVersion::new(1, 10, 0), "1.10.0".parse()?);
    assert_eq!("0.0.0", QueryVersion::new(0, 0, 0).to_string());
    for refused in [
        "1",
        "1.0",
        "1.0.0.0",
        "01.0.0",
        "1.00.0",
        "1.0.0-rc.1",
        "1.0.0+b",
        "v1.0.0",
        "1..0",
        "",
        "1.0.99999999999999999999",
    ] {
        assert_eq!(
            Err(QueryVersionError::Malformed),
            refused.parse::<QueryVersion>(),
            "{refused:?}"
        );
    }
    Ok(())
}

#[test]
fn versions_order_by_number_and_not_by_text() -> TestResult {
    let older: QueryVersion = "1.9.3".parse()?;
    let newer: QueryVersion = "1.10.0".parse()?;
    assert!(older < newer);
    Ok(())
}

#[test]
fn a_pattern_is_exact_or_a_major_or_major_minor_prefix() -> TestResult {
    let version: QueryVersion = "1.7.1".parse()?;
    for (pattern, matches) in [
        ("1", true),
        ("1.7", true),
        ("1.7.1", true),
        ("1.6", false),
        ("2", false),
        ("1.7.0", false),
    ] {
        assert_eq!(
            matches,
            pattern.parse::<VersionPattern>()?.matches(&version),
            "{pattern}"
        );
    }
    for refused in ["", "1.", "x", "1.0.0.0", "01"] {
        assert_eq!(
            Err(QueryVersionError::Pattern),
            refused.parse::<VersionPattern>(),
            "{refused:?}"
        );
    }
    Ok(())
}

#[test]
fn a_held_name_and_version_is_refused_and_stands_unchanged() -> TestResult {
    let memory = Memory::default();
    let definitions = Definitions::open(Box::new(memory.clone()))?;
    let first = definition("ns::q", "1.0.0", "SELECT 1")?;
    assert_eq!(Insertion::Stored, definitions.insert(first.clone())?);
    let second = definition("ns::q", "1.0.0", "SELECT 2")?;
    assert_eq!(Insertion::Held, definitions.insert(second)?, "§12.7, N44");
    let name: QueryName = "ns::q".parse()?;
    let found = definitions.find(&name, None).ok_or("held")?;
    assert_eq!("SELECT 1", found.aql(), "the stored version is immutable");
    assert_eq!(
        vec![first],
        memory.load()?,
        "the store holds the first only"
    );
    Ok(())
}

#[test]
fn a_lookup_selects_the_highest_version_its_pattern_matches() -> TestResult {
    let definitions = Definitions::open(Box::new(Memory::default()))?;
    for version in ["1.0.0", "1.9.3", "1.10.0", "2.0.0", "1.10.2"] {
        let inserted = definitions.insert(definition("q", version, version)?)?;
        assert_eq!(Insertion::Stored, inserted);
    }
    let name: QueryName = "q".parse()?;
    let at = |pattern: Option<&str>| -> Result<Option<String>, Box<dyn Error>> {
        let pattern = pattern.map(str::parse::<VersionPattern>).transpose()?;
        Ok(definitions
            .find(&name, pattern.as_ref())
            .map(|found| found.version().to_string()))
    };
    assert_eq!(
        Some("2.0.0".to_owned()),
        at(None)?,
        "no version is the latest"
    );
    assert_eq!(Some("1.10.2".to_owned()), at(Some("1"))?);
    assert_eq!(Some("1.9.3".to_owned()), at(Some("1.9"))?);
    assert_eq!(Some("1.10.0".to_owned()), at(Some("1.10.0"))?);
    assert_eq!(None, at(Some("3"))?);
    assert_eq!(None, definitions.find(&"other".parse()?, None));
    Ok(())
}

#[test]
fn the_view_opens_with_what_the_store_holds() -> TestResult {
    let memory = Memory::default();
    let stored = definition("ns::q", "1.0.0", "SELECT 1")?;
    assert_eq!(Insertion::Stored, memory.insert_if_absent(&stored)?);
    let reopened = Definitions::open(Box::new(memory))?;
    assert_eq!(1, reopened.len());
    let found = reopened.find(&"ns::q".parse()?, None).ok_or("held")?;
    assert_eq!(&stored, found.as_ref());
    assert_eq!(
        Insertion::Held,
        reopened.insert(definition("ns::q", "1.0.0", "SELECT 2")?)?,
        "N44: a refusal holds after a reopen"
    );
    Ok(())
}

#[test]
fn a_list_names_every_version_of_every_name_the_pattern_starts() -> TestResult {
    let definitions = Definitions::open(Box::new(Memory::default()))?;
    for (name, version) in [
        ("org.openehr::b", "1.0.0"),
        ("org.openehr::a", "2.0.0"),
        ("org.openehr::a", "1.0.0"),
        ("com.example::c", "1.0.0"),
    ] {
        let inserted = definitions.insert(definition(name, version, "SELECT 1")?)?;
        assert_eq!(Insertion::Stored, inserted);
    }
    let listed: Vec<String> = definitions
        .list("org.openehr")
        .iter()
        .map(|found| format!("{}/{}", found.name(), found.version()))
        .collect();
    assert_eq!(
        vec![
            "org.openehr::a/1.0.0",
            "org.openehr::a/2.0.0",
            "org.openehr::b/1.0.0"
        ],
        listed
    );
    assert!(definitions.list("none").is_empty());
    Ok(())
}

/// [`Memory`] as several processes would share it.
#[derive(Debug, Clone, Default)]
struct Shared(Memory);

impl DefinitionStore for Shared {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        self.0.insert_if_absent(definition)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        self.0.load()
    }

    fn is_shared(&self) -> bool {
        true
    }
}

/// A store that holds what it opened with and refuses every insert.
#[derive(Debug, Clone, Default)]
struct ReadOnly(Vec<StoredDefinition>);

impl DefinitionStore for ReadOnly {
    fn insert_if_absent(&self, _definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        Err(StoreError::ReadOnly)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        Ok(self.0.clone())
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

#[test]
fn a_view_over_a_shared_store_learns_another_writers_versions_on_a_refresh() -> TestResult {
    let shared = Shared::default();
    let here = Definitions::open(Box::new(shared.clone()))?;
    let there = Definitions::open(Box::new(shared))?;
    assert!(here.is_shared());
    let name: QueryName = "ns::q".parse()?;
    assert_eq!(
        Insertion::Stored,
        there.insert(definition("ns::q", "1.1.0", "SELECT 2")?)?
    );
    assert_eq!(None, here.find(&name, None), "not read yet");
    here.refresh_named(&name)?;
    let found = here.find(&name, None).ok_or("learned")?;
    assert_eq!("SELECT 2", found.aql());
    assert_eq!(
        Insertion::Held,
        here.insert(definition("ns::q", "1.1.0", "SELECT 3")?)?,
        "§12.7, N44: the store refuses a version another writer holds"
    );
    assert_eq!(
        Insertion::Stored,
        there.insert(definition("other", "1.0.0", "SELECT 4")?)?
    );
    here.refresh()?;
    assert_eq!(2, here.len(), "a whole refresh learns every name");
    Ok(())
}

#[test]
fn a_refresh_never_replaces_a_held_version() -> TestResult {
    let shared = Shared::default();
    let view = Definitions::open(Box::new(shared.clone()))?;
    let held = definition("q", "1.0.0", "SELECT 1")?;
    assert_eq!(Insertion::Stored, view.insert(held.clone())?);
    let altered = definition("q", "1.0.0", "SELECT 9")?;
    *shared
        .0
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .first_mut()
        .ok_or("one row")? = altered;
    view.refresh()?;
    let found = view.find(&"q".parse()?, None).ok_or("held")?;
    assert_eq!(
        &held,
        found.as_ref(),
        "§12.7, N44: a held version is immutable"
    );
    Ok(())
}

#[test]
fn a_store_that_is_not_shared_is_not_read_again() -> TestResult {
    let memory = Memory::default();
    let view = Definitions::open(Box::new(memory.clone()))?;
    assert!(!view.is_shared());
    assert_eq!(
        Insertion::Stored,
        memory.insert_if_absent(&definition("q", "1.0.0", "SELECT 1")?)?
    );
    view.refresh()?;
    view.refresh_named(&"q".parse()?)?;
    assert!(view.is_empty(), "one process holds the store");
    Ok(())
}

#[test]
fn a_read_only_store_serves_what_it_holds_and_refuses_an_insert() -> TestResult {
    let held = definition("q", "1.0.0", "SELECT 1")?;
    let view = Definitions::open(Box::new(ReadOnly(vec![held.clone()])))?;
    assert!(view.is_read_only());
    let refused = view.insert(definition("q", "2.0.0", "SELECT 2")?);
    assert!(matches!(refused, Err(StoreError::ReadOnly)), "{refused:?}");
    assert_eq!(1, view.len(), "nothing is added");
    let found = view.find(&"q".parse()?, None).ok_or("held")?;
    assert_eq!(&held, found.as_ref());
    Ok(())
}

#[test]
fn a_named_load_keeps_the_versions_of_that_name_alone() -> TestResult {
    let memory = Memory::default();
    for (name, version) in [("a", "1.0.0"), ("b", "1.0.0"), ("a", "2.0.0")] {
        let inserted = memory.insert_if_absent(&definition(name, version, "SELECT 1")?)?;
        assert_eq!(Insertion::Stored, inserted);
    }
    let versions: Vec<String> = memory
        .load_named(&"a".parse()?)?
        .iter()
        .map(|found| found.version().to_string())
        .collect();
    assert_eq!(vec!["1.0.0", "2.0.0"], versions);
    Ok(())
}
