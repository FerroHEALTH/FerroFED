// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The priority category codes held to the vendored packages they are read
//! from: HL7 Europe's `EEHRxFDocumentPriorityCategoryCS`
//! (`hl7.fhir.eu.health-data-api` 1.0.0-ballot), the imaging report guide
//! that requires one of its codes, and the discharge report guide and the
//! MyHealth@EU Master Value Sets Catalogue, which code no category.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::io::Read as _;

use ehds_logging::category::{Category, PRIORITY_SYSTEM, PRIORITY_VERSION};
use flate2::read::GzDecoder;

/// The bytes of `member` in the vendored package `package`, under
/// `docs/specs/`, or `None` when the package holds no such member.
fn member(package: &str, member: &str) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs")
        .join(package);
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(path)?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.as_os_str() == member {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

/// Whether any member of the vendored package `package` names the priority
/// category code system.
fn names_the_code_system(package: &str) -> Result<bool, Box<dyn Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs")
        .join(package);
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(path)?));
    for entry in archive.entries()? {
        let mut bytes = Vec::new();
        entry?.read_to_end(&mut bytes)?;
        if bytes
            .windows(PRIORITY_SYSTEM.len())
            .any(|window| window == PRIORITY_SYSTEM.as_bytes())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The parts of a FHIR R4 `CodeSystem` the codes are held to.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeSystem {
    url: String,
    version: String,
    case_sensitive: bool,
    concept: Vec<Concept>,
}

/// One concept of a [`CodeSystem`].
#[derive(serde::Deserialize)]
struct Concept {
    code: String,
    display: String,
}

/// The differential of a FHIR R4 `StructureDefinition`.
#[derive(serde::Deserialize)]
struct StructureDefinition {
    differential: Differential,
}

/// The elements of a [`StructureDefinition`]'s differential.
#[derive(serde::Deserialize)]
struct Differential {
    element: Vec<Element>,
}

/// One element, with the pattern it fixes.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Element {
    id: String,
    min: Option<u32>,
    pattern_codeable_concept: Option<CodeableConcept>,
}

/// A FHIR R4 `CodeableConcept`.
#[derive(serde::Deserialize)]
struct CodeableConcept {
    coding: Vec<Coding>,
}

/// A FHIR R4 `Coding`.
#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Coding {
    system: String,
    code: String,
}

#[test]
fn the_priority_codes_are_the_ones_the_vendored_code_system_defines() -> Result<(), Box<dyn Error>>
{
    let bytes = member(
        "eu-hl7-health-data-api/hl7.fhir.eu.health-data-api-1.0.0-ballot.tgz",
        "package/CodeSystem-eehrxf-document-priority-category-cs.json",
    )?
    .ok_or("the package defines eehrxf-document-priority-category-cs")?;
    let system: CodeSystem = serde_json::from_slice(&bytes)?;
    assert_eq!(system.url, PRIORITY_SYSTEM);
    assert_eq!(system.version, PRIORITY_VERSION);
    assert!(system.case_sensitive, "the codes are compared exactly");
    let codes: Vec<&str> = system
        .concept
        .iter()
        .map(|concept| concept.code.as_str())
        .collect();
    let written: Vec<&str> = Category::PRIORITY.iter().map(Category::code).collect();
    assert_eq!(codes, written);
    let displays: Vec<&str> = system
        .concept
        .iter()
        .map(|concept| concept.display.as_str())
        .collect();
    assert_eq!(
        displays,
        [
            "patient summaries",
            "electronic prescriptions",
            "electronic dispensations",
            "medical imaging studies and related imaging reports",
            "medical test results, including laboratory and other diagnostic results and related reports",
            "discharge reports"
        ],
        "the displays are the Art 14(1) terms, (a) to (f) in order"
    );
    Ok(())
}

#[test]
fn the_imaging_report_requires_the_code_the_record_writes_for_imaging() -> Result<(), Box<dyn Error>>
{
    for (profile, slice) in [
        (
            "package/StructureDefinition-CompositionEuImaging.json",
            "Composition.category:imaging",
        ),
        (
            "package/StructureDefinition-DiagnosticReportEuImaging.json",
            "DiagnosticReport.category:imaging",
        ),
    ] {
        let bytes = member(
            "eu-hl7-imaging/hl7.fhir.eu.imaging-1.0.0-ballot.tgz",
            profile,
        )?
        .ok_or(profile)?;
        let definition: StructureDefinition = serde_json::from_slice(&bytes)?;
        let element = definition
            .differential
            .element
            .into_iter()
            .find(|element| element.id == slice)
            .ok_or(slice)?;
        assert_eq!(element.min, Some(1), "{slice} is required");
        let pattern = element.pattern_codeable_concept.ok_or(slice)?;
        assert_eq!(
            pattern.coding,
            [Coding {
                system: Category::MedicalImaging.system().to_owned(),
                code: Category::MedicalImaging.code().to_owned(),
            }],
            "{slice}"
        );
    }
    Ok(())
}

#[test]
fn the_discharge_report_and_the_master_value_sets_code_no_category() -> Result<(), Box<dyn Error>> {
    for package in [
        "eu-hl7-hdr/hl7.fhir.eu.hdr-0.1.0-ballot.tgz",
        "ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz",
    ] {
        assert!(!names_the_code_system(package)?, "{package}");
    }
    Ok(())
}
