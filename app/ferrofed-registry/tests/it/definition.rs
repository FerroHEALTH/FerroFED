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
