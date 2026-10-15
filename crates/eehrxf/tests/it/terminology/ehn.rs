// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The eHN Patient Summary §4 preferred systems held to the vendored
//! MyHealth@EU Master Value Sets Catalogue 9.1.0: each URI is the system
//! the value set its documentation names includes.

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
}

/// The systems each value set of the vendored catalogue includes, by the
/// value set's name.
fn catalogue() -> Result<BTreeMap<String, BTreeSet<String>>, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz");
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
        let systems = value_set
            .compose
            .map(|compose| compose.include)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|include| include.system)
            .collect();
        sets.insert(set, systems);
    }
    Ok(sets)
}

#[test]
fn each_preferred_system_is_the_uri_the_ehdsi_catalogue_includes() -> TestResult {
    let sets = catalogue()?;
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
