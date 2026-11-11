// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging report's stored query, under its reserved name, and the
//! report the gateway writes from one member's imaging composition through
//! the shipped imaging project, compiled against FerroBRIDGE's vendored ADL
//! 2 imaging report template, held to `BundleReportEuImaging` over the
//! Imaging Report 1.0.0-ballot package closure by FerroBRIDGE's validator.
#![expect(
    clippy::disallowed_types,
    reason = "a test reads the document it wrote as JSON"
)]

use std::error::Error;

use eehrxf::crosswalk::imaging::{IMAGING_COMPOSITION, IMAGING_DIAGNOSTIC_REPORT, IMAGING_REPORT};
use eehrxf::document::imaging::IMAGING_BUNDLE;
use ferrofed_eehrxf::imaging::{self, ARCHETYPE, ELEMENT, SLUG, Served};
use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_eehrxf::reserved::{self, NAMESPACE, VERSION};
use ferrofed_eehrxf::section_query::{self, ISSUER, PATIENT};
use ferrofed_identity::role::header::PatientHeader;
use ferrofed_testkit::{fhir_packages, imaging as fixture};
use openehr_query::parser;
use serde_json::Value;

use crate::conforming;

type TestResult = Result<(), Box<dyn Error>>;

/// The version id of the synthetic composition.
const UID: &str = "5b0c3f7e-0000-4000-8000-00000000e101::cdr-a.example.org::1";

#[test]
fn the_query_selects_compositions_holding_the_imaging_result_by_parameter_alone() -> TestResult {
    let aql = imaging::aql();
    assert_eq!(
        section_query::containing(&ARCHETYPE),
        parser::parse_str(&aql)?
    );
    assert!(aql.contains(ARCHETYPE.id), "{aql}");
    assert!(aql.contains(&format!("${PATIENT}")), "{aql}");
    assert!(aql.contains(&format!("${ISSUER}")), "{aql}");
    assert!(!aql.contains('\''), "no literal: {aql}");
    let template = std::fs::read_to_string(fixture::template_path())?;
    assert!(template.contains(ARCHETYPE.id), "the template holds it");
    Ok(())
}

#[test]
fn the_query_sits_under_its_reserved_name_at_the_immutable_version() -> TestResult {
    let definition = imaging::definition()?;
    assert_eq!(
        format!("{NAMESPACE}::{SLUG}"),
        definition.name().to_string()
    );
    assert!(reserved::reserves(definition.name()));
    assert_eq!(VERSION, definition.version());
    assert!(reserved::definitions()?.contains(&definition));
    assert!(IMAGING_REPORT.row(ELEMENT).is_some(), "{ELEMENT}");
    Ok(())
}

/// The document the gateway writes from the synthetic composition, as JSON.
fn written() -> Result<Value, Box<dyn Error>> {
    written_with(|_| Ok(()))
}

/// The imaging findings the synthetic member records beside its report,
/// which the imaging mapping declares and never carries.
const FINDING: &str = "SENTINEL-FINDING-i8f2";

/// The document the gateway writes from the synthetic composition after
/// `edit` changes it, as JSON.
fn written_with(
    edit: impl FnOnce(&mut Value) -> Result<(), Box<dyn Error>>,
) -> Result<Value, Box<dyn Error>> {
    let written = fixture::template()?;
    let template = Template::Adl2 {
        template: &written.template,
        archetypes: &written.archetypes,
    };
    let served = Served::compile(template)?;
    let mut held: Value = serde_json::from_str(&fixture::composition(&template.source()?, UID)?)?;
    edit(&mut held)?;
    let composition = held.to_string();
    let bundle = conforming::written(
        &served,
        (
            &composition,
            "urn:uuid:5b0c3f7e-0000-4000-8000-00000000e101",
        ),
        &PatientHeader::default(),
    )?;
    Ok(serde_json::to_value(&bundle)?)
}

/// The resources of `bundle` of the type `kind`.
fn of_type<'a>(bundle: &'a Value, kind: &str) -> Vec<&'a Value> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind)
        .collect()
}

#[test]
fn an_imaging_composition_is_written_as_an_imaging_report() -> TestResult {
    let bundle = written()?;
    assert_eq!(bundle["meta"]["profile"][0], IMAGING_BUNDLE);
    let [composition] = of_type(&bundle, "Composition")[..] else {
        return Err("one Composition".into());
    };
    assert_eq!(composition["meta"]["profile"][0], IMAGING_COMPOSITION);
    let [report] = of_type(&bundle, "DiagnosticReport")[..] else {
        return Err("one DiagnosticReport".into());
    };
    assert_eq!(report["meta"]["profile"][0], IMAGING_DIAGNOSTIC_REPORT);
    assert_eq!(report["conclusion"], fixture::CONCLUSION);
    let [study] = of_type(&bundle, "ImagingStudy")[..] else {
        return Err("one ImagingStudy".into());
    };
    assert_eq!(study["identifier"][0]["value"], fixture::STUDY_UID);
    let sections = composition["section"].as_array().ok_or("sections")?;
    assert_eq!(4, sections.len(), "the four sections the profile requires");
    for section in sections {
        let empty = section["entry"].as_array().is_none_or(Vec::is_empty);
        assert_eq!(empty, section["emptyReason"].is_object(), "{section}");
    }
    assert_eq!(sections[0]["code"]["coding"][0]["code"], "18726-0");
    assert!(sections[0]["entry"][0]["reference"].is_string());
    Ok(())
}

