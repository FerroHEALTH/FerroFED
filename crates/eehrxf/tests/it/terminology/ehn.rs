// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The eHN Patient Summary §4 preferred systems held to the vendored
//! MyHealth@EU Master Value Sets Catalogue 9.1.0, and NPU, which the
//! catalogue does not include, to the vendored HL7 Europe Laboratory Report
//! 2.0.0: each URI is the system the value set its documentation names
//! includes.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

use eehrxf::terminology::ehn;
use flate2::read::GzDecoder;
use serde::Deserialize;

type TestResult = Result<(), Box<dyn Error>>;

/// The part of a `ValueSet` this test reads.
#[derive(Debug, Deserialize)]
struct ValueSet {
    compose: Option<Compose>,
}

/// A `ValueSet.compose`.
#[derive(Debug, Deserialize)]
struct Compose {
    #[serde(default)]
    include: Vec<Include>,
}

/// A `ValueSet.compose.include`.
#[derive(Debug, Deserialize)]
struct Include {
    system: Option<String>,
    version: Option<String>,
}

/// The vendored MyHealth@EU catalogue.
pub(super) const CATALOGUE: &str = "ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz";

/// The vendored HL7 Europe Laboratory Report guide.
const LABORATORY: &str = "eu-hl7-laboratory/hl7.fhir.eu.laboratory-2.0.0.tgz";

/// The system and version of each include of a value set.
pub(super) type Included = Vec<(String, Option<String>)>;

/// The includes of each value set of the vendored package `package`, a path
/// under `docs/specs/`, by the value set's id.
pub(super) fn includes(package: &str) -> Result<BTreeMap<String, Included>, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs")
        .join(package);
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(path)?));
    let mut sets = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        let Some(set) = name
            .strip_prefix("package/ValueSet-")
            .and_then(|rest| rest.strip_suffix(".json"))
            .map(str::to_owned)
        else {
            continue;
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let value_set: ValueSet = serde_json::from_slice(&bytes)?;
        let included = value_set
            .compose
            .map(|compose| compose.include)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|include| Some((include.system?, include.version)))
            .collect();
        sets.insert(set, included);
    }
    Ok(sets)
}

/// The systems each value set of the vendored package `package` includes,
/// by the value set's id.
fn value_sets(package: &str) -> Result<BTreeMap<String, BTreeSet<String>>, Box<dyn Error>> {
    Ok(includes(package)?
        .into_iter()
        .map(|(set, included)| {
            let systems = included.into_iter().map(|(system, _)| system).collect();
            (set, systems)
        })
        .collect())
}

#[test]
fn each_preferred_system_is_the_uri_the_ehdsi_catalogue_includes() -> TestResult {
    let sets = value_sets(CATALOGUE)?;
    for (system, set) in [
        (ehn::SNOMED_CT, "eHDSIProcedure"),
        (ehn::ICD_10, "eHDSIIllnessandDisorder"),
        (ehn::ORPHACODE, "eHDSIRareDisease"),
        (ehn::ATC, "eHDSIActiveIngredient"),
        (ehn::EDQM_STANDARD_TERMS, "eHDSIDoseForm"),
        (ehn::UCUM, "eHDSIUnit"),
        (ehn::LOINC, "eHDSILabCode"),
        (ehn::ISO_3166, "eHDSICountry"),
        (ehn::HL7_ADMINISTRATIVE_GENDER, "eHDSIAdministrativeGender"),
        (ehn::HL7_ROLE_CLASS, "eHDSIRoleClass"),
        (ehn::HL7_ROLE_CODE, "eHDSIPersonalRelationship"),
        (ehn::HL7_OBSERVATION_CATEGORY, "eHDSIObservationCategory"),
    ] {
        let included = sets.get(set).ok_or(format!("the catalogue has no {set}"))?;
        assert!(included.contains(system), "{set} does not include {system}");
    }
    Ok(())
}

#[test]
fn the_patient_summary_set_names_each_system_once() {
    let distinct: BTreeSet<&str> = ehn::PATIENT_SUMMARY.into_iter().collect();
    assert_eq!(distinct.len(), ehn::PATIENT_SUMMARY.len());
}

#[test]
fn npu_is_the_uri_hl7_europe_includes_its_laboratory_codes_by() -> TestResult {
    let sets = value_sets(LABORATORY)?;
    let included = sets
        .get("lab-obsCode-npu-eu-lab")
        .ok_or("the laboratory guide has no lab-obsCode-npu-eu-lab")?;
    assert_eq!(included, &BTreeSet::from([String::from(ehn::NPU)]));
    assert!(ehn::PATIENT_SUMMARY.contains(&ehn::NPU));
    Ok(())
}

#[test]
fn the_unnamed_medical_device_system_is_not_shipped_as_emdn() -> TestResult {
    // NOTE: eHN PS §4 prefers EMDN for devices, and no vendored text names the catalogue's OID
    // system in eHDSIMedicalDevice as EMDN, so it is no preferred system.
    let sets = value_sets(CATALOGUE)?;
    let devices = sets
        .get("eHDSIMedicalDevice")
        .ok_or("the catalogue has no eHDSIMedicalDevice")?;
    let unnamed: Vec<&String> = devices
        .iter()
        .filter(|system| !ehn::PATIENT_SUMMARY.contains(&system.as_str()))
        .collect();
    assert_eq!(unnamed, ["urn:oid:1.3.6.1.4.1.12559.11.10.1.3.1.44.6"]);
    Ok(())
}
