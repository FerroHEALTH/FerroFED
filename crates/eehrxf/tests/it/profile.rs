// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A resource profile read from the vendored HL7 Europe Patient Summary
//! package, and the refusals of a package that lacks or doubles it.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::dataset::DatasetError;
use eehrxf::dataset::Max;
use eehrxf::dataset::ResourceProfile;
use eehrxf::dataset::constraint::DiscriminatorKind;
use eehrxf::dataset::constraint::Pattern;
use eehrxf::dataset::constraint::SlicingRules;

use super::support::MANIFEST;
use super::support::archive;
use super::support::eps;
use super::support::eps_package;

/// The EPS composition profile.
const COMPOSITION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

#[test]
fn the_eps_composition_profile_is_read_with_its_slices() -> Result<(), Box<dyn Error>> {
    let profile = eps(COMPOSITION)?;
    assert_eq!(profile.url(), COMPOSITION);
    assert_eq!(profile.name(), "CompositionEuEps");
    assert_eq!(profile.version(), Some("1.0.0-ballot"));
    assert_eq!(
        profile.package().to_string(),
        "hl7.fhir.eu.eps#1.0.0-ballot"
    );
    let section = profile
        .element("Composition.section:sectionAllergies")
        .ok_or("the allergies section slice")?;
    assert_eq!(section.cardinality().lower(), 1);
    assert_eq!(section.cardinality().upper(), Max::Bounded(1));
    let entry = profile
        .element("Composition.section:sectionAllergies.entry:allergyOrIntolerance")
        .ok_or("the allergy entry slice")?;
    assert_eq!(
        entry.target_profiles(),
        ["http://hl7.eu/fhir/base/StructureDefinition/allergyIntolerance-eu-core"]
    );
    Ok(())
}

#[test]
fn a_versioned_target_profile_is_kept_as_written() -> Result<(), Box<dyn Error>> {
    let profile = eps(COMPOSITION)?;
    let entry = profile
        .element("Composition.section:sectionPatientStory.entry")
        .ok_or("the patient story entry")?;
    assert_eq!(
        entry.target_profiles(),
        ["http://hl7.org/fhir/StructureDefinition/Resource|4.0.1"]
    );
    Ok(())
}

#[test]
fn a_profile_the_package_lacks_is_refused() -> Result<(), Box<dyn Error>> {
    let read = ResourceProfile::read(
        std::fs::File::open(eps_package())?,
        "http://example.org/fhir/StructureDefinition/absent",
    );
    assert!(
        matches!(read, Err(DatasetError::MissingProfile { ref url }) if url == "http://example.org/fhir/StructureDefinition/absent"),
        "{read:?}"
    );
    Ok(())
}

/// A synthetic resource profile at `url` with one root element.
fn resource(url: &str) -> String {
    format!(
        r#"{{"resourceType":"StructureDefinition","url":"{url}","name":"ExampleComposition","kind":"resource","derivation":"constraint","snapshot":{{"element":[{{"id":"Composition","path":"Composition","min":0,"max":"*"}}]}}}}"#
    )
}

