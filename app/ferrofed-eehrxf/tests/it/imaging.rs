// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging report's stored query, under its reserved name, and the
//! report the gateway writes from one member's imaging composition through
//! the shipped imaging project, compiled against FerroBRIDGE's vendored ADL
//! 2 imaging report template.
#![expect(
    clippy::disallowed_types,
    reason = "a test reads the document it wrote as JSON"
)]

use std::error::Error;

use eehrxf::crosswalk::imaging::{IMAGING_COMPOSITION, IMAGING_DIAGNOSTIC_REPORT, IMAGING_REPORT};
use eehrxf::document::imaging::IMAGING_BUNDLE;
use ferrofed_eehrxf::imaging::{self, ARCHETYPE, ELEMENT, SLUG, Served};
use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_eehrxf::report::{Report, Writes as _};
use ferrofed_eehrxf::reserved::{self, NAMESPACE, VERSION};
use ferrofed_eehrxf::section_query::{self, ISSUER, PATIENT};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_identity::role::header::PatientHeader;
use ferrofed_testkit::imaging as fixture;
use openehr_query::parser;
use serde_json::Value;

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
    let written = fixture::template()?;
    let template = Template::Adl2 {
        template: &written.template,
        archetypes: &written.archetypes,
    };
    let served = Served::compile(template)?;
    let composition = fixture::composition(&template.source()?, UID)?;
    let organisation = |name: &str| Organisation {
        id: name.to_lowercase().replace(' ', "-"),
        name: Some(name.to_owned()),
        identifiers: Vec::new(),
    };
    let (operator, member) = (
        organisation("Synthetic operator"),
        organisation("Synthetic member"),
    );
    let bundle = served.document(
        &composition,
        Report {
            base: "https://gateway.example.org/fhir",
            identifier: "urn:uuid:5b0c3f7e-0000-4000-8000-00000000e101",
            timestamp: "2026-10-21T08:00:00Z",
            patient: ("urn:oid:2.999.1", "synthetic-patient-0001"),
            header: &PatientHeader::default(),
            operator: &operator,
            member: &member,
        },
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
