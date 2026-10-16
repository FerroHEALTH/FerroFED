// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 `CodeSystem` of the connection type, held to the `CodeSystem`
//! resource as the vendored page under `docs/specs/fhir-r4/` states it, and
//! to the codes [`ConnectionType::from_code`] reads (N19, §15.2).

use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;

use ferrofed_registry::snapshot::ConnectionType;
use fhir_types::codec::{Json, Object, Path, Value};
use fhir_types::r4::code_system::CodeSystem;

type TestResult = Result<(), Box<dyn Error>>;

/// The vendored FHIR R4 page `name`.
fn page(name: &str) -> Result<String, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/fhir-r4")
        .join(name);
    Ok(std::fs::read_to_string(path)?)
}

/// The definition, as a JSON object.
fn definition() -> Result<Object, Box<dyn Error>> {
    match serde_json::from_str(ConnectionType::DEFINITION)? {
        Value::Object(object) => Ok(object),
        _ => Err("the definition is not a JSON object".into()),
    }
}

/// The string member `name` of `object`.
fn text<'a>(object: &'a Object, name: &str) -> Option<&'a str> {
    object.get(name).and_then(Value::as_str)
}

/// The codes of the definition's concepts, in order.
fn codes(object: &Object) -> Result<Vec<&str>, Box<dyn Error>> {
    object
        .get("concept")
        .and_then(Value::as_array)
        .ok_or("the definition has no concepts")?
        .iter()
        .map(|concept| {
            concept
                .get("code")
                .and_then(Value::as_str)
                .ok_or_else(|| "a concept has no code".into())
        })
        .collect()
}

/// Every connection type, by an exhaustive match, so a new one fails to build
/// until this test says whether the code system holds it.
fn every() -> [ConnectionType; 2] {
    let all = [
        ConnectionType::OpenehrRestQuery,
        ConnectionType::Hl7FhirRest,
    ];
    for kind in all {
        match kind {
            ConnectionType::OpenehrRestQuery | ConnectionType::Hl7FhirRest => {}
        }
    }
    all
}

#[test]
fn the_rules_the_test_holds_are_the_vendored_r4_text() -> TestResult {
    let page = page("codesystem.html")?;
    for rule in [
        "Within a code system definition, all the codes SHALL be unique",
        "not-present | example | fragment | complete | supplement",
        "name.matches('[A-Z]([A-Za-z0-9_]){0,254}')",
    ] {
        assert!(page.contains(rule), "CodeSystem does not say: {rule}");
    }
    Ok(())
}

#[test]
fn the_definition_is_an_r4_code_system_at_the_system_url() -> TestResult {
    let object = definition()?;
    let read = CodeSystem::from_json(&object, &mut Path::root("CodeSystem"))?;
    assert_eq!(read.to_json()?, object);
    assert_eq!(text(&object, "resourceType"), Some("CodeSystem"));
    assert_eq!(text(&object, "url"), Some(ConnectionType::SYSTEM));
    assert_eq!(
        ConnectionType::SYSTEM.rsplit('/').next(),
        text(&object, "id")
    );
    assert_eq!(text(&object, "name"), Some("ConnectionType"));
    assert_eq!(text(&object, "status"), Some("active"));
    Ok(())
}

#[test]
fn the_code_system_is_complete_unique_and_case_sensitive() -> TestResult {
    let object = definition()?;
    let codes = codes(&object)?;
    let unique: BTreeSet<_> = codes.iter().collect();
    assert_eq!(unique.len(), codes.len());
    assert_eq!(text(&object, "content"), Some("complete"));
    let count = match object.get("count") {
        Some(Value::Number(number)) => number.as_u64(),
        _ => None,
    };
    assert_eq!(count, u64::try_from(codes.len()).ok());
    assert_eq!(object.get("caseSensitive"), Some(&Value::Bool(true)));
    for code in &codes {
        assert_eq!(ConnectionType::from_code(&code.to_uppercase()), None);
    }
    Ok(())
}

#[test]
fn the_codes_are_the_ones_the_reader_takes() -> TestResult {
    let object = definition()?;
    let codes: BTreeSet<_> = codes(&object)?.into_iter().collect();
    for code in &codes {
        assert_eq!(
            ConnectionType::from_code(code).map(ConnectionType::code),
            Some(*code)
        );
    }
    let read: BTreeSet<_> = every()
        .into_iter()
        .map(ConnectionType::code)
        .filter(|code| ConnectionType::from_code(code).is_some())
        .collect();
    assert_eq!(read, codes);
    Ok(())
}