#[test]
fn a_profile_given_twice_is_refused() -> Result<(), Box<dyn Error>> {
    let url = "http://example.org/fhir/StructureDefinition/ExampleComposition";
    let members = vec![
        ("package/package.json", MANIFEST.to_owned()),
        ("package/StructureDefinition-a.json", resource(url)),
        ("package/StructureDefinition-b.json", resource(url)),
    ];
    let read = ResourceProfile::read(archive(&members)?.as_slice(), url);
    assert!(
        matches!(read, Err(DatasetError::DuplicateUrl { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_profile_without_a_snapshot_is_refused() -> Result<(), Box<dyn Error>> {
    let url = "http://example.org/fhir/StructureDefinition/ExampleComposition";
    let members = vec![
        ("package/package.json", MANIFEST.to_owned()),
        (
            "package/StructureDefinition-a.json",
            format!(
                r#"{{"resourceType":"StructureDefinition","url":"{url}","name":"ExampleComposition","kind":"resource"}}"#
            ),
        ),
    ];
    let read = ResourceProfile::read(archive(&members)?.as_slice(), url);
    assert!(
        matches!(read, Err(DatasetError::MissingSnapshot { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_synthetic_profile_is_read_with_the_manifest() -> Result<(), Box<dyn Error>> {
    let url = "http://example.org/fhir/StructureDefinition/ExampleComposition";
    let members = vec![
        ("package/package.json", MANIFEST.to_owned()),
        ("package/StructureDefinition-a.json", resource(url)),
    ];
    let profile = ResourceProfile::read(archive(&members)?.as_slice(), url)?;
    assert_eq!(
        profile.package().to_string(),
        "example.synthetic.models#0.0.1"
    );
    assert_eq!(profile.version(), None);
    assert_eq!(profile.elements().len(), 1);
    Ok(())
}

#[test]
fn the_eps_sections_are_sliced_by_the_value_of_their_code() -> Result<(), Box<dyn Error>> {
    let profile = eps(COMPOSITION)?;
    let section = profile
        .element("Composition.section")
        .ok_or("the section element")?;
    let slicing = section.slicing().ok_or("the sections are sliced")?;
    assert_eq!(slicing.rules(), SlicingRules::Open);
    assert!(!slicing.ordered());
    let [discriminator] = slicing.discriminators() else {
        return Err(format!("one discriminator: {slicing:?}").into());
    };
    assert_eq!(discriminator.kind(), DiscriminatorKind::Value);
    assert_eq!(discriminator.path(), "code");
    let code = profile
        .element("Composition.section:sectionAllergies.code")
        .ok_or("the allergies section code")?;
    let Some(Pattern::Concept { codings, text }) = code.pattern() else {
        return Err(format!("a CodeableConcept pattern: {:?}", code.pattern()).into());
    };
    assert_eq!(text, &None);
    let [coding] = codings.as_slice() else {
        return Err(format!("one coding: {codings:?}").into());
    };
    assert_eq!(coding.system(), Some("http://loinc.org"));
    assert_eq!(coding.code(), Some("48765-2"));
    assert_eq!(coding.display(), None);
    Ok(())
}

#[test]
fn a_fixed_uri_and_a_pattern_code_are_read_as_primitives() -> Result<(), Box<dyn Error>> {
    let profile = eps(COMPOSITION)?;
    let url = profile
        .element("Composition.extension:version.url")
        .ok_or("the version extension url")?;
    assert_eq!(
        url.pattern(),
        Some(&Pattern::Primitive(String::from(
            "http://hl7.org/fhir/5.0/StructureDefinition/extension-Composition.version"
        )))
    );
    let mode = profile
        .element("Composition.attester:legalAuthenticator.mode")
        .ok_or("the legal authenticator mode")?;
    assert_eq!(
        mode.pattern(),
        Some(&Pattern::Primitive(String::from("legal")))
    );
    Ok(())
}

/// A synthetic Composition profile whose one sliced element carries
/// `member`.
fn sliced(member: &str) -> String {
    format!(
        r#"{{"resourceType":"StructureDefinition","url":"http://example.org/fhir/StructureDefinition/ExampleComposition","name":"ExampleComposition","kind":"resource","derivation":"constraint","snapshot":{{"element":[{{"id":"Composition","path":"Composition","min":0,"max":"*"}},{{"id":"Composition.section","path":"Composition.section","min":0,"max":"*",{member}}}]}}}}"#
    )
}

/// Reads the synthetic profile [`sliced`] writes with `member`.
fn read_sliced(member: &str) -> Result<Result<ResourceProfile, DatasetError>, Box<dyn Error>> {
    let members = vec![
        ("package/package.json", MANIFEST.to_owned()),
        ("package/StructureDefinition-a.json", sliced(member)),
    ];
    Ok(ResourceProfile::read(
        archive(&members)?.as_slice(),
        "http://example.org/fhir/StructureDefinition/ExampleComposition",
    ))
}

#[test]
fn a_discriminator_type_outside_the_r4_value_set_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_sliced(
        r#""slicing":{"discriminator":[{"type":"position","path":"code"}],"rules":"open"}"#,
    )?;
    assert!(
        matches!(read, Err(DatasetError::Slicing { ref path, .. }) if path == "Composition.section"),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_slicing_without_rules_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_sliced(r#""slicing":{"discriminator":[{"type":"value","path":"code"}]}"#)?;
    assert!(
        matches!(read, Err(DatasetError::Slicing { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_fixed_codeable_concept_is_kept_unread_under_its_key() -> Result<(), Box<dyn Error>> {
    let profile = read_sliced(
        r#""fixedCodeableConcept":{"coding":[{"system":"http://loinc.org","code":"11450-4"}]}"#,
    )??;
    let section = profile
        .element("Composition.section")
        .ok_or("the section element")?;
    assert_eq!(
        section.pattern(),
        Some(&Pattern::Unread {
            key: String::from("fixedCodeableConcept")
        })
    );
    Ok(())
}

#[test]
fn a_pattern_coding_with_a_member_the_model_does_not_read_is_kept_unread()
-> Result<(), Box<dyn Error>> {
    let profile = read_sliced(
        r#""patternCoding":{"system":"http://loinc.org","code":"11450-4","userSelected":true}"#,
    )??;
    let section = profile
        .element("Composition.section")
        .ok_or("the section element")?;
    assert_eq!(
        section.pattern(),
        Some(&Pattern::Unread {
            key: String::from("patternCoding")
        })
    );
    Ok(())
}
