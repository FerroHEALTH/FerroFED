// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary document: the section slots held to the vendored HL7
//! Europe Patient Summary composition profile, and a document assembled from
//! a synthetic mapping run, written as the document `Bundle` the profiles
//! constrain.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the vendored profile and the written Bundle are read as JSON"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

use eehrxf::document::{
    Document, DocumentError, EmptyReason, LOINC, PATIENT_SUMMARY, SLOTS, Section, slot,
};
use eehrxf::mapping::Mapping;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;
use flate2::read::GzDecoder;
use serde_json::Value;

use super::mapping::COMPOSITION;
use super::support::eps_package;

/// The base the test documents name their entries under, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The EPS composition profile's file in the package.
const COMPOSITION_FILE: &str = "package/StructureDefinition-composition-eu-eps.json";

/// Reads the JSON member `path` of the vendored EPS package.
fn package_member(path: &str) -> Result<Value, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(eps_package())?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == path {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            return Ok(serde_json::from_str(&text)?);
        }
    }
    Err(format!("the package holds no {path}").into())
}

/// The snapshot elements of the composition profile.
fn composition_elements() -> Result<Vec<Value>, Box<dyn Error>> {
    let profile = package_member(COMPOSITION_FILE)?;
    Ok(profile["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .clone())
}

/// The section slices the composition profile names, with their lower
/// cardinality.
fn profile_slices() -> Result<Vec<(String, u64)>, Box<dyn Error>> {
    Ok(composition_elements()?
        .iter()
        .filter_map(|element| {
            let id = element["id"].as_str()?;
            let slice = id.strip_prefix("Composition.section:")?;
            (!slice.contains('.')).then(|| (slice.to_owned(), element["min"].as_u64().unwrap_or(0)))
        })
        .collect())
}

#[test]
fn every_slot_is_a_section_slice_with_the_code_the_profile_fixes() -> Result<(), Box<dyn Error>> {
    let elements = composition_elements()?;
    let slices = profile_slices()?;
    assert_eq!(slices.len(), SLOTS.len(), "one slot per section slice");
    for (slice, _) in &slices {
        let held = slot(slice).ok_or_else(|| format!("a slot for {slice}"))?;
        let code = elements
            .iter()
            .find(|element| element["id"] == format!("Composition.section:{slice}.code"))
            .ok_or_else(|| format!("{slice}.code"))?;
        let coding = &code["patternCodeableConcept"]["coding"][0];
        assert_eq!(coding["system"], LOINC, "{slice}");
        assert_eq!(coding["code"], held.code, "{slice}");
    }
    let fixed = elements
        .iter()
        .find(|element| element["id"] == "Composition.type")
        .ok_or("Composition.type")?;
    assert_eq!(
        fixed["patternCodeableConcept"]["coding"][0]["code"],
        PATIENT_SUMMARY
    );
    Ok(())
}

/// The fixtures beside this suite.
pub(crate) fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The synthetic mapping of the mapping suite.
fn mapping() -> Result<Mapping, Box<dyn Error>> {
    let opt = std::fs::read_to_string(fixture("opt/allergy.opt"))?;
    Ok(Mapping::compile(
        &opt,
        &[
            fixture("mapping/ferrofed_allergy.yml"),
            fixture("mapping/ferrofed_allergy.context.yml"),
        ],
        &["ferrofed_allergy.context"],
    )?)
}

/// The synthetic patient, its name and birth date absent.
fn patient() -> Patient {
    Patient {
        id: Some(String::from("synthetic-subject")),
        name: vec![HumanName {
            text: Some("Synthetic Subject".into()),
            ..HumanName::default()
        }],
        birth_date: Some("1970-01-01".into()),
        ..Patient::default()
    }
}

/// A document over the synthetic patient whose allergies section holds one
/// mapping run, run `runs` times, and whose other required sections are
/// empty.
fn document(runs: usize) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::to_value(&document_mapped(&mapping()?, runs)?)?)
}

