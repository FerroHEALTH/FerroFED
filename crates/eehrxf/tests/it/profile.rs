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
