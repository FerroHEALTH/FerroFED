// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received document checked against the vendored EPS `Bundle` and
//! `Composition` profiles, and against synthetic profiles for the forms the
//! EPS profiles do not use.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::dataset::ResourceProfile;
use eehrxf::dataset::constraint::DiscriminatorKind;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::conform::CheckError;
use eehrxf::receive::conform::Finding;
use eehrxf::receive::conform::FindingKind;
use eehrxf::receive::conform::Unread;

use super::BUNDLE_PROFILE;
use super::COMPOSITION_PROFILE;
use super::DOCUMENT;
use super::PATIENT_IDENTIFIER;
use super::document_with;
use super::eps_example;
use crate::support::MANIFEST;
use crate::support::archive;
use crate::support::eps;

/// The two EPS profiles a patient summary is checked against.
fn profiles() -> Result<(ResourceProfile, ResourceProfile), Box<dyn Error>> {
    Ok((eps(BUNDLE_PROFILE)?, eps(COMPOSITION_PROFILE)?))
}

/// Checks `text` against the EPS profiles, expecting findings.
fn findings(text: &str) -> Result<Vec<Finding>, Box<dyn Error>> {
    let (bundle, composition) = profiles()?;
    let checked = ReceivedDocument::read(text)?.check(&bundle, &composition);
    match checked {
        Ok(conformance) => Err(format!("the document conforms: {conformance:?}").into()),
        Err(CheckError::NonConformant { findings }) => Ok(findings),
        Err(other) => Err(other.into()),
    }
}

/// Returns whether `findings` holds one at `element` of `kind`.
fn has(findings: &[Finding], element: &str, kind: &FindingKind) -> bool {
    findings
        .iter()
        .any(|finding| finding.element == element && finding.kind == *kind)
}

#[test]
fn the_synthetic_patient_summary_conforms_to_the_eps_profiles() -> Result<(), Box<dyn Error>> {
    let (bundle, composition) = profiles()?;
    let conformance = ReceivedDocument::read(DOCUMENT)?.check(&bundle, &composition)?;
    let entry_by_profile = conformance.unevaluated().iter().any(|unevaluated| {
        unevaluated.profile == BUNDLE_PROFILE
            && unevaluated.element == "Bundle.entry"
            && unevaluated.reason
                == Unread::Discriminator {
                    kind: DiscriminatorKind::Profile,
                    path: String::from("resource"),
                }
    });
    assert!(
        entry_by_profile,
        "the profile discriminator of Bundle.entry is listed as not evaluated: {conformance:?}"
    );
    Ok(())
}