/// The document of [`document`], its allergies mapped by `mapping`.
pub(crate) fn document_mapped(mapping: &Mapping, runs: usize) -> Result<Bundle, Box<dyn Error>> {
    let mut document = Document::new(
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-0000000000aa"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Patient Summary"),
    );
    let subject = document.with_patient(patient())?;
    let device = document.with_author(&Resource::Device(Box::new(Device {
        id: Some(String::from("gateway")),
        device_name: vec![DeviceDeviceName {
            name: "Synthetic gateway".into(),
            r#type: "user-friendly-name".into(),
            ..DeviceDeviceName::default()
        }],
        ..Device::default()
    })))?;
    document.with_author(&Resource::Organization(Box::new(Organization {
        id: Some(String::from("operator")),
        name: Some("Synthetic operator".into()),
        ..Organization::default()
    })))?;
    let mut allergies = Section::new(
        slot("sectionAllergies").ok_or("the allergies slot")?,
        vec![String::from("One synthetic allergy <from one member>.")],
    );
    for _ in 0..runs {
        let context = CallContext::new()
            .with_patient(reference(&subject))
            .with_who(reference(&device));
        let answer = mapping.to_fhir_with(
            COMPOSITION,
            &Settings::new(device.clone(), "2026-10-06T10:00:00Z"),
            context,
        )?;
        let resources: Vec<Resource> = answer
            .bundle()
            .entry
            .iter()
            .filter_map(|entry| entry.resource.clone())
            .collect();
        let added = document.add(&resources)?;
        let first = added.first().ok_or("the run mapped one resource")?;
        allergies = allergies.with_entry(first.clone());
    }
    document.with_section(allergies)?;
    for slice in [
        "sectionProblems",
        "sectionMedications",
        "sectionProceduresHx",
        "sectionMedicalDevices",
    ] {
        let section = Section::new(
            slot(slice).ok_or("a required slot")?,
            vec![String::from("Nothing recorded.")],
        )
        .empty_because(EmptyReason::NilKnown);
        document.with_section(section)?;
    }
    Ok(document.into_bundle()?)
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// The entries of `bundle` by `fullUrl`.
fn entries(bundle: &Value) -> Result<Vec<(String, Value)>, Box<dyn Error>> {
    bundle["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .map(|entry| {
            Ok((
                entry["fullUrl"].as_str().ok_or("a fullUrl")?.to_owned(),
                entry["resource"].clone(),
            ))
        })
        .collect()
}

#[test]
fn the_document_is_a_bundle_the_eps_profiles_constrain() -> Result<(), Box<dyn Error>> {
    let bundle = document(1)?;
    assert_eq!(bundle["resourceType"], "Bundle");
    assert_eq!(bundle["type"], "document", "FHIR R4 documents");
    assert!(bundle["identifier"]["value"].is_string(), "bdl-9");
    assert!(bundle["timestamp"].is_string(), "bdl-10");
    let entries = entries(&bundle)?;
    let urls: BTreeSet<&str> = entries.iter().map(|(url, _)| url.as_str()).collect();
    assert_eq!(urls.len(), entries.len(), "bdl-7: every fullUrl is unique");
    let (_, composition) = entries.first().ok_or("a first entry")?;
    assert_eq!(composition["resourceType"], "Composition");
    assert_eq!(composition["status"], "final");
    assert_eq!(composition["type"]["coding"][0]["code"], PATIENT_SUMMARY);
    let subject = composition["subject"]["reference"]
        .as_str()
        .ok_or("a subject")?;
    assert!(urls.contains(subject), "the subject is an entry");
    assert_eq!(composition["author"].as_array().map(Vec::len), Some(2));
    let sections = composition["section"].as_array().ok_or("sections")?;
    let held: BTreeSet<&str> = sections
        .iter()
        .filter_map(|section| section["code"]["coding"][0]["code"].as_str())
        .collect();
    for (slice, min) in profile_slices()? {
        let code = slot(&slice).ok_or("a slot")?.code;
        if min > 0 {
            assert!(held.contains(code), "the required {slice} is written");
        }
    }
    for section in sections {
        assert!(section["title"].is_string(), "section.title 1..1");
        assert_eq!(section["text"]["status"], "generated");
        let entry = section["entry"].as_array().map_or(0, Vec::len);
        assert!(
            entry == 0 || section["emptyReason"].is_null(),
            "cmp-2: {section}"
        );
        for referenced in section["entry"].as_array().into_iter().flatten() {
            let url = referenced["reference"].as_str().ok_or("a reference")?;
            assert!(urls.contains(url), "{url} is an entry");
        }
    }
    let allergy = entries
        .iter()
        .find(|(_, resource)| resource["resourceType"] == "AllergyIntolerance")
        .ok_or("the mapped allergy")?;
    assert_eq!(
        allergy.1["patient"]["reference"], subject,
        "eps-bundle-patient-ref: the patient is the document's subject"
    );
    let patient = entries
        .iter()
        .find(|(url, _)| url == subject)
        .ok_or("the patient entry")?;
    assert_eq!(
        patient.1["meta"]["profile"][0],
        "http://hl7.eu/fhir/eps/StructureDefinition/patient-eu-eps"
    );
    Ok(())
}

