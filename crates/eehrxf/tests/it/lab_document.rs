// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report document: the section slots held to the vendored
//! HL7 Europe Laboratory Report 2.0.0 profiles, and a document assembled
//! from synthetic resources, written as the `Bundle-eu-lab` the profiles
//! constrain, its report and its composition held to one another.

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

use eehrxf::document::laboratory::{
    LAB_BUNDLE, LAB_SLOTS, LABORATORY_REPORT, LABORATORY_REPORT_FORM,
};
use eehrxf::document::{Document, DocumentError, LOINC, Section};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::diagnostic_report::DiagnosticReport;
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::observation::Observation;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use flate2::read::GzDecoder;
use serde_json::Value;

use super::support::lab_package;

/// The base the test documents name their entries under, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The synthetic document identifier.
const IDENTIFIER: &str = "urn:uuid:5b0c3f7e-0000-4000-8000-00000000a001";

/// Reads the JSON member `path` of the vendored Laboratory Report package.
fn package_member(path: &str) -> Result<Value, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(lab_package())?));
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

/// The LOINC codes the value set `file` includes.
fn value_set_codes(file: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let set = package_member(file)?;
    Ok(set["compose"]["include"]
        .as_array()
        .ok_or("an include")?
        .iter()
        .filter(|include| include["system"] == LOINC)
        .flat_map(|include| include["concept"].as_array().cloned().unwrap_or_default())
        .filter_map(|concept| concept["code"].as_str().map(str::to_owned))
        .collect())
}

#[test]
fn every_slot_is_a_study_type_or_a_section_slice_of_the_composition_profile()
-> Result<(), Box<dyn Error>> {
    let study_types = value_set_codes("package/ValueSet-lab-studyType-eu-lab.json")?;
    let composition = package_member("package/StructureDefinition-Composition-eu-lab.json")?;
    let elements = composition["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?;
    for slot in &LAB_SLOTS {
        let sliced = elements
            .iter()
            .find(|element| element["id"] == format!("Composition.section:{}.code", slot.slice));
        match sliced {
            Some(code) => {
                let coding = &code["patternCodeableConcept"]["coding"][0];
                assert_eq!(coding["system"], LOINC, "{}", slot.slice);
                assert_eq!(coding["code"], slot.code, "{}", slot.slice);
            }
            None => assert!(
                study_types.iter().any(|code| code == slot.code),
                "{} is a study type of the value set",
                slot.slice
            ),
        }
    }
    assert_eq!(
        value_set_codes("package/ValueSet-lab-reportType-eu-lab.json")?,
        [LABORATORY_REPORT],
        "the document type is the one report type"
    );
    let bundle = package_member("package/StructureDefinition-Bundle-eu-lab.json")?;
    assert_eq!(bundle["url"], LAB_BUNDLE);
    assert_eq!(
        composition["url"], LABORATORY_REPORT_FORM.composition,
        "the form names the vendored composition profile"
    );
    Ok(())
}

/// The synthetic patient.
fn patient() -> Patient {
    Patient {
        id: Some(String::from("patient-1")),
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            ..HumanName::default()
        }],
        ..Patient::default()
    }
}

/// A synthetic glucose result.
fn glucose() -> Resource {
    Resource::Observation(Box::new(Observation {
        id: Some(String::from("glucose")),
        status: "final".into(),
        code: CodeableConcept {
            coding: vec![Coding {
                system: Some(LOINC.into()),
                code: Some("2345-7".into()),
                ..Coding::default()
            }],
            ..CodeableConcept::default()
        },
        ..Observation::default()
    }))
}

/// The synthetic report, naming `result` among its results.
fn report(result: &str) -> DiagnosticReport {
    DiagnosticReport {
        id: Some(String::from("lab-report")),
        status: "final".into(),
        result: vec![Reference {
            reference: Some(result.into()),
            ..Reference::default()
        }],
        ..DiagnosticReport::default()
    }
}

/// A laboratory report document over the synthetic resources.
fn document() -> Result<Document, Box<dyn Error>> {
    let mut document = Document::of(
        &LABORATORY_REPORT_FORM,
        BASE,
        IDENTIFIER.to_owned(),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Laboratory report"),
    );
    document.with_patient(patient())?;
    let urls = document.add(&[glucose()])?;
    let glucose_url = urls.first().ok_or("the result's fullUrl")?.clone();
    let chemistry = LAB_SLOTS
        .iter()
        .find(|slot| slot.code == "18719-5")
        .ok_or("the chemistry slot")?;
    document.with_section(
        Section::new(chemistry, vec![String::from("Glucose.")]).with_entry(glucose_url),
    )?;
    document.with_report(report("Observation/glucose"))?;
    Ok(document)
}

/// The resources of `bundle` of type `kind`.
fn of_type<'a>(bundle: &'a Value, kind: &str) -> Vec<&'a Value> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind)
        .collect()
}

