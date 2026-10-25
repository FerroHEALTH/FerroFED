// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MyHealth@EU, eHDSI and Xt-EHR material the architecture's decisions on
//! the national contact point, the exchange format and the priority
//! categories cite, read from the vendored corpora: every committed package
//! is the package, version and licence its pin names, the dependencies and
//! sentences the decisions cite are in the pinned bytes, and the `OpenNCP`
//! coded element list binds the Master Value Sets Catalogue (#1290). A re-pin
//! that moves one fails here, so a citation never points at text that is not
//! there.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the vendored package manifests and resources are read as JSON"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use serde_json::Value;

type TestResult = Result<(), Box<dyn Error>>;

/// The path of `path` under the repository's `docs/specs/`.
fn vendored(path: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/specs")).join(path)
}

/// The text of `path` under `docs/specs/`.
fn text(path: &str) -> Result<String, Box<dyn Error>> {
    let file = vendored(path);
    Ok(std::fs::read_to_string(&file).map_err(|error| format!("{}: {error}", file.display()))?)
}

/// Every JSON member of the package tarball `path` whose name `wanted`
/// accepts, by member name.
fn members(
    path: &str,
    wanted: impl Fn(&str) -> bool,
) -> Result<BTreeMap<String, Value>, Box<dyn Error>> {
    let file = vendored(path);
    let opened =
        std::fs::File::open(&file).map_err(|error| format!("{}: {error}", file.display()))?;
    let mut archive = tar::Archive::new(GzDecoder::new(opened));
    let mut found = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        if !wanted(&name) {
            continue;
        }
        let mut body = String::new();
        entry.read_to_string(&mut body)?;
        found.insert(name, serde_json::from_str(&body)?);
    }
    Ok(found)
}

