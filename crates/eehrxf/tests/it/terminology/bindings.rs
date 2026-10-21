// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `required` bindings of a profile held against the expansions a
//! package carries: a concept in the expansion holds, one outside it is a
//! finding, and a binding with no expansion is named as not evaluated until
//! a terminology server's `$validate-code` answers it
//! (<https://hl7.org/fhir/R4/terminologies.html#required>).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use eehrxf::dataset::ResourceProfile;
use eehrxf::receive::ReceivedDocument;
use eehrxf::terminology::Terminology;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::bindings::BindingCheck;
use eehrxf::terminology::bindings::BindingGap;
use eehrxf::terminology::bindings::UnevaluatedBinding;
use eehrxf::terminology::bindings::check_required;
use eehrxf::terminology::expansion::Expansions;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use http::StatusCode;

use super::LOCAL;
use super::Script;
use super::Scripted;
use crate::receive::BUNDLE_PROFILE;
use crate::receive::COMPOSITION_PROFILE;
use crate::receive::DOCUMENT;
use crate::support::MANIFEST;
use crate::support::archive;
use crate::support::eps;
use crate::support::eps_package;

type TestResult = Result<(), Box<dyn Error>>;

/// The canonical URL of the synthetic profile.
const PROFILE: &str = "http://example.org/fhir/StructureDefinition/synthetic-bound-composition";

/// The value set of the status, expanded in the package.
const STATUSES: &str = "http://example.org/fhir/ValueSet/synthetic-statuses";

/// The value set of the type, expanded in the package.
const TYPES: &str = "http://example.org/fhir/ValueSet/synthetic-types";

/// A value set the package carries without an expansion.
const UNEXPANDED: &str = "http://example.org/fhir/ValueSet/synthetic-unexpanded";

/// A synthetic package: a Composition profile with five `required`
/// bindings, two value sets with an expansion and one without.
fn package() -> Result<Vec<u8>, Box<dyn Error>> {
    let profile = format!(
        r#"{{"resourceType":"StructureDefinition","url":"{PROFILE}","name":"SyntheticBound","kind":"resource","derivation":"constraint","snapshot":{{"element":[
          {{"id":"Composition","path":"Composition","min":0,"max":"*"}},
          {{"id":"Composition.status","path":"Composition.status","min":1,"max":"1","type":[{{"code":"code"}}],"binding":{{"strength":"required","valueSet":"{STATUSES}|1.0.0"}}}},
          {{"id":"Composition.type","path":"Composition.type","min":1,"max":"1","type":[{{"code":"CodeableConcept"}}],"binding":{{"strength":"required","valueSet":"{TYPES}"}}}},
          {{"id":"Composition.confidentiality","path":"Composition.confidentiality","min":0,"max":"1","type":[{{"code":"code"}}],"binding":{{"strength":"required","valueSet":"{UNEXPANDED}"}}}},
          {{"id":"Composition.title","path":"Composition.title","min":0,"max":"1","type":[{{"code":"string"}}],"binding":{{"strength":"required","valueSet":"{UNEXPANDED}"}}}},
          {{"id":"Composition.section","path":"Composition.section","min":0,"max":"*"}},
          {{"id":"Composition.section:first.code","path":"Composition.section.code","min":0,"max":"1","type":[{{"code":"CodeableConcept"}}],"binding":{{"strength":"required","valueSet":"{TYPES}"}}}},
          {{"id":"Composition.language","path":"Composition.language","min":0,"max":"1","type":[{{"code":"code"}}],"binding":{{"strength":"preferred","valueSet":"{UNEXPANDED}"}}}}
        ]}}}}"#
    );
    let statuses = format!(
        r#"{{"resourceType":"ValueSet","url":"{STATUSES}","expansion":{{"contains":[{{"system":"{LOCAL}","code":"final"}},{{"code":"group","contains":[{{"system":"{LOCAL}","code":"preliminary"}}]}}]}}}}"#
    );
    let types = format!(
        r#"{{"resourceType":"ValueSet","url":"{TYPES}","expansion":{{"contains":[{{"system":"{LOCAL}","code":"T-1"}}]}}}}"#
    );
    let unexpanded = format!(
        r#"{{"resourceType":"ValueSet","url":"{UNEXPANDED}","compose":{{"include":[{{"system":"{LOCAL}"}}]}}}}"#
    );
    archive(&[
        ("package/package.json", MANIFEST.to_owned()),
        ("package/StructureDefinition-bound.json", profile),
        ("package/ValueSet-statuses.json", statuses),
        ("package/ValueSet-types.json", types),
        ("package/ValueSet-unexpanded.json", unexpanded),
    ])
}

/// A Composition with `status`, a type coding of `type_code`, and a
/// confidentiality.
fn composition(status: &str, type_code: &str) -> Result<Object, Box<dyn Error>> {
    let text = format!(
        r#"{{"resourceType":"Composition","status":"{status}","type":{{"coding":[{{"system":"http://example.org/other","code":"O-1"}},{{"system":"{LOCAL}","code":"{type_code}"}}]}},"confidentiality":"N","title":"Synthetic"}}"#
    );
    match serde_json::from_str::<Value>(&text)? {
        Value::Object(object) => Ok(object),
        _ => Err("the composition is no object".into()),
    }
}

/// Checks `resource` against the synthetic profile and expansions.
fn checked(resource: &Object) -> Result<BindingCheck, Box<dyn Error>> {
    let package = package()?;
    let profile = ResourceProfile::read(package.as_slice(), PROFILE)?;
    let expansions = Expansions::read(package.as_slice())?;
    Ok(check_required(
        &profile,
        resource,
        "Composition",
        &expansions,
    )?)
}