#[test]
fn the_member_is_named_by_provenance_and_the_patient_by_identifier() -> TestResult {
    let bundle = written()?;
    let [patient] = of_type(&bundle, "Patient")[..] else {
        return Err("one Patient".into());
    };
    assert_eq!(patient["identifier"][0]["value"], "synthetic-patient-0001");
    let named = of_type(&bundle, "Provenance")
        .into_iter()
        .any(|provenance| {
            provenance["target"]
                .as_array()
                .is_some_and(|targets| !targets.is_empty())
                && provenance["agent"][0]["who"]["reference"]
                    .as_str()
                    .is_some_and(|who| who.contains("member"))
        });
    assert!(named, "a Provenance names the member");
    Ok(())
}

#[test]
fn the_composition_names_the_member_its_custodian() -> TestResult {
    let bundle = written()?;
    let custodian = conforming::custodian(&bundle)?;
    assert_eq!(custodian["resourceType"], "Organization", "{custodian}");
    assert_eq!(custodian["name"], conforming::MEMBER, "{custodian}");
    Ok(())
}

#[tokio::test]
async fn a_written_imaging_report_conforms_to_bundle_report_eu_imaging() -> TestResult {
    let breaks = conforming::breaks(
        &written()?.to_string(),
        IMAGING_BUNDLE,
        (
            fhir_packages::imaging(),
            fhir_packages::imaging_dependencies()?,
        ),
    )
    .await?;
    assert!(breaks.is_empty(), "{breaks:#?}");
    Ok(())
}

/// Records [`FINDING`] as the imaging findings of the composition's one
/// observation (`ELEMENT[at0008]` of `openEHR-EHR-OBSERVATION.imaging_exam_result.v1`).
fn with_finding(composition: &mut Value) -> Result<(), Box<dyn Error>> {
    let items = composition
        .pointer_mut("/content/0/data/events/0/data/items")
        .and_then(Value::as_array_mut)
        .ok_or("the observation's items")?;
    items.push(serde_json::json!({
        "_type": "ELEMENT",
        "archetype_node_id": "at0008",
        "name": {"_type": "DV_TEXT", "value": "Imaging findings"},
        "value": {"_type": "DV_TEXT", "value": FINDING}
    }));
    Ok(())
}

/// The narrative of the imaging study section of `bundle`.
fn study_narrative(bundle: &Value) -> Result<String, Box<dyn Error>> {
    let [composition] = of_type(bundle, "Composition")[..] else {
        return Err("one Composition".into());
    };
    let section = composition
        .get("section")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|section| {
            section
                .pointer("/code/coding/0/code")
                .and_then(Value::as_str)
                == Some("18726-0")
        })
        .ok_or("the imaging study section")?;
    Ok(section
        .pointer("/text/div")
        .and_then(Value::as_str)
        .ok_or("the section's narrative")?
        .to_owned())
}

// NOTE: hazard I-20 of the clinical safety risk file: a finding the mapping does not carry is
// named in the report by its path, never by its value, so the reader knows the record holds more.
#[test]
fn a_finding_the_mapping_leaves_out_is_named_in_the_report_by_path() -> TestResult {
    let bundle = written_with(with_finding)?;
    let narrative = study_narrative(&bundle)?;
    assert!(
        narrative.contains("could not be carried into this report")
            && narrative.contains("items[at0008]"),
        "the finding is named by its openEHR path: {narrative}"
    );
    assert!(
        !bundle.to_string().contains(FINDING),
        "no value of the finding reaches the report: {narrative}"
    );
    let plain = study_narrative(&written()?)?;
    assert!(
        !plain.contains("could not be carried"),
        "a report that loses nothing says nothing: {plain}"
    );
    Ok(())
}

#[tokio::test]
async fn a_report_naming_a_lost_finding_conforms_to_bundle_report_eu_imaging() -> TestResult {
    let breaks = conforming::breaks(
        &written_with(with_finding)?.to_string(),
        IMAGING_BUNDLE,
        (
            fhir_packages::imaging(),
            fhir_packages::imaging_dependencies()?,
        ),
    )
    .await?;
    assert!(breaks.is_empty(), "{breaks:#?}");
    Ok(())
}
