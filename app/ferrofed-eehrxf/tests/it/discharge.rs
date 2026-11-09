// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report's stored query, under its reserved name, and the
//! report the gateway writes from one member's discharge composition through
//! the shipped HDR project, compiled against FerroBRIDGE's vendored ADL 2
//! discharge report template, held to `bundle-eu-hdr` over the HDR 1.0.0
//! package closure by FerroBRIDGE's validator.
#![expect(
    clippy::disallowed_types,
    reason = "a test reads the document it wrote as JSON"
)]

use std::error::Error;

use eehrxf::crosswalk::discharge::{DISCHARGE, HDR_COMPOSITION};
use eehrxf::document::discharge::{HDR_BUNDLE, HDR_ENCOUNTER};
use ferrofed_eehrxf::discharge::{self, COMPOSITION, ELEMENT, SLUG, Served};
use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_eehrxf::report::{Report, ServedError, Writes as _};
use ferrofed_eehrxf::reserved::{self, NAMESPACE, SAVED, VERSION};
use ferrofed_eehrxf::section_query::{self, ISSUER, PATIENT};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_identity::role::header::{PatientHeader, PersonName};
use ferrofed_testkit::discharge as fixture;
use ferrofed_testkit::fhir_packages;
use openehr_query::parser;

use crate::conforming;
use serde_json::Value;

type TestResult = Result<(), Box<dyn Error>>;

/// The version id of the synthetic composition.
const UID: &str = "5b0c3f7e-0000-4000-8000-00000000d101::cdr-a.example.org::1";

#[test]
fn the_query_parses_back_to_the_tree_it_was_printed_from() -> TestResult {
    assert_eq!(
        section_query::composed(COMPOSITION),
        parser::parse_str(&discharge::aql())?
    );
    Ok(())
}

#[test]
fn the_query_selects_compositions_of_the_transfer_summary_by_parameter_alone() {
    let aql = discharge::aql();
    assert!(
        aql.contains(&format!("COMPOSITION c[{COMPOSITION}]")),
        "{aql}"
    );
    assert!(aql.contains(&format!("${PATIENT}")), "{aql}");
    assert!(aql.contains(&format!("${ISSUER}")), "{aql}");
    assert!(!aql.contains('\''), "no literal: {aql}");
    assert!(aql.starts_with("SELECT DISTINCT"), "{aql}");
}

#[test]
fn the_template_specialises_the_queried_composition_archetype() -> TestResult {
    let template = std::fs::read_to_string(fixture::template_path())?;
    assert!(
        template.contains(&format!("specialize\n    {COMPOSITION}\n")),
        "the template specialises {COMPOSITION}"
    );
    Ok(())
}

#[test]
fn the_query_sits_under_its_reserved_name_at_the_immutable_version() -> TestResult {
    let definition = discharge::definition()?;
    assert_eq!(
        format!("{NAMESPACE}::{SLUG}"),
        definition.name().to_string()
    );
    assert!(reserved::reserves(definition.name()));
    assert_eq!(VERSION, definition.version());
    assert_eq!(SAVED, definition.saved());
    assert_eq!(discharge::aql(), definition.aql());
    assert!(
        reserved::definitions()?.contains(&definition),
        "the registry holds it"
    );
    Ok(())
}

#[test]
fn the_query_feeds_a_row_of_the_discharge_report_crosswalk() {
    assert!(DISCHARGE.row(ELEMENT).is_some(), "{ELEMENT}");
}

/// The shipped HDR project compiled against the vendored template.
fn served() -> Result<Served, Box<dyn Error>> {
    let written = fixture::template()?;
    Ok(Served::compile(Template::Adl2 {
        template: &written.template,
        archetypes: &written.archetypes,
    })?)
}

/// A synthetic organisation named `name`.
fn organisation(name: &str) -> Organisation {
    Organisation {
        id: name.to_lowercase().replace(' ', "-"),
        name: Some(name.to_owned()),
        identifiers: Vec::new(),
    }
}

/// The synthetic family name the identity binding holds.
const FAMILY: &str = "SENTINEL-FAMILY-d1h7";

/// The synthetic birth date the identity binding holds.
const BIRTH_DATE: &str = "1970-01-01";

/// The `data-absent-reason` extension.
const DATA_ABSENT_REASON: &str = "http://hl7.org/fhir/StructureDefinition/data-absent-reason";

/// The header the identity binding holds of the synthetic patient: a family
/// name and a birth date.
fn header() -> PatientHeader {
    PatientHeader {
        names: vec![PersonName {
            family: Some(FAMILY.to_owned()),
            ..PersonName::default()
        }],
        birth_date: Some(BIRTH_DATE.to_owned()),
        ..PatientHeader::default()
    }
}

/// The document the gateway writes from the synthetic composition, as JSON.
fn written() -> Result<Value, Box<dyn Error>> {
    written_with(&header())
}

