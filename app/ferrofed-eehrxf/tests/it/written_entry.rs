// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The list entry of a report the gateway writes on read: the description
//! the European interoperability component writes from the member's
//! composition, held to HL7 Europe EU Health Data API
//! `EehrxfMhdDocumentReference` and MyHealth@EU `DocumentReference-minimal`
//! (`myhealth.eu.fhir.ncp-api` 9.1.1-ci) by FerroBRIDGE's validator.

use std::error::Error;
use std::path::PathBuf;

use eehrxf::document::metadata::DOCUMENT_REFERENCE_VERSION;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::validate::Validator;
use ferrofed_eehrxf::list::Listing;
use ferrofed_eehrxf::list::written::{self, Undescribed, Writing};
use ferrofed_eehrxf::report::Written;
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_testkit::fhir_packages;
use fhir_types::codec::{Json as _, Value};
use fhir_types::r4::document_reference::DocumentReference;
use serde_json::json;

/// The canonical URL of MyHealth@EU `DocumentReference-minimal`.
const MINIMAL: &str =
    "http://fhir.ehdsi.eu/ncp-api/StructureDefinition/DocumentReference-minimal-myhealtheu";

/// The canonical URL of HL7 Europe `EehrxfMhdDocumentReference`.
const EEHRXF_MHD: &str =
    "http://hl7.eu/fhir/health-data-api/StructureDefinition/EehrxfMhdDocumentReference";

/// A synthetic organisation named `name`.
fn organisation(name: &str) -> Organisation {
    Organisation {
        id: name.to_lowercase().replace(' ', "-"),
        name: Some(name.to_owned()),
        identifiers: vec![("urn:oid:2.999.7".to_owned(), name.to_lowercase())],
    }
}

/// The entry of a synthetic composition of the category `written`.
fn entry(written: Written, composition: &str) -> Result<DocumentReference, Undescribed> {
    let (operator, custodian) = (
        organisation("Synthetic operator"),
        organisation("Synthetic member"),
    );
    written::entry(
        Writing {
            written,
            composition,
            operator: &operator,
            custodian: &custodian,
        },
        (
            "synthetic-entry",
            "urn:uuid:7c4e1d20-0000-4000-8000-0000000008b1",
        ),
        Listing {
            system: "urn:oid:2.999.1",
            value: "synthetic-patient-0001",
            patient: "synthetic-patient",
            date: "2026-11-25T08:00:00Z",
        },
        "Binary/synthetic-token",
    )
}

/// A synthetic composition at version 2, in Dutch of the Netherlands, of
/// one service period.
fn composition() -> String {
    json!({
        "_type": "COMPOSITION",
        "uid": {"value": "7c4e1d20-0000-4000-8000-0000000008b1::cdr-a.example.org::2"},
        "language": {"code_string": "nl"},
        "territory": {"code_string": "NL"},
        "context": {
            "start_time": {"value": "2026-11-20T09:00:00Z"},
            "end_time": {"value": "2026-11-20T11:00:00Z"}
        }
    })
    .to_string()
}

/// Every error or fatal issue FerroBRIDGE's validator finds in `reference`
/// against `profile`, over `packages`.
fn breaks(
    reference: &DocumentReference,
    profile: &str,
    packages: Vec<PathBuf>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let set: PackageSet = PackageSet::load(packages)?;
    let report =
        Validator::new(&set).validate_against(&Value::Object(reference.to_json()?), profile)?;
    Ok(report
        .issues()
        .iter()
        .filter(|issue| matches!(issue.severity, Severity::Error | Severity::Fatal))
        .map(|issue| format!("{}: {}", issue.expression, issue.message))
        .collect())
}

/// The MyHealth@EU `NCPeH` API package and its dependency closure.
fn ncp_api() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut packages = vec![fhir_packages::ncp_api()];
    packages.extend(fhir_packages::ncp_api_dependencies()?);
    Ok(packages)
}

/// The EU Health Data API package and its dependency closure.
fn health_data_api() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    Ok(fhir_packages::imaging_dependencies()?)
}

#[test]
fn each_written_entry_is_the_components_description_of_its_composition()
-> Result<(), Box<dyn Error>> {
    for (written, code) in [
        (Written::Laboratory, "11502-2"),
        (Written::Discharge, "34105-7"),
        (Written::Imaging, "18748-4"),
    ] {
        let reference = serde_json::to_value(entry(written, &composition())?.to_json()?)?;
        assert_eq!(reference["meta"]["profile"][0], EEHRXF_MHD);
        assert_eq!(
            reference["masterIdentifier"],
            json!({
                "system": "urn:ietf:rfc:3986",
                "type": {"coding": [{
                    "system": "https://profiles.ihe.net/ITI/MHD/CodeSystem/IHE.MHD.MHDIdentifierType",
                    "code": "uniqueId"
                }]},
                "use": "usual",
                "value": "urn:uuid:7c4e1d20-0000-4000-8000-0000000008b1"
            })
        );
        assert_eq!(reference["type"]["coding"][0]["code"], code);
        assert_eq!(
            reference["subject"]["reference"],
            "Patient/synthetic-patient"
        );
        assert_eq!(reference["date"], "2026-11-25T08:00:00Z");
        assert!(reference["description"].is_string());
        // Annex III Table 1: version, language and period come from the composition.
        assert_eq!(
            reference["extension"][0],
            json!({"url": DOCUMENT_REFERENCE_VERSION, "valueString": "2"})
        );
        assert_eq!(reference["content"][0]["attachment"]["language"], "nl-NL");
        assert_eq!(
            reference["content"][0]["attachment"]["url"],
            "Binary/synthetic-token"
        );
        assert_eq!(
            reference["context"]["period"],
            json!({"start": "2026-11-20T09:00:00Z", "end": "2026-11-20T11:00:00Z"})
        );
        // One category: the document category and the priority category beside it.
        let codings = reference["category"][0]["coding"]
            .as_array()
            .ok_or("one category with codings")?;
        assert_eq!(reference["category"].as_array().map(Vec::len), Some(1));
        assert_eq!(codings.len(), 2, "{codings:?}");
        assert_eq!(codings[1]["code"], written.category());
        assert_eq!(reference["content"][0]["format"]["code"], written.bundle());
        assert_eq!(reference["custodian"]["display"], "Synthetic member");
    }
    Ok(())
}

#[test]
fn a_written_entry_validates_against_both_guides() -> Result<(), Box<dyn Error>> {
    for written in [Written::Laboratory, Written::Discharge, Written::Imaging] {
        let reference = entry(written, &composition())?;
        let mhd = breaks(&reference, EEHRXF_MHD, health_data_api()?)?;
        assert!(mhd.is_empty(), "{written:?}: {mhd:#?}");
        let minimal = breaks(&reference, MINIMAL, ncp_api()?)?;
        assert!(minimal.is_empty(), "{written:?}: {minimal:#?}");
    }
    Ok(())
}

#[test]
fn a_composition_with_no_version_gives_no_entry() {
    let unversioned =
        json!({"_type": "COMPOSITION", "language": {"code_string": "en"}}).to_string();
    assert!(matches!(
        entry(Written::Laboratory, &unversioned),
        Err(Undescribed::Document(_))
    ));
    assert!(matches!(
        entry(Written::Laboratory, "not json"),
        Err(Undescribed::Json(_))
    ));
}
