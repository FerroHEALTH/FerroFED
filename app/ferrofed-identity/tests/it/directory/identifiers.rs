// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identifiers a directory publishes for an organisation are kept on the
//! registry's organisation, ordered and each once, such as the URA of a
//! Dutch care provider (mCSD `Organization.identifier`; Annex B §B.2).

use std::error::Error;

use ferrofed_identity::directory;
use ferrofed_registry::id::OrganisationId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use serde_json::json;

use super::{NATIVE, ORG_A, fhir, resource};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn an_organisations_identifiers_are_kept_ordered_and_each_once() -> TestResult {
    let mut bundle = fhir();
    resource(&mut bundle, ORG_A)["identifier"] = json!([
        {"system": "https://ferrofed.eu/fhir/sid/organisation-id", "value": "org-a"},
        {"system": "http://fhir.nl/fhir/NamingSystem/ura", "value": "ura-test-0001"},
        {"system": "urn:oid:2.999.7", "value": "kvk-test-0001"},
        {"system": "http://fhir.nl/fhir/NamingSystem/ura", "value": "ura-test-0001"},
        {"value": "no-system"},
        {"system": "urn:oid:2.999.8"}
    ]);
    let registry = directory::snapshot_from_json(bundle.to_string().as_bytes())?;
    let organisation = registry
        .organisation(&OrganisationId::new("org-a")?)
        .ok_or("org-a is in the registry")?;
    let kept: Vec<(&str, &str)> = organisation
        .identifiers()
        .iter()
        .map(|identifier| (identifier.system(), identifier.value()))
        .collect();
    assert_eq!(
        vec![
            ("http://fhir.nl/fhir/NamingSystem/ura", "ura-test-0001"),
            ("urn:oid:2.999.7", "kvk-test-0001"),
        ],
        kept,
        "whole identifiers beside the registry id, ordered, the repeated URA once"
    );
    Ok(())
}

#[test]
fn a_registry_in_the_native_form_carries_no_organisation_identifiers() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(NATIVE)?;
    assert!(
        registry
            .organisations()
            .all(|organisation| organisation.identifiers().is_empty()),
        "only a directory publishes them"
    );
    Ok(())
}

#[test]
fn the_native_form_does_not_take_organisation_identifiers() {
    let written = NATIVE.replacen(
        "name = \"Hospital A\"\n",
        "name = \"Hospital A\"\n\n[[organisation.identifier]]\nsystem = \"urn:oid:2.999.7\"\nvalue = \"x\"\n",
        1,
    );
    assert!(
        RegistrySnapshot::from_toml_str(&written).is_err(),
        "the native form refuses a key it does not define"
    );
}