/// The not evaluated binding of `element` for `gap`.
fn gap(element: &str, value_set: &str, gap: BindingGap) -> UnevaluatedBinding {
    UnevaluatedBinding {
        profile: PROFILE.to_owned(),
        element: element.to_owned(),
        value_set: Some(value_set.to_owned()),
        gap,
    }
}

#[test]
fn a_package_keeps_only_the_value_sets_it_expands() -> TestResult {
    let expansions = Expansions::read(package()?.as_slice())?;
    assert_eq!(expansions.urls().collect::<Vec<_>>(), [STATUSES, TYPES]);
    assert_eq!(
        expansions.contains(STATUSES, Some(LOCAL), "preliminary"),
        Some(true),
        "a nested entry is a member"
    );
    assert_eq!(expansions.contains(UNEXPANDED, None, "N"), None);
    Ok(())
}

#[test]
fn concepts_in_the_expansions_hold_and_the_rest_is_named() -> TestResult {
    let check = checked(&composition("final", "T-1")?)?;
    assert!(check.findings().is_empty(), "{:?}", check.findings());
    assert_eq!(
        check.unevaluated(),
        [
            gap(
                "Composition.confidentiality",
                UNEXPANDED,
                BindingGap::NoExpansion
            ),
            gap("Composition.section:first.code", TYPES, BindingGap::Sliced),
            gap(
                "Composition.title",
                UNEXPANDED,
                BindingGap::Type(String::from("string"))
            ),
        ],
        "a binding the check cannot evaluate is never passed"
    );
    Ok(())
}

#[test]
fn a_code_outside_its_expansion_is_a_finding() -> TestResult {
    let check = checked(&composition("amended", "T-1")?)?;
    let found: Vec<(&str, &str)> = check
        .findings()
        .iter()
        .map(|finding| (finding.element.as_str(), finding.location.as_str()))
        .collect();
    assert_eq!(found, [("Composition.status", "Composition.status")]);
    Ok(())
}

#[test]
fn a_concept_with_no_coding_in_its_expansion_is_a_finding() -> TestResult {
    let check = checked(&composition("final", "T-9")?)?;
    let found: Vec<(&str, &str)> = check
        .findings()
        .iter()
        .map(|finding| (finding.element.as_str(), finding.value_set.as_str()))
        .collect();
    assert_eq!(found, [("Composition.type", TYPES)]);
    Ok(())
}

#[tokio::test]
async fn without_a_server_an_unexpanded_binding_stays_not_evaluated() -> TestResult {
    let check = checked(&composition("final", "T-1")?)?;
    let completed = check.clone().complete(&Terminology::none()).await?;
    assert_eq!(completed, check);
    Ok(())
}

#[tokio::test]
async fn a_server_that_refuses_the_concept_turns_it_into_a_finding() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Valid(false))));
    let completed = checked(&composition("final", "T-1")?)?
        .complete(&terminology)
        .await?;
    let found: Vec<&str> = completed
        .findings()
        .iter()
        .map(|finding| finding.element.as_str())
        .collect();
    assert_eq!(found, ["Composition.confidentiality"]);
    assert!(
        !completed
            .unevaluated()
            .iter()
            .any(|binding| binding.gap == BindingGap::NoExpansion)
    );
    Ok(())
}

#[tokio::test]
async fn a_server_that_admits_the_concept_leaves_no_finding() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Valid(true))));
    let completed = checked(&composition("final", "T-1")?)?
        .complete(&terminology)
        .await?;
    assert_eq!(completed.findings(), []);
    assert_eq!(
        completed.unevaluated().len(),
        2,
        "the slice and the type stay"
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_validation_is_an_error_never_a_pass() -> TestResult {
    let terminology =
        Terminology::server(Arc::new(Scripted(Script::Refuse(StatusCode::BAD_GATEWAY))));
    let refused = checked(&composition("final", "T-1")?)?
        .complete(&terminology)
        .await;
    assert!(matches!(
        refused,
        Err(TerminologyError::Status {
            status: StatusCode::BAD_GATEWAY,
            ..
        })
    ));
    Ok(())
}

/// The vendored EPS package carries no expansion, so every `required`
/// binding the document's Bundle and Composition carry a value for is named
/// as not evaluated.
#[test]
fn the_vendored_patient_summary_package_carries_no_expansion() -> TestResult {
    let expansions = Expansions::read(std::fs::File::open(eps_package())?)?;
    assert_eq!(expansions.urls().count(), 0);
    let document = ReceivedDocument::read(DOCUMENT)?;
    let tree: Value = serde_json::from_str(DOCUMENT)?;
    let Value::Object(tree) = tree else {
        return Err("the document is no object".into());
    };
    let bundle = check_required(&eps(BUNDLE_PROFILE)?, &tree, "Bundle", &expansions)?;
    assert_eq!(bundle.findings(), []);
    assert!(bundle.unevaluated().iter().any(|binding| {
        binding.element == "Bundle.type"
            && binding.value_set.as_deref() == Some("http://hl7.org/fhir/ValueSet/bundle-type")
            && binding.gap == BindingGap::NoExpansion
    }));
    let first = tree
        .get("entry")
        .and_then(Value::as_array)
        .and_then(<[Value]>::first)
        .and_then(|entry| entry.get("resource"))
        .and_then(Value::as_object)
        .ok_or("the document has a first entry")?;
    let composition = check_required(
        &eps(COMPOSITION_PROFILE)?,
        first,
        "Bundle.entry[0].resource",
        &expansions,
    )?;
    assert_eq!(composition.findings(), []);
    assert!(composition.unevaluated().iter().any(|binding| {
        binding.element == "Composition.status" && binding.gap == BindingGap::NoExpansion
    }));
    assert_eq!(document.original(), DOCUMENT);
    Ok(())
}
