// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging report document: the section slots and categories held to the
//! vendored HL7 Europe Imaging Report 1.0.0-ballot profiles, and a document
//! assembled from synthetic resources, written as the `BundleReportEuImaging`
//! the profiles constrain, its composition's `Organization` authors the
//! report's performers (`dr-comp-author-org`) and the operator its custodian.

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

use eehrxf::document::imaging::{
    IMAGING_BUNDLE, IMAGING_CATEGORIES, IMAGING_REPORT, IMAGING_REPORT_FORM, IMAGING_SLOTS,
};
use eehrxf::document::{Document, LOINC, Section};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::device::Device;
use fhir_types::r4::diagnostic_report::DiagnosticReport;
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::imaging_study::ImagingStudy;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use flate2::read::GzDecoder;
use serde_json::Value;

use super::support::imaging_package;

/// The base the test documents name their entries under, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The synthetic document identifier.
const IDENTIFIER: &str = "urn:uuid:5b0c3f7e-0000-4000-8000-00000000c001";

/// The LOINC code of the synthetic report, an MR of the brain.
const MR_BRAIN: &str = "24802-1";

/// Reads the JSON member `path` of the vendored Imaging Report package.
fn package_member(path: &str) -> Result<Value, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(imaging_package())?));
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

/// The snapshot element `id` of `profile`.
fn element<'a>(profile: &'a Value, id: &str) -> Result<&'a Value, Box<dyn Error>> {
    profile["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .iter()
        .find(|element| element["id"] == id)
        .ok_or_else(|| format!("the element {id}").into())
}

#[test]
fn every_slot_and_category_is_the_profile_s() -> Result<(), Box<dyn Error>> {
    let composition = package_member("package/StructureDefinition-CompositionEuImaging.json")?;
    assert_eq!(composition["url"], IMAGING_REPORT_FORM.composition);
    let slices: Vec<&str> = composition["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .iter()
        .filter_map(|element| {
            let slice = element["id"]
                .as_str()?
                .strip_prefix("Composition.section:")?;
            (!slice.contains('.')).then_some(slice)
        })
        .collect();
    let slots: Vec<&str> = IMAGING_SLOTS.iter().map(|slot| slot.slice).collect();
    assert_eq!(slices, slots, "one slot per section slice, in order");
    for slot in &IMAGING_SLOTS {
        let code = element(
            &composition,
            &format!("Composition.section:{}.code", slot.slice),
        )?;
        let coding = &code["patternCodeableConcept"]["coding"][0];
        assert_eq!(coding["code"], slot.code, "{}", slot.slice);
    }
    for (slice, category) in ["imaging-report", "imaging"]
        .iter()
        .zip(&IMAGING_CATEGORIES)
    {
        for (profile, path) in [
            (&composition, "Composition"),
            (
                &package_member("package/StructureDefinition-DiagnosticReportEuImaging.json")?,
                "DiagnosticReport",
            ),
        ] {
            let pattern = &element(profile, &format!("{path}.category:{slice}"))?["patternCodeableConcept"]
                ["coding"][0];
            assert_eq!(pattern["system"], category.system, "{path} {slice}");
            assert_eq!(pattern["code"], category.code, "{path} {slice}");
        }
    }
    let bundle = package_member("package/StructureDefinition-BundleReportEuImaging.json")?;
    assert_eq!(bundle["url"], IMAGING_BUNDLE);
    let set = package_member("package/ValueSet-ImagingReportTypesEuVSEuImaging.json")?;
    let filter = &set["compose"]["include"][0]["filter"][0];
    assert_eq!(
        (&filter["op"], &filter["value"]),
        (&Value::from("is-a"), &Value::from(IMAGING_REPORT)),
        "the fallback type is the concept the report types descend from"
    );
    Ok(())
}

/// A literal reference to `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// A synthetic imaging report document, its report coded `code` and
/// performed by the radiology department.
fn document(code: Option<&str>) -> Result<Document, Box<dyn Error>> {
    let mut document = Document::of(
        &IMAGING_REPORT_FORM,
        BASE,
        IDENTIFIER.to_owned(),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Imaging report"),
    );
    let subject = document.with_patient(Patient {
        id: Some(String::from("patient-1")),
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            ..HumanName::default()
        }],
        ..Patient::default()
    })?;
    document.with_author(&Resource::Device(Box::new(Device {
        id: Some(String::from("gateway")),
        ..Device::default()
    })))?;
    document.with_author(&Resource::Organization(Box::new(Organization {
        id: Some(String::from("operator")),
        name: Some("Synthetic Operator".into()),
        ..Organization::default()
    })))?;
    let added = document.add(&[
        Resource::Organization(Box::new(Organization {
            id: Some(String::from("radiology")),
            name: Some("Synthetic Radiology".into()),
            ..Organization::default()
        })),
        Resource::ImagingStudy(Box::new(ImagingStudy {
            id: Some(String::from("study-1")),
            status: "available".into(),
            subject: reference(&subject),
            ..ImagingStudy::default()
        })),
    ])?;
    let study = added.get(1).ok_or("the study's fullUrl")?.clone();
    for slot in IMAGING_SLOTS.iter().take(4) {
        let mut section = Section::new(slot, vec![format!("{}.", slot.title)]);
        if slot.slice == "imagingstudy" {
            section = section.with_entry(study.clone());
        }
        document.with_section(section)?;
    }
    document.with_report(DiagnosticReport {
        id: Some(String::from("imaging-report")),
        status: "final".into(),
        code: CodeableConcept {
            coding: code
                .map(|code| Coding {
                    system: Some(LOINC.into()),
                    code: Some(code.into()),
                    ..Coding::default()
                })
                .into_iter()
                .collect(),
            ..CodeableConcept::default()
        },
        performer: vec![reference("Organization/radiology")],
        ..DiagnosticReport::default()
    })?;
    Ok(document)
}