/// The document the gateway writes from the synthetic composition about the
/// patient the identity binding describes with `header`, as JSON.
fn written_with(header: &PatientHeader) -> Result<Value, Box<dyn Error>> {
    let bundle = conforming::written(
        &served()?,
        (
            &fixture::composition(UID),
            "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d101",
        ),
        header,
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
fn a_discharge_composition_is_written_as_a_hospital_discharge_report() -> TestResult {
    let bundle = written()?;
    assert_eq!(bundle["meta"]["profile"][0], HDR_BUNDLE);
    assert_eq!(bundle["type"], "document");
    assert_eq!(bundle["language"], "en");
    let [composition] = of_type(&bundle, "Composition")[..] else {
        return Err("one Composition".into());
    };
    assert_eq!(composition["meta"]["profile"][0], HDR_COMPOSITION);
    assert_eq!(composition["type"]["coding"][0]["code"], "34105-7");
    let [encounter] = of_type(&bundle, "Encounter")[..] else {
        return Err("one Encounter".into());
    };
    assert_eq!(encounter["meta"]["profile"][0], HDR_ENCOUNTER);
    assert_eq!(encounter["period"]["start"], fixture::ADMITTED);
    assert_eq!(encounter["period"]["end"], fixture::DISCHARGED);
    let codes: Vec<&str> = composition["section"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|section| section["code"]["coding"][0]["code"].as_str())
        .collect();
    for code in ["8648-8", "11535-2", "10185-7", "75311-1"] {
        assert!(codes.contains(&code), "{code} in {codes:?}");
    }
    let condition = of_type(&bundle, "Condition");
    assert_eq!(condition.len(), 1);
    assert_eq!(
        condition[0]["code"]["coding"][0]["code"],
        fixture::DIAGNOSIS
    );
    assert_eq!(of_type(&bundle, "Procedure").len(), 1);
    assert_eq!(of_type(&bundle, "MedicationRequest").len(), 1);
    Ok(())
}

#[test]
fn the_patient_is_named_by_identifier_and_header_and_the_member_by_provenance() -> TestResult {
    let bundle = written()?;
    let [patient] = of_type(&bundle, "Patient")[..] else {
        return Err("one Patient".into());
    };
    assert_eq!(patient["identifier"][0]["value"], "synthetic-patient-0001");
    assert_eq!(patient["name"][0]["family"], FAMILY, "{patient}");
    assert_eq!(patient["birthDate"], BIRTH_DATE, "{patient}");
    let [provenance] = of_type(&bundle, "Provenance")
        .into_iter()
        .filter(|provenance| provenance["agent"][0]["who"]["reference"].is_string())
        .filter(|provenance| {
            provenance["target"]
                .as_array()
                .is_some_and(|targets| targets.len() == 3)
        })
        .collect::<Vec<_>>()[..]
    else {
        return Err("one Provenance naming the member for the three entries".into());
    };
    let member = provenance["agent"][0]["who"]["reference"]
        .as_str()
        .ok_or("the member's reference")?;
    assert!(
        of_type(&bundle, "Organization")
            .iter()
            .any(|organization| organization["name"] == "Synthetic member"
                && member.ends_with(organization["id"].as_str().unwrap_or("-"))),
        "{member}"
    );
    let [composition] = of_type(&bundle, "Composition")[..] else {
        return Err("one Composition".into());
    };
    assert!(
        composition["author"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|author| author["reference"] == member),
        "documents.html §3.3.1: the Composition references the member: {composition}"
    );
    Ok(())
}

#[test]
fn a_name_and_birth_date_the_binding_does_not_hold_are_written_absent() -> TestResult {
    let bundle = written_with(&PatientHeader::default())?;
    let [patient] = of_type(&bundle, "Patient")[..] else {
        return Err("one Patient".into());
    };
    let [name] = patient["name"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    else {
        return Err(format!("one name: {patient}").into());
    };
    assert_eq!(name["extension"][0]["url"], DATA_ABSENT_REASON, "{patient}");
    assert_eq!(name["extension"][0]["valueCode"], "unknown", "{patient}");
    assert!(name.get("family").is_none(), "nothing filled in: {patient}");
    assert!(
        patient.get("birthDate").is_none(),
        "nothing filled in: {patient}"
    );
    assert_eq!(
        patient["_birthDate"]["extension"][0]["url"], DATA_ABSENT_REASON,
        "{patient}"
    );
    Ok(())
}

/// Every error FerroBRIDGE's validator finds in `document` against
/// `bundle-eu-hdr`, over the HDR 1.0.0 package and its dependency closure.
async fn breaks(document: &Value) -> Result<Vec<String>, Box<dyn Error>> {
    conforming::breaks(
        &document.to_string(),
        HDR_BUNDLE,
        (fixture::package(), fhir_packages::discharge_dependencies()?),
    )
    .await
}

#[tokio::test]
async fn a_written_discharge_report_conforms_to_bundle_eu_hdr() -> TestResult {
    let breaks = breaks(&written()?).await?;
    assert!(breaks.is_empty(), "{breaks:#?}");
    Ok(())
}

#[tokio::test]
async fn a_report_of_a_patient_the_binding_holds_nothing_of_conforms_to_bundle_eu_hdr() -> TestResult
{
    let breaks = breaks(&written_with(&PatientHeader::default())?).await?;
    assert!(breaks.is_empty(), "{breaks:#?}");
    Ok(())
}

#[test]
fn a_composition_with_no_language_is_refused() -> TestResult {
    let composition = fixture::composition(UID).replace(r#""language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
  "territory""#, r#""territory""#);
    let (operator, member) = (organisation("o"), organisation("m"));
    let refused = served()?.document(
        &composition,
        Report {
            base: "https://gateway.example.org/fhir",
            identifier: "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d102",
            timestamp: "2026-10-21T08:00:00Z",
            patient: ("urn:oid:2.999.1", "synthetic-patient-0001"),
            header: &header(),
            operator: &operator,
            member: &member,
        },
    );
    assert!(
        matches!(refused, Err(ServedError::Language(_))),
        "{refused:?}"
    );
    Ok(())
}
