// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report document: the section slots held to the vendored
//! HL7 Europe Hospital Discharge Report 1.0.0 composition profile, and a
//! document assembled from synthetic resources, written as the
//! `bundle-eu-hdr` the profiles constrain, with its encounter and language.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the vendored profile and the written Bundle are read as JSON"
)]

use std::error::Error;
use std::io::Read;

use eehrxf::document::discharge::{
    DISCHARGE_REPORT_FORM, DISCHARGE_SUMMARY, HDR_BUNDLE, HDR_ENCOUNTER, HDR_SLOTS,
};
use eehrxf::document::{Document, DocumentError, LOINC, Section};
use fhir_types::r4::coding::Coding;
use fhir_types::r4::encounter::Encounter;
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::patient::Patient;
use flate2::read::GzDecoder;
use serde_json::Value;

use super::support::hdr_package;

/// The base the test documents name their entries under, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The synthetic document identifier.
const IDENTIFIER: &str = "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d001";

/// Reads the JSON member `path` of the vendored Hospital Discharge Report
/// package.
fn package_member(path: &str) -> Result<Value, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(hdr_package())?));
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

#[test]
fn every_slot_is_a_section_slice_with_the_code_the_profile_fixes() -> Result<(), Box<dyn Error>> {
    let composition = package_member("package/StructureDefinition-composition-eu-hdr.json")?;
    assert_eq!(composition["url"], DISCHARGE_REPORT_FORM.composition);
    let elements = composition["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?;
    let slices: Vec<&str> = elements
        .iter()
        .filter_map(|element| {
            let slice = element["id"]
                .as_str()?
                .strip_prefix("Composition.section:")?;
            (!slice.contains('.')).then_some(slice)
        })
        .collect();
    let slots: Vec<&str> = HDR_SLOTS.iter().map(|slot| slot.slice).collect();
    assert_eq!(slices, slots, "one slot per section slice, in order");
    for slot in &HDR_SLOTS {
        let code = elements
            .iter()
            .find(|element| element["id"] == format!("Composition.section:{}.code", slot.slice))
            .ok_or_else(|| format!("{}.code", slot.slice))?;
        let coding = &code["patternCodeableConcept"]["coding"][0];
        assert_eq!(coding["system"], LOINC, "{}", slot.slice);
        assert_eq!(coding["code"], slot.code, "{}", slot.slice);
    }
    let kind = elements
        .iter()
        .find(|element| element["id"] == "Composition.type")
        .ok_or("Composition.type")?;
    assert_eq!(
        kind["patternCodeableConcept"]["coding"][0]["code"],
        DISCHARGE_SUMMARY
    );
    let encounter = elements
        .iter()
        .find(|element| element["id"] == "Composition.encounter")
        .ok_or("Composition.encounter")?;
    assert_eq!(encounter["min"], 1, "the profile requires the encounter");
    assert_eq!(encounter["type"][0]["targetProfile"][0], HDR_ENCOUNTER);
    let bundle = package_member("package/StructureDefinition-bundle-eu-hdr.json")?;
    assert_eq!(bundle["url"], HDR_BUNDLE);
    let language = bundle["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .iter()
        .find(|element| element["id"] == "Bundle.language")
        .ok_or("Bundle.language")?;
    assert_eq!(language["min"], 1, "the profile requires the language");
    Ok(())
}

/// A discharge report document over a synthetic patient, given its
/// encounter when `encounter` and its language when `language`.
fn document(encounter: bool, language: bool) -> Result<Document, Box<dyn Error>> {
    let mut document = Document::of(
        &DISCHARGE_REPORT_FORM,
        BASE,
        IDENTIFIER.to_owned(),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Discharge report"),
    );
    document.with_patient(Patient {
        id: Some(String::from("patient-1")),
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            ..HumanName::default()
        }],
        ..Patient::default()
    })?;
    if encounter {
        document.with_encounter(Encounter {
            id: Some(String::from("stay-1")),
            status: "finished".into(),
            class: Coding {
                system: Some("http://terminology.hl7.org/CodeSystem/v3-ActCode".into()),
                code: Some("IMP".into()),
                ..Coding::default()
            },
            ..Encounter::default()
        })?;
    }
    if language {
        document.with_language(String::from("en"));
    }
    let course = HDR_SLOTS
        .iter()
        .find(|slot| slot.slice == "sectionHospitalCourse")
        .ok_or("the hospital course slot")?;
    document.with_section(Section::new(
        course,
        vec![String::from("A synthetic stay.")],
    ))?;
    Ok(document)
}

#[test]
fn the_document_is_a_bundle_the_discharge_report_profiles_constrain() -> Result<(), Box<dyn Error>>
{
    let bundle = serde_json::to_value(document(true, true)?.into_bundle()?)?;
    assert_eq!(bundle["type"], "document");
    assert_eq!(bundle["meta"]["profile"][0], HDR_BUNDLE);
    assert_eq!(bundle["language"], "en");
    assert_eq!(bundle["identifier"]["value"], IDENTIFIER);
    let entries = bundle["entry"].as_array().ok_or("entries")?;
    assert!(entries.len() >= 3, "Bundle.entry min 3");
    assert_eq!(entries[0]["resource"]["resourceType"], "Composition");
    assert!(
        entries
            .iter()
            .skip(1)
            .all(|entry| entry["resource"]["resourceType"] != "Composition"),
        "bdl-hdr-1"
    );
    let composition = &entries[0]["resource"];
    assert_eq!(composition["type"]["coding"][0]["code"], DISCHARGE_SUMMARY);
    assert_eq!(composition["subject"]["reference"], "Patient/patient-1");
    assert_eq!(composition["encounter"]["reference"], "Encounter/stay-1");
    let encounter = entries
        .iter()
        .find(|entry| entry["resource"]["resourceType"] == "Encounter")
        .ok_or("the encounter entry")?;
    assert_eq!(encounter["resource"]["meta"]["profile"][0], HDR_ENCOUNTER);
    let patient = entries
        .iter()
        .find(|entry| entry["resource"]["resourceType"] == "Patient")
        .ok_or("the patient entry")?;
    assert_eq!(
        patient["resource"]["meta"]["profile"][0],
        DISCHARGE_REPORT_FORM.patient
    );
    let codes: Vec<&Value> = composition["section"]
        .as_array()
        .ok_or("sections")?
        .iter()
        .map(|section| &section["code"]["coding"][0]["code"])
        .collect();
    assert!(
        codes
            .iter()
            .any(|code| *code == "18842-5" || *code == "8648-8"),
        "cmp-hdr-2"
    );
    Ok(())
}

#[test]
fn a_discharge_report_without_its_encounter_or_its_language_is_refused()
-> Result<(), Box<dyn Error>> {
    assert!(matches!(
        document(false, true)?.into_bundle(),
        Err(DocumentError::Missing { part: "encounter" })
    ));
    assert!(matches!(
        document(true, false)?.into_bundle(),
        Err(DocumentError::Missing { part: "language" })
    ));
    Ok(())
}