/// Whether the member name `name` ends in a `.json` extension, any case.
fn is_json(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

/// The package manifest of the package tarball `path`.
fn manifest(path: &str) -> Result<Value, Box<dyn Error>> {
    members(path, |name| name == "package/package.json")?
        .into_values()
        .next()
        .ok_or_else(|| format!("{path} has no package/package.json").into())
}

/// The string member `key` of `value`, empty when it has none.
fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The packages #1290 vendors or re-pins, and those #1306 vendors for their
/// dependency closure, grouped by the licence their pin records and the FHIR
/// version they declare. Each file is named `<name>-<version>.tgz` after
/// the package its pin names.
const PINNED: &[(&str, &str, &[&str])] = &[
    (
        "CC0-1.0",
        "4.0.1",
        &[
            "ehdsi/myhealth.eu.fhir.ncp-api-9.1.1-ci.tgz",
            "ehdsi-core/myhealth.eu.fhir.core-1.0.0.tgz",
            "ehdsi-laboratory/myhealth.eu.fhir.laboratory-9.1.1.tgz",
            "ehdsi-hdr/myhealth.eu.fhir.hdr-9.1.1.tgz",
            "ehdsi-imaging/myhealth.eu.fhir.imaging-9.1.1.tgz",
            "eu-hl7-laboratory/hl7.fhir.eu.laboratory-2.1.0.tgz",
            "eu-hl7-hdr/hl7.fhir.eu.hdr-1.0.0.tgz",
            "eu-hl7-imaging/hl7.fhir.eu.imaging-1.0.0-alpha.tgz",
            "eu-hl7-extensions/hl7.fhir.eu.extensions-0.1.0.tgz",
            "hl7-ips/hl7.fhir.uv.ips-2.0.1.tgz",
        ],
    ),
    (
        "CC-BY-4.0",
        "4.0.1",
        &[
            "ehdsi-eps/myhealth.eu.fhir.eps-1.0.0-poc.tgz",
            "ehdsi-epd/myhealth.eu.fhir.epd-1.0.0-poc.tgz",
            "ehdsi-euvac/myhealth.eu.fhir.euvac-10.0.0-preview.tgz",
            "ihe-mhd/ihe.iti.mhd-4.2.2.tgz",
        ],
    ),
    (
        "CC0-1.0",
        "5.0.0",
        &[
            "eu-xtehr-models/xtehr.eu.ehds.models-0.3.0.tgz",
            "eu-xtehr-metadata/hl7.eu.fhir.xt-ehr.metadata-0.1.0.tgz",
        ],
    ),
];

/// The packages whose manifests must name only packages the repository
/// holds: the MyHealth@EU guides and the two HL7 Europe re-pins of #1290.
const CLOSURE_ROOTS: &[&str] = &[
    "ehdsi/myhealth.eu.fhir.ncp-api-9.1.1-ci.tgz",
    "ehdsi-core/myhealth.eu.fhir.core-1.0.0.tgz",
    "ehdsi-eps/myhealth.eu.fhir.eps-1.0.0-poc.tgz",
    "ehdsi-epd/myhealth.eu.fhir.epd-1.0.0-poc.tgz",
    "ehdsi-laboratory/myhealth.eu.fhir.laboratory-9.1.1.tgz",
    "ehdsi-hdr/myhealth.eu.fhir.hdr-9.1.1.tgz",
    "ehdsi-imaging/myhealth.eu.fhir.imaging-9.1.1.tgz",
    "ehdsi-euvac/myhealth.eu.fhir.euvac-10.0.0-preview.tgz",
    "ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz",
    "eu-hl7-laboratory/hl7.fhir.eu.laboratory-2.1.0.tgz",
    "eu-hl7-hdr/hl7.fhir.eu.hdr-1.0.0.tgz",
];

/// The provenance files of the cache-only package corpora, whose `### `
/// headings name each cached tarball as `<name>-<version>.tgz`.
const CACHED_PACKAGES: &[&str] = &[
    "fhir-packages/PROVENANCE.md",
    "fhir-packages-myhealth-eu/PROVENANCE.md",
];

/// Every `<name>@<version>` of a package tarball committed one level under
/// `docs/specs/`, read from its manifest.
fn committed_packages() -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut found = BTreeSet::new();
    for corpus in std::fs::read_dir(vendored(""))? {
        let corpus = corpus?.path();
        if !corpus.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&corpus)? {
            let file = file?.path();
            if file.extension().and_then(|extension| extension.to_str()) != Some("tgz") {
                continue;
            }
            let Some(relative) = file
                .strip_prefix(vendored(""))
                .ok()
                .and_then(|relative| relative.to_str())
            else {
                continue;
            };
            let package = manifest(relative)?;
            found.insert(format!(
                "{}@{}",
                string(&package, "name"),
                string(&package, "version")
            ));
        }
    }
    Ok(found)
}

/// Every tarball file name a `### ` heading of the cache-only provenance
/// files names.
fn cached_packages() -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut found = BTreeSet::new();
    for provenance in CACHED_PACKAGES {
        for line in text(provenance)?.lines() {
            if let Some((file, _)) = line
                .strip_prefix("### `")
                .and_then(|rest| rest.split_once('`'))
            {
                found.insert(file.to_owned());
            }
        }
    }
    Ok(found)
}

#[test]
fn every_package_the_myhealth_guides_name_is_vendored() -> TestResult {
    let committed = committed_packages()?;
    let cached = cached_packages()?;
    let mut missing = Vec::new();
    for root in CLOSURE_ROOTS {
        let package = manifest(root)?;
        let dependencies = package
            .get("dependencies")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("{root} names no dependencies"))?;
        for (name, version) in dependencies {
            let version = version.as_str().unwrap_or_default();
            let held = committed.contains(&format!("{name}@{version}"))
                || cached.contains(&format!("{name}-{version}.tgz"));
            if !held {
                missing.push(format!("{root} names {name} {version}"));
            }
        }
    }
    assert!(missing.is_empty(), "packages no corpus holds: {missing:#?}");
    Ok(())
}

