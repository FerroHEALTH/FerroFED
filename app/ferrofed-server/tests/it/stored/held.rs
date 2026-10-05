// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A definition a store holds that the gateway never admitted: every read of
//! every backend passes the admission a `PUT` passes, so a row naming its
//! patient by a literal is refused and never served or run (§12.7, §5.4.1,
//! N33). The PostgreSQL store is held to it in the `e2e` module.

use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};

use ferrofed_registry::definition::store::{DefinitionStore, Definitions, Insertion, StoreError};
use ferrofed_registry::definition::{QueryName, StoredDefinition};
use ferrofed_server::stored::embedded::RedbStore;
use ferrofed_server::stored::{Admitted, HeldRefused, Inadmissible};
use jiff::Timestamp;
use openehr_federation::aql::{Context, Targeting};

use crate::facade::{NAMESPACE, PATIENT, PATIENT_TAIL, node_answering, received};
use crate::support::chain;

use super::{NAME, TestResult, parameterised, store_file, two_members};

/// The qualified name a definition the gateway admitted is held under.
const ADMITTED: &str = "org.example::admitted";

/// A definition naming the patient by a literal, which no `PUT` stores.
fn literal() -> String {
    parameterised().replace("$patient", &format!("'{PATIENT}'"))
}

/// The definition of `name` at 1.0.0 holding `aql`.
fn held(name: &str, aql: &str) -> Result<StoredDefinition, Box<dyn Error>> {
    Ok(StoredDefinition::new(
        QueryName::new(name)?,
        "1.0.0".parse()?,
        aql.to_owned(),
        Timestamp::UNIX_EPOCH,
    ))
}

/// A store another process writes to, its rows held where the test adds to
/// them.
#[derive(Debug, Default)]
struct Shared {
    rows: Arc<Mutex<Vec<StoredDefinition>>>,
}

impl DefinitionStore for Shared {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        self.rows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(definition.clone());
        Ok(Insertion::Stored)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        Ok(self
            .rows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    fn is_shared(&self) -> bool {
        true
    }
}

/// The [`HeldRefused`] a refused read carries.
fn refusal(read: Result<(), StoreError>) -> Result<HeldRefused, Box<dyn Error>> {
    let Err(StoreError::Corrupt(cause)) = read else {
        return Err(format!("a refused read: {read:?}").into());
    };
    Ok(*cause
        .downcast::<HeldRefused>()
        .map_err(|cause| cause.to_string())?)
}

// conformance: CP-40
#[tokio::test]
async fn a_held_definition_naming_its_patient_by_a_literal_refuses_the_start() -> TestResult {
    let dir = tempfile::tempdir()?;
    {
        let store = RedbStore::open(&store_file(dir.path()))?;
        assert_eq!(
            Insertion::Stored,
            store.insert_if_absent(&held(NAME, &literal())?)?
        );
    }
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let refused = two_members(dir.path(), &a, &b)
        .err()
        .ok_or("N33: a store holding a literal is never served")?;
    let line = chain(refused.as_ref());
    assert!(
        line.contains("names its patient by a literal"),
        "says why: {line}"
    );
    assert!(line.contains(NAME), "names the definition: {line}");
    assert!(!line.contains(PATIENT_TAIL), "§5.4.3: never quoted: {line}");
    assert!(received(&a).await?.is_empty(), "nothing reached node A");
    assert!(received(&b).await?.is_empty(), "nothing reached node B");
    Ok(())
}

// conformance: CP-40
#[test]
fn a_literal_a_shared_store_gains_after_the_start_is_never_served() -> TestResult {
    let rows = Arc::new(Mutex::new(vec![held(ADMITTED, &parameterised())?]));
    let store = Shared {
        rows: Arc::clone(&rows),
    };
    let context = Context::new(Targeting::AskAll).with_default_namespace(NAMESPACE);
    let definitions = Definitions::open(Box::new(Admitted::new(Box::new(store), context)))?;
    assert_eq!(1, definitions.len(), "the admitted definition is held");

    rows.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(held(NAME, &literal())?);
    let name = QueryName::new(NAME)?;
    let refused = refusal(definitions.refresh_named(&name))?;
    assert_eq!(name, refused.name);
    assert!(
        matches!(refused.reason, Inadmissible::SubjectLiteral { .. }),
        "N33: {refused:?}"
    );
    assert!(
        !chain(&refused).contains(PATIENT_TAIL),
        "§5.4.3: never quoted: {refused}"
    );
    refusal(definitions.refresh())?;
    assert!(
        definitions.find(&name, None).is_none(),
        "§5.4.1, N33: the literal is never held, so never run"
    );

    let admitted = QueryName::new(ADMITTED)?;
    definitions.refresh_named(&admitted)?;
    assert!(
        definitions.find(&admitted, None).is_some(),
        "another name is still served"
    );
    Ok(())
}

#[test]
fn a_held_definition_the_admission_refuses_is_refused_on_read() -> TestResult {
    let rows = Arc::new(Mutex::new(vec![held(NAME, "SELECT FROM")?]));
    let store = Shared { rows };
    let context = Context::new(Targeting::AskAll).with_default_namespace(NAMESPACE);
    let opened = Definitions::open(Box::new(Admitted::new(Box::new(store), context)));
    let refused = refusal(opened.map(drop))?;
    assert!(
        matches!(refused.reason, Inadmissible::Refused(_)),
        "§12.7: {refused:?}"
    );
    Ok(())
}