/// The one resource of `bundle` of type `kind`.
fn the<'a>(bundle: &'a Value, kind: &str) -> Result<&'a Value, Box<dyn Error>> {
    let found: Vec<&Value> = bundle["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind)
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        _ => Err(format!("one {kind}, found {}", found.len()).into()),
    }
}

#[test]
fn the_document_is_a_bundle_the_imaging_report_profiles_constrain() -> Result<(), Box<dyn Error>> {
    let bundle = serde_json::to_value(document(Some(MR_BRAIN))?.into_bundle()?)?;
    assert_eq!(bundle["meta"]["profile"][0], IMAGING_BUNDLE);
    let entries = bundle["entry"].as_array().ok_or("entries")?;
    assert!(entries.len() >= 4, "Bundle.entry min 4");
    assert_eq!(entries[0]["resource"]["resourceType"], "Composition");
    assert_eq!(entries[1]["resource"]["resourceType"], "DiagnosticReport");
    let composition = the(&bundle, "Composition")?;
    let report = the(&bundle, "DiagnosticReport")?;
    the(&bundle, "ImagingStudy")?;

    assert_eq!(
        composition["type"]["coding"][0]["code"], MR_BRAIN,
        "the composition's type is the report's code"
    );
    for category in &IMAGING_CATEGORIES {
        for (name, resource) in [("Composition", composition), ("DiagnosticReport", report)] {
            assert!(
                resource["category"]
                    .as_array()
                    .ok_or("categories")?
                    .iter()
                    .any(|held| held["coding"][0]["code"] == category.code),
                "{name} carries {}",
                category.code
            );
        }
    }
    assert!(
        report["identifier"][0].is_object(),
        "DiagnosticReportEuImaging.identifier is 1..*"
    );

    let organizations = |references: &Value| -> Vec<Value> {
        references
            .as_array()
            .into_iter()
            .flatten()
            .filter(|held| {
                held["reference"]
                    .as_str()
                    .is_some_and(|text| text.contains("Organization/"))
            })
            .cloned()
            .collect()
    };
    assert_eq!(
        organizations(&composition["author"]),
        organizations(&report["performer"]),
        "dr-comp-author-org: the composition's Organization authors are the report's performers"
    );
    assert_eq!(
        composition["custodian"]["reference"],
        format!("{BASE}/Organization/operator"),
        "A109: the operator is the custodian"
    );
    assert!(
        composition["author"]
            .as_array()
            .ok_or("authors")?
            .iter()
            .any(|author| author["reference"] == format!("{BASE}/Device/gateway")),
        "the gateway Device stays an author"
    );
    let codes: Vec<&Value> = composition["section"]
        .as_array()
        .ok_or("sections")?
        .iter()
        .map(|section| &section["code"]["coding"][0]["code"])
        .collect();
    for required in ["18726-0", "55115-0", "11329-0", "55111-9"] {
        assert!(codes.iter().any(|code| *code == required), "{required}");
    }
    Ok(())
}

#[test]
fn a_report_with_no_code_is_typed_as_an_imaging_report() -> Result<(), Box<dyn Error>> {
    let bundle = serde_json::to_value(document(None)?.into_bundle()?)?;
    let composition = the(&bundle, "Composition")?;
    let report = the(&bundle, "DiagnosticReport")?;
    assert_eq!(composition["type"]["coding"][0]["code"], IMAGING_REPORT);
    assert_eq!(report["code"]["coding"][0]["code"], IMAGING_REPORT);
    Ok(())
}