#[test]
fn each_committed_package_is_the_package_and_licence_its_pin_names() -> TestResult {
    let pinned = PINNED
        .iter()
        .flat_map(|&(licence, fhir, paths)| paths.iter().map(move |&path| (path, licence, fhir)));
    for (path, licence, fhir) in pinned {
        let file = path.rsplit('/').next().unwrap_or(path);
        let package = manifest(path)?;
        let named = format!(
            "{}-{}.tgz",
            string(&package, "name"),
            string(&package, "version")
        );
        assert_eq!(named, file, "the package its pin names: {path}");
        assert_eq!(string(&package, "license"), licence, "{path}");
        let declared: Vec<&str> = package
            .get("fhirVersions")
            .and_then(Value::as_array)
            .map(|versions| versions.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert_eq!(declared, [fhir], "{path}");

        let guides = members(path, |name| {
            name.starts_with("package/ImplementationGuide-") && is_json(name)
        })?;
        assert_eq!(guides.len(), 1, "{path} carries one ImplementationGuide");
        for guide in guides.values() {
            assert_eq!(string(guide, "license"), licence, "{path}");
        }
    }
    Ok(())
}

#[test]
fn the_myhealth_guides_derive_from_the_vendored_versions() -> TestResult {
    // (package, dependency, the version its package manifest names)
    let cited = [
        (
            "ehdsi-core/myhealth.eu.fhir.core-1.0.0.tgz",
            "xtehr.eu.ehds.models",
            "0.3.0",
        ),
        (
            "ehdsi-core/myhealth.eu.fhir.core-1.0.0.tgz",
            "myhealth.eu.fhir.mvc-package",
            "9.1.0",
        ),
        (
            "ehdsi-core/myhealth.eu.fhir.core-1.0.0.tgz",
            "hl7.fhir.eu.base",
            "2.0.0",
        ),
        (
            "ehdsi/myhealth.eu.fhir.ncp-api-9.1.1-ci.tgz",
            "myhealth.eu.fhir.mvc-package",
            "9.1.0",
        ),
        (
            "ehdsi-laboratory/myhealth.eu.fhir.laboratory-9.1.1.tgz",
            "hl7.fhir.eu.laboratory",
            "2.0.0",
        ),
        (
            "ehdsi-hdr/myhealth.eu.fhir.hdr-9.1.1.tgz",
            "hl7.fhir.eu.hdr",
            "0.1.0-ballot",
        ),
        (
            "ehdsi-eps/myhealth.eu.fhir.eps-1.0.0-poc.tgz",
            "hl7.fhir.eu.eps",
            "1.0.0-ballot",
        ),
        (
            "ehdsi-epd/myhealth.eu.fhir.epd-1.0.0-poc.tgz",
            "hl7.fhir.eu.mpd",
            "1.0.0",
        ),
    ];
    for (path, dependency, version) in cited {
        let package = manifest(path)?;
        let named = package
            .get("dependencies")
            .and_then(|dependencies| dependencies.get(dependency))
            .and_then(Value::as_str);
        assert_eq!(named, Some(version), "{path} depends on {dependency}");
    }
    Ok(())
}

#[test]
fn the_ncpeh_api_build_keeps_the_national_side_out_of_scope() -> TestResult {
    // The scope (home page) and the pre-requisites (Document Retrieval) of the 9.1.1-ci build.
    let home = text("ehdsi/ehdsi-ncp-api-index.html")?;
    assert!(home.contains(
        "It is out of scope of this guide to define the interfaces with the National Connector and/or between the National Connector and National Infrastructure."
    ));
    assert!(home.contains("9.1.1-ci"));
    let retrieval = text("ehdsi/ehdsi-ncp-api-sequence-doc.html")?;
    assert!(retrieval.contains(
        "The Myhealth@EU general architecture requires that no assumptions are made on the way the service is implemented at the national level."
    ));
    Ok(())
}

#[test]
fn the_patient_summary_and_prescription_services_run_on_cda() -> TestResult {
    // The scope section of each guide's home page.
    let proof =
        "This IG serves as a proof of concept for implementing the same service using HL7 FHIR.";
    let summary = text("ehdsi-eps/myhealth.eu.fhir.eps-index.html")?;
    assert!(summary.contains(
        "Current Patient Summary service in MyHealth@EU is operational since 2019 and is based on HL7 CDA."
    ));
    assert!(summary.contains(proof));
    let prescription = text("ehdsi-epd/myhealth.eu.fhir.epd-index.html")?;
    assert!(prescription.contains(
        "Current ePrescription and eDispensation service in MyHealth@EU is operational since 2019 and is based on HL7 CDA."
    ));
    assert!(prescription.contains(proof));
    Ok(())
}

#[test]
fn no_myhealth_prescription_element_carries_the_remaining_dispensations() -> TestResult {
    // The authorised count of repeats is an element; a remaining count is none.
    let definitions = members("ehdsi-epd/myhealth.eu.fhir.epd-1.0.0-poc.tgz", |name| {
        name == "package/StructureDefinition-MedicationRequest-myhealtheu-epd.json"
            || name == "package/StructureDefinition-MyHealthEuPrescription.json"
    })?;
    assert_eq!(definitions.len(), 2);
    let mut paths = BTreeSet::new();
    for definition in definitions.values() {
        let elements = definition
            .get("snapshot")
            .and_then(|snapshot| snapshot.get("element"))
            .and_then(Value::as_array)
            .ok_or("a definition without a snapshot")?;
        for element in elements {
            let body = element.to_string().to_lowercase();
            assert!(!body.contains("remaining"), "{}", string(element, "id"));
            paths.insert(string(element, "path").to_owned());
        }
    }
    assert!(paths.contains("MedicationRequest.dispenseRequest.numberOfRepeatsAllowed"));
    assert!(paths.contains("MyHealthEuPrescription.prescriptionItem.numberOfRepeats"));
    Ok(())
}

/// The OID of every value set the Master Value Sets Catalogue 9.1.0 names by
/// an `urn:oid:` identifier.
fn catalogue_oids() -> Result<BTreeSet<String>, Box<dyn Error>> {
    let value_sets = members("ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz", |name| {
        name.starts_with("package/ValueSet-") && is_json(name)
    })?;
    let mut oids = BTreeSet::new();
    for value_set in value_sets.values() {
        for identifier in value_set
            .get("identifier")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(oid) = string(identifier, "value").strip_prefix("urn:oid:") {
                oids.insert(oid.to_owned());
            }
        }
    }
    Ok(oids)
}