/// The `valueReference` of the extension `url` of `resource`.
fn extension_reference<'a>(resource: &'a Value, url: &str) -> Option<&'a str> {
    resource["extension"]
        .as_array()?
        .iter()
        .find(|extension| extension["url"] == url)?["valueReference"]["reference"]
        .as_str()
}

#[test]
fn the_document_is_a_bundle_the_laboratory_report_profiles_constrain() -> Result<(), Box<dyn Error>>
{
    let bundle = serde_json::to_value(document()?.into_bundle()?)?;
    assert_eq!(bundle["type"], "document");
    assert_eq!(bundle["meta"]["profile"][0], LAB_BUNDLE);
    assert_eq!(bundle["identifier"]["value"], IDENTIFIER);
    assert!(bundle["timestamp"].is_string());
    let entries = bundle["entry"].as_array().ok_or("entries")?;
    assert!(entries.len() >= 2, "Bundle.entry min 2");
    assert!(
        entries.iter().all(|entry| entry["fullUrl"].is_string()),
        "every entry has a fullUrl"
    );
    assert_eq!(entries[0]["resource"]["resourceType"], "Composition");
    assert_eq!(entries[1]["resource"]["resourceType"], "DiagnosticReport");

    let compositions = of_type(&bundle, "Composition");
    let reports = of_type(&bundle, "DiagnosticReport");
    assert_eq!(compositions.len(), 1, "one-comp");
    assert_eq!(reports.len(), 1, "one-dr");
    let (composition, report) = (compositions[0], reports[0]);

    assert!(
        report["identifier"]
            .as_array()
            .ok_or("the report's identifiers")?
            .contains(&composition["identifier"]),
        "dr-comp-identifier"
    );
    let kind = &composition["type"]["coding"][0];
    assert_eq!(kind["system"], LOINC);
    assert_eq!(kind["code"], LABORATORY_REPORT);
    assert!(
        report["code"]["coding"]
            .as_array()
            .ok_or("the report's codings")?
            .contains(kind),
        "dr-comp-type"
    );
    assert_eq!(composition["subject"], report["subject"], "dr-comp-subj");
    assert_eq!(composition["subject"]["reference"], "Patient/patient-1");
    assert!(composition.get("encounter").is_none() && report.get("encounter").is_none());
    assert!(composition.get("category").is_none() && report.get("category").is_none());

    let companion = LABORATORY_REPORT_FORM.report.ok_or("the form's report")?;
    assert_eq!(report["meta"]["profile"][0], companion.profile);
    assert_eq!(
        extension_reference(report, companion.back),
        entries[0]["fullUrl"].as_str(),
        "the report names the composition"
    );
    assert_eq!(
        extension_reference(composition, companion.link),
        entries[1]["fullUrl"].as_str(),
        "the composition names the report"
    );

    let section = &composition["section"][0];
    assert_eq!(section["code"]["coding"][0]["code"], "18719-5");
    assert!(section["title"].is_string() && section["text"]["div"].is_string());
    let patient = of_type(&bundle, "Patient");
    assert_eq!(
        patient[0]["meta"]["profile"][0],
        LABORATORY_REPORT_FORM.patient
    );
    Ok(())
}

#[test]
fn a_report_category_is_written_on_the_composition_too() -> Result<(), Box<dyn Error>> {
    let mut document = document()?;
    let mut categorised = report("Observation/glucose");
    categorised.category = vec![CodeableConcept {
        coding: vec![Coding {
            system: Some(LOINC.into()),
            code: Some("18719-5".into()),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    }];
    document.with_report(categorised)?;
    let bundle = serde_json::to_value(document.into_bundle()?)?;
    assert_eq!(
        of_type(&bundle, "Composition")[0]["category"],
        of_type(&bundle, "DiagnosticReport")[0]["category"],
        "dr-comp-category"
    );
    Ok(())
}

#[test]
fn a_laboratory_report_without_its_report_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = Document::of(
        &LABORATORY_REPORT_FORM,
        BASE,
        IDENTIFIER.to_owned(),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Laboratory report"),
    );
    document.with_patient(patient())?;
    assert!(matches!(
        document.into_bundle(),
        Err(DocumentError::Report {
            expected: "requires"
        })
    ));
    Ok(())
}

#[cfg(feature = "patient-summary")]
#[test]
fn a_section_of_another_form_or_a_report_the_form_holds_none_of_is_refused()
-> Result<(), Box<dyn Error>> {
    let mut lab = document()?;
    let problems = eehrxf::document::slot("sectionProblems").ok_or("the problems slot")?;
    assert!(matches!(
        lab.with_section(Section::new(problems, Vec::new())),
        Err(DocumentError::Foreign {
            slice: "sectionProblems"
        })
    ));
    let mut summary = Document::new(
        BASE,
        IDENTIFIER.to_owned(),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Patient summary"),
    );
    assert!(matches!(
        summary.with_report(report("Observation/glucose")),
        Err(DocumentError::Report {
            expected: "holds no"
        })
    ));
    Ok(())
}
