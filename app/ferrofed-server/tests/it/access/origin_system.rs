// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 `CodeSystem` of the default origin system of the access log's
//! security labels, held to the `CodeSystem` resource as the vendored page
//! under `docs/specs/fhir-r4/` states it, and to the default
//! `[access_log.labels] origin_system`.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::path::PathBuf;

use ferrofed_server::access::config::{Labels, ORIGIN_SYSTEM_DEFINITION};
use fhir_types::codec::{Json, Object, Path, Value};
use fhir_types::r4::code_system::CodeSystem;

type TestResult = Result<(), Box<dyn Error>>;

/// The definition, as a JSON object.
fn definition() -> Result<Object, Box<dyn Error>> {
    match serde_json::from_str(ORIGIN_SYSTEM_DEFINITION)? {
        Value::Object(object) => Ok(object),
        _ => Err("the definition is not a JSON object".into()),
    }
}

/// The string member `name` of `object`.
fn text<'a>(object: &'a Object, name: &str) -> Option<&'a str> {
    object.get(name).and_then(Value::as_str)
}

#[test]
fn the_rules_the_test_holds_are_the_vendored_r4_text() -> TestResult {
    let page = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/fhir-r4/codesystem.html"),
    )?;
    for rule in [
        "not-present | example | fragment | complete | supplement",
        "name.matches('[A-Z]([A-Za-z0-9_]){0,254}')",
    ] {
        assert!(page.contains(rule), "CodeSystem does not say: {rule}");
    }
    Ok(())
}

#[test]
fn the_definition_is_an_r4_code_system_at_the_default_origin_system() -> TestResult {
    let object = definition()?;
    let read = CodeSystem::from_json(&object, &mut Path::root("CodeSystem"))?;
    assert_eq!(read.to_json()?, object);
    assert_eq!(text(&object, "resourceType"), Some("CodeSystem"));
    let system = Labels::default().origin_system;
    assert_eq!(text(&object, "url"), Some(system.as_str()));
    assert_eq!(system.rsplit('/').next(), text(&object, "id"));
    assert_eq!(text(&object, "name"), Some("AccessLogOrigin"));
    assert_eq!(text(&object, "status"), Some("active"));
    Ok(())
}

#[test]
fn the_codes_are_the_deployments_endpoint_ids_so_none_is_listed() -> TestResult {
    let object = definition()?;
    assert_eq!(text(&object, "content"), Some("not-present"));
    assert_eq!(object.get("concept"), None);
    assert_eq!(object.get("count"), None);
    assert_eq!(object.get("caseSensitive"), Some(&Value::Bool(true)));
    Ok(())
}
