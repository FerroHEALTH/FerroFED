// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The conformance suite every writable stored-query store passes (§12.7,
//! N44): a held name and version refused with the first definition standing,
//! the refusal holding across a reopen, the instant stored read back
//! exactly, and the lookups and listing the registry answers from. The
//! embedded `redb` store runs it here, the PostgreSQL store in the `e2e`
//! module, each scenario on a store of its own.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_registry::definition::store::{DefinitionStore, Definitions, Insertion, StoreError};
use ferrofed_registry::definition::{QueryName, StoredDefinition, VersionPattern};
use ferrofed_server::stored::embedded::RedbStore;
use jiff::Timestamp;

type TestResult = Result<(), Box<dyn Error>>;

/// Opens the one store a scenario runs on, empty the first time; a scenario
/// drops what it opened before it opens again.
pub(crate) type Open<'a> = &'a dyn Fn() -> Result<Box<dyn DefinitionStore>, StoreError>;

/// One scenario of the suite, by name.
pub(crate) type Scenario = (&'static str, fn(Open<'_>) -> TestResult);

/// Every scenario, each to run on a fresh store.
pub(crate) const SCENARIOS: [Scenario; 5] = [
    (
        "held",
        a_held_pair_is_refused_and_the_first_definition_stands,
    ),
    (
        "reopen",
        a_reopened_store_holds_what_was_stored_and_refuses_it,
    ),
    (
        "lookup",
        a_lookup_selects_the_highest_version_its_pattern_matches,
    ),
    (
        "list",
        a_list_names_every_version_of_every_name_the_prefix_starts,
    ),
    (
        "named",
        a_named_load_returns_the_versions_of_that_name_alone,
    ),
];

/// The definition of `name` at `version` holding `aql`, stored at an instant
/// with nanoseconds, which a store reads back exactly.
pub(crate) fn definition(
    name: &str,
    version: &str,
    aql: &str,
) -> Result<StoredDefinition, Box<dyn Error>> {
    Ok(StoredDefinition::new(
        name.parse()?,
        version.parse()?,
        aql.to_owned(),
        Timestamp::new(1_790_000_000, 123_456_789)?,
    ))
}

fn a_held_pair_is_refused_and_the_first_definition_stands(open: Open<'_>) -> TestResult {
    let first = definition("org.example::q", "1.0.0", "SELECT 1")?;
    {
        let definitions = Definitions::open(open()?)?;
        assert_eq!(
            Insertion::Stored,
            definitions.insert(first.clone())?,
            "a new pair is stored"
        );
        let second = definition("org.example::q", "1.0.0", "SELECT 2")?;
        assert_eq!(Insertion::Held, definitions.insert(second)?, "§12.7, N44");
        let found = definitions
            .find(&"org.example::q".parse()?, None)
            .ok_or("held")?;
        assert_eq!(&first, found.as_ref(), "the stored version is immutable");
    }
    assert_eq!(
        vec![first],
        open()?.load()?,
        "the store holds the first only"
    );
    Ok(())
}

fn a_reopened_store_holds_what_was_stored_and_refuses_it(open: Open<'_>) -> TestResult {
    let stored = definition("org.example::q", "1.0.0", "SELECT 1")?;
    assert_eq!(
        Insertion::Stored,
        open()?.insert_if_absent(&stored)?,
        "a new pair is stored"
    );
    let reopened = Definitions::open(open()?)?;
    assert_eq!(1, reopened.len(), "the reopened store holds one version");
    let found = reopened
        .find(&"org.example::q".parse()?, None)
        .ok_or("held")?;
    assert_eq!(&stored, found.as_ref(), "name, version, AQL and instant");
    assert_eq!(
        Insertion::Held,
        reopened.insert(definition("org.example::q", "1.0.0", "SELECT 3")?)?,
        "§12.7, N44: the refusal holds across a restart"
    );
    Ok(())
}

fn a_lookup_selects_the_highest_version_its_pattern_matches(open: Open<'_>) -> TestResult {
    let definitions = Definitions::open(open()?)?;
    for version in ["1.0.0", "1.9.3", "1.10.0", "2.0.0", "1.10.2"] {
        let inserted = definitions.insert(definition("q", version, version)?)?;
        assert_eq!(Insertion::Stored, inserted, "every pair is new");
    }
    drop(definitions);
    let definitions = Definitions::open(open()?)?;
    let name: QueryName = "q".parse()?;
    let at = |pattern: Option<&str>| -> Result<Option<String>, Box<dyn Error>> {
        let pattern = pattern.map(str::parse::<VersionPattern>).transpose()?;
        Ok(definitions
            .find(&name, pattern.as_ref())
            .map(|found| found.version().to_string()))
    };
    assert_eq!(Some("2.0.0".to_owned()), at(None)?, "the latest");
    assert_eq!(Some("1.10.2".to_owned()), at(Some("1"))?, "by number");
    assert_eq!(Some("1.9.3".to_owned()), at(Some("1.9"))?, "a minor prefix");
    assert_eq!(Some("1.10.0".to_owned()), at(Some("1.10.0"))?, "exact");
    assert_eq!(None, at(Some("3"))?, "no version matches");
    Ok(())
}

fn a_list_names_every_version_of_every_name_the_prefix_starts(open: Open<'_>) -> TestResult {
    let definitions = Definitions::open(open()?)?;
    for (name, version) in [
        ("org.openehr::b", "1.0.0"),
        ("org.openehr::a", "2.0.0"),
        ("org.openehr::a", "1.0.0"),
        ("com.example::c", "1.0.0"),
    ] {
        let inserted = definitions.insert(definition(name, version, "SELECT 1")?)?;
        assert_eq!(Insertion::Stored, inserted, "every pair is new");
    }
    drop(definitions);
    let definitions = Definitions::open(open()?)?;
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
        listed,
        "by name, then by version"
    );
    assert!(definitions.list("none").is_empty(), "no name starts so");
    Ok(())
}

fn a_named_load_returns_the_versions_of_that_name_alone(open: Open<'_>) -> TestResult {
    let store = open()?;
    for (name, version) in [("a", "1.0.0"), ("b", "1.0.0"), ("a", "2.0.0")] {
        let inserted = store.insert_if_absent(&definition(name, version, "SELECT 1")?)?;
        assert_eq!(Insertion::Stored, inserted, "every pair is new");
    }
    let mut versions: Vec<String> = store
        .load_named(&"a".parse()?)?
        .iter()
        .map(|found| found.version().to_string())
        .collect();
    versions.sort();
    assert_eq!(vec!["1.0.0", "2.0.0"], versions, "the versions of a alone");
    assert!(
        store.load_named(&"c".parse()?)?.is_empty(),
        "no version of c is held"
    );
    Ok(())
}

#[test]
fn the_embedded_store_passes_the_suite() -> TestResult {
    for (scenario, run) in SCENARIOS {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("definitions.redb");
        let open = || -> Result<Box<dyn DefinitionStore>, StoreError> {
            Ok(Box::new(RedbStore::open(&path)?))
        };
        run(&open).map_err(|failed| format!("{scenario}: {failed}"))?;
    }
    Ok(())
}

#[test]
fn the_embedded_store_is_neither_shared_nor_read_only() -> TestResult {
    let dir = tempfile::tempdir()?;
    let store = RedbStore::open(&dir.path().join("definitions.redb"))?;
    assert!(!store.is_shared(), "one process opens the file");
    assert!(!store.is_read_only());
    Ok(())
}