#[test]
fn two_runs_over_one_composition_never_share_a_full_url() -> Result<(), Box<dyn Error>> {
    let bundle = document(2)?;
    let entries = entries(&bundle)?;
    let allergies: Vec<&String> = entries
        .iter()
        .filter(|(_, resource)| resource["resourceType"] == "AllergyIntolerance")
        .map(|(url, _)| url)
        .collect();
    assert_eq!(allergies.len(), 2, "listed twice, never merged");
    assert_ne!(allergies.first(), allergies.get(1));
    let targets: BTreeSet<String> = entries
        .iter()
        .filter(|(_, resource)| resource["resourceType"] == "Provenance")
        .filter_map(|(_, resource)| {
            resource["target"][0]["reference"]
                .as_str()
                .map(|target| format!("{BASE}/{target}"))
        })
        .collect();
    let named: BTreeSet<String> = allergies.into_iter().cloned().collect();
    assert_eq!(targets, named, "each Provenance follows its own run");
    Ok(())
}

#[test]
fn the_narrative_escapes_what_it_writes() -> Result<(), Box<dyn Error>> {
    let bundle = document(1)?;
    let (_, composition) = entries(&bundle)?
        .into_iter()
        .next()
        .ok_or("a composition")?;
    let div = composition["section"][0]["text"]["div"]
        .as_str()
        .ok_or("a div")?;
    assert!(
        div.starts_with(r#"<div xmlns="http://www.w3.org/1999/xhtml">"#),
        "{div}"
    );
    assert!(div.contains("&lt;from one member&gt;"), "{div}");
    Ok(())
}

#[test]
fn a_section_naming_a_resource_the_document_does_not_hold_is_refused() -> Result<(), Box<dyn Error>>
{
    let mut document = Document::new(
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-0000000000ab"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Patient Summary"),
    );
    document.with_patient(patient())?;
    let section = Section::new(slot("sectionProblems").ok_or("a slot")?, Vec::new())
        .with_entry(format!("{BASE}/Condition/absent"));
    document.with_section(section)?;
    let refused = document.into_bundle();
    assert!(
        matches!(
            refused,
            Err(DocumentError::Dangling {
                slice: "sectionProblems"
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_slice_given_twice_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = Document::new(
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-0000000000ac"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Patient Summary"),
    );
    let problems = slot("sectionProblems").ok_or("a slot")?;
    document.with_section(Section::new(problems, Vec::new()))?;
    let refused = document.with_section(Section::new(problems, Vec::new()));
    assert!(
        matches!(
            refused,
            Err(DocumentError::Repeated {
                slice: "sectionProblems"
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_document_with_no_patient_is_refused() {
    let document = Document::new(
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-0000000000ad"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Patient Summary"),
    );
    assert!(matches!(
        document.into_bundle(),
        Err(DocumentError::Dangling { slice: "subject" })
    ));
}