#[test]
fn the_coded_element_list_binds_the_catalogue_but_confidentiality() -> TestResult {
    let list = text("ehdsi-openncp-tm/coded_element_list_ehdsi.xml")?;
    assert!(list.contains(
        "Created based on eHDSI Semantic Implementation Guides and eHDSI PS and eP display documents"
    ));
    assert_eq!(list.matches("<codedElement>").count(), 87);
    let bound: BTreeSet<String> = list
        .split("<valueSet>")
        .skip(1)
        .filter_map(|rest| {
            rest.split_once("</valueSet>")
                .map(|(oid, _)| oid.trim().to_owned())
        })
        .collect();
    assert_eq!(bound.len(), 51);
    let catalogue = catalogue_oids()?;
    let missing: Vec<&String> = bound.difference(&catalogue).collect();
    // eHDSIConfidentiality, which the catalogue's FHIR package does not carry.
    assert_eq!(missing, ["1.3.6.1.4.1.12559.11.10.1.3.1.42.31"]);
    Ok(())
}

#[test]
fn the_transformation_manager_schematrons_are_iso_schematron() -> TestResult {
    let expected = [
        ("PatientSummary.sch", "epSOS Patient Summary Document"),
        ("ePrescription.sch", "epSOS ePrescription Document"),
        ("eDispensation.sch", "epSOS eDispensation Document"),
        (
            "epSOS-PatientSummary-Friendly.sch",
            "epSOS Patient Summary Document",
        ),
    ];
    for (file, document) in expected {
        let schema = text(&format!("ehdsi-openncp-tm/{file}"))?;
        assert!(
            schema.contains(r#"<schema xmlns="http://purl.oclc.org/dsdl/schematron""#),
            "{file}"
        );
        assert!(
            schema.contains(&format!(
                "<title>Schematron schema for validating {document}</title>"
            )),
            "{file}"
        );
    }
    let notice = text("ehdsi-openncp-tm/NOTICE")?;
    assert!(notice.contains("This product is licensed under the Apache License, Version 2.0."));
    Ok(())
}