#[test]
fn a_missing_required_section_is_found() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""code": { "coding": [{ "system": "http://loinc.org", "code": "47519-4" }] }"#,
        r#""code": { "coding": [{ "system": "http://loinc.org", "code": "29762-2" }] }"#,
    )?;
    let findings = findings(&text)?;
    assert!(
        has(
            &findings,
            "Composition.section:sectionProceduresHx",
            &FindingKind::TooFew { min: 1, found: 0 }
        ),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn fewer_sections_than_the_profile_requires_are_found() -> Result<(), Box<dyn Error>> {
    let start = DOCUMENT
        .find(
            r#"          {
            "title": "Medical devices","#,
        )
        .ok_or("the last section")?;
    let end = DOCUMENT
        .find("        ]\n      }\n    },\n    {\n      \"fullUrl\": \"http://example.org/fhir/Patient/")
        .ok_or("the end of the sections")?;
    let last = DOCUMENT.get(start..end).ok_or("the last section text")?;
    let text = DOCUMENT.replacen(&format!(",\n{last}"), "\n", 1);
    let findings = findings(&text)?;
    assert!(
        has(
            &findings,
            "Composition.section",
            &FindingKind::TooFew { min: 5, found: 4 }
        ),
        "{findings:?}"
    );
    assert!(
        has(
            &findings,
            "Composition.section:sectionMedicalDevices",
            &FindingKind::TooFew { min: 1, found: 0 }
        ),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_document_type_other_than_the_pattern_is_found() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""type": { "coding": [{ "system": "http://loinc.org", "code": "60591-5" }] }"#,
        r#""type": { "coding": [{ "system": "http://loinc.org", "code": "34133-9" }] }"#,
    )?;
    let findings = findings(&text)?;
    assert!(
        has(&findings, "Composition.type", &FindingKind::Pattern),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_section_without_a_title_is_found_on_the_section_and_its_slice() -> Result<(), Box<dyn Error>> {
    let text = document_with(r#""title": "Medication","#, "")?;
    let findings = findings(&text)?;
    assert!(
        has(
            &findings,
            "Composition.section.title",
            &FindingKind::TooFew { min: 1, found: 0 }
        ),
        "{findings:?}"
    );
    assert!(
        has(
            &findings,
            "Composition.section:sectionMedications.title",
            &FindingKind::TooFew { min: 1, found: 0 }
        ),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_nested_section_the_profile_removes_is_found() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""text": { "status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">No problems recorded</div>" }"#,
        r#""text": { "status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">No problems recorded</div>" },
            "section": [{ "title": "Nested", "text": { "status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">Nested</div>" } }]"#,
    )?;
    let findings = findings(&text)?;
    assert!(
        has(
            &findings,
            "Composition.section.section",
            &FindingKind::TooMany { max: 0, found: 1 }
        ),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_bundle_entry_without_a_full_url_is_found() -> Result<(), Box<dyn Error>> {
    // The allergy names its patient by an absolute reference, which resolves
    // from an entry with no fullUrl; a relative one would not.
    let text = document_with(
        r#""fullUrl": "http://example.org/fhir/AllergyIntolerance/synthetic-allergy","#,
        "",
    )?
    .replacen(
        r#""patient": { "reference": "Patient/synthetic-patient" }"#,
        r#""patient": { "reference": "http://example.org/fhir/Patient/synthetic-patient" }"#,
        1,
    );
    let findings = findings(&text)?;
    assert!(
        has(
            &findings,
            "Bundle.entry.fullUrl",
            &FindingKind::TooFew { min: 1, found: 0 }
        ),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_finding_names_its_place_and_never_a_value_of_the_document() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""type": { "coding": [{ "system": "http://loinc.org", "code": "60591-5" }] }"#,
        r#""type": { "coding": [{ "system": "http://loinc.org", "code": "34133-9" }] }"#,
    )?;
    let (bundle, composition) = profiles()?;
    let error = ReceivedDocument::read(&text)?
        .check(&bundle, &composition)
        .err()
        .ok_or("the document conforms")?;
    let shown = error.to_string();
    assert!(shown.contains("Composition.type"), "{shown}");
    assert!(!shown.contains("34133-9"), "{shown}");
    assert!(!shown.contains(PATIENT_IDENTIFIER), "{shown}");
    Ok(())
}

#[test]
fn a_profile_of_another_resource_type_is_refused() -> Result<(), Box<dyn Error>> {
    let (bundle, composition) = profiles()?;
    let checked = ReceivedDocument::read(DOCUMENT)?.check(&composition, &bundle);
    assert!(
        matches!(
            checked,
            Err(CheckError::ProfileType {
                expected: "Bundle",
                ..
            })
        ),
        "{checked:?}"
    );
    Ok(())
}

/// The canonical URL of the synthetic Composition profile.
const SYNTHETIC: &str = "http://example.org/fhir/StructureDefinition/synthetic-composition";

/// A synthetic Composition profile whose sections are sliced by `code` with
/// `rules`, one slice for the problems section, and `extra` elements.
fn synthetic(rules: &str, extra: &str) -> Result<ResourceProfile, Box<dyn Error>> {
    let definition = format!(
        r#"{{"resourceType":"StructureDefinition","url":"{SYNTHETIC}","name":"SyntheticComposition","kind":"resource","derivation":"constraint","snapshot":{{"element":[
          {{"id":"Composition","path":"Composition","min":0,"max":"*"}},
          {{"id":"Composition.section","path":"Composition.section","min":0,"max":"*","slicing":{{"discriminator":[{{"type":"value","path":"code"}}],"rules":"{rules}"}}}},
          {{"id":"Composition.section.code","path":"Composition.section.code","min":0,"max":"1"}},
          {{"id":"Composition.section:problems","path":"Composition.section","sliceName":"problems","min":0,"max":"1"}},
          {{"id":"Composition.section:problems.code","path":"Composition.section.code","min":1,"max":"1","patternCodeableConcept":{{"coding":[{{"system":"http://loinc.org","code":"11450-4"}}]}}}}{extra}
        ]}}}}"#
    );
    let members = vec![
        ("package/package.json", MANIFEST.to_owned()),
        ("package/StructureDefinition-synthetic.json", definition),
    ];
    Ok(ResourceProfile::read(
        archive(&members)?.as_slice(),
        SYNTHETIC,
    )?)
}

#[test]
fn a_closed_slicing_finds_every_section_no_slice_admits() -> Result<(), Box<dyn Error>> {
    let (bundle, _) = profiles()?;
    let profile = synthetic("closed", "")?;
    let checked = ReceivedDocument::read(DOCUMENT)?.check(&bundle, &profile);
    let Err(CheckError::NonConformant { findings }) = checked else {
        return Err(format!("{checked:?}").into());
    };
    let unsliced = findings
        .iter()
        .filter(|finding| finding.kind == FindingKind::NoSlice)
        .count();
    assert_eq!(unsliced, 4, "four sections are not problems: {findings:?}");
    Ok(())
}

#[test]
fn an_open_slicing_admits_sections_no_slice_names() -> Result<(), Box<dyn Error>> {
    let (bundle, _) = profiles()?;
    let profile = synthetic("open", "")?;
    ReceivedDocument::read(DOCUMENT)?.check(&bundle, &profile)?;
    Ok(())
}

#[test]
fn a_pattern_form_the_model_does_not_read_is_listed_as_not_evaluated() -> Result<(), Box<dyn Error>>
{
    let (bundle, _) = profiles()?;
    let profile = synthetic(
        "open",
        r#",{"id":"Composition.date","path":"Composition.date","min":1,"max":"1","fixedDateTime":"2026-10-06T10:00:00Z","patternQuantity":{"value":1}}"#,
    )?;
    let conformance = ReceivedDocument::read(DOCUMENT)?.check(&bundle, &profile)?;
    assert!(
        conformance.unevaluated().iter().any(|unevaluated| {
            unevaluated.element == "Composition.date"
                && matches!(unevaluated.reason, Unread::Pattern(ref key) if key.contains("patternQuantity"))
        }),
        "{conformance:?}"
    );
    Ok(())
}

#[test]
fn the_published_eps_examples_are_read_and_conform() -> Result<(), Box<dyn Error>> {
    let (bundle, composition) = profiles()?;
    for file in [
        "Bundle-EPSExampleBundle01NoProblemsMedicationAllergies.json",
        "Bundle-Instance-Bundle-1a24e60d-9b12-4109-a50a-07249a4f21c3.json",
    ] {
        let text = eps_example(file)?;
        let document = ReceivedDocument::read(&text)?;
        let conformance = document.check(&bundle, &composition)?;
        assert!(
            conformance
                .unevaluated()
                .iter()
                .any(|unevaluated| unevaluated.element == "Bundle.entry"),
            "{file}: the entry profiles are listed as not evaluated"
        );
    }
    Ok(())
}
