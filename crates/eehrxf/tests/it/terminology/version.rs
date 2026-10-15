// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Coding.version` of an openEHR terminology id's version: the
//! version in the form the code system's versions take, as the vendored
//! packages write them, and none where the openEHR value is not in that form
//! (FHIR R4 `Coding.version`; openEHR BASE 1.1.0 `TERMINOLOGY_ID`).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::terminology::ehn;
use eehrxf::terminology::system::fhir_version;
use eehrxf::terminology::transcode::PreferredSystems;
use eehrxf::terminology::transcode::transcode;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;

use super::ehn::CATALOGUE;
use super::ehn::includes;

type TestResult = Result<(), Box<dyn Error>>;

/// The vendored HL7 France Core guide, whose value sets include SNOMED CT at
/// a version.
const FRANCE_CORE: &str = "fr-ans/hl7.fhir.fr.core-2.2.0.tgz";

/// A SNOMED CT version of the vendored HL7 France Core 2.2.0: the
/// International Edition's module at the release of 2026-02-01.
const EDITION_AND_RELEASE: &str = "http://snomed.info/sct/900000000000207008/version/20260201";

/// The `system` and `version` of the one coding a `DV_CODED_TEXT` of the
/// terminology id `terminology` (with its version) is written as.
fn written(terminology: &str) -> Result<(String, Option<String>), Box<dyn Error>> {
    let text: DvText = serde_json::from_str(&format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "Synthetic finding", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "{terminology}" }}, "code_string": "1001" }} }}"#
    ))?;
    let transcoded = transcode(&text, &PreferredSystems::default())?;
    let [coding] = transcoded.codings() else {
        return Err(format!("{terminology} is not one coding").into());
    };
    let system = coding
        .system
        .as_ref()
        .and_then(|system| system.value.clone())
        .ok_or("a coding with no system")?;
    let version = coding
        .version
        .as_ref()
        .and_then(|version| version.value.clone());
    Ok((system, version))
}

#[test]
fn a_snomed_ct_version_naming_an_edition_is_written_as_recorded() -> TestResult {
    let edition = "http://snomed.info/sct/900000000000207008";
    for version in [EDITION_AND_RELEASE, edition] {
        let (system, written) = written(&format!("SNOMED-CT({version})"))?;
        assert_eq!(system, ehn::SNOMED_CT);
        assert_eq!(written.as_deref(), Some(version));
    }
    Ok(())
}

#[test]
fn a_snomed_ct_version_naming_no_edition_is_left_out() -> TestResult {
    for version in [
        "20260201",
        "v2003",
        "International_2026",
        "http://snomed.info/sct",
        "http://snomed.info/sct/",
        "http://snomed.info/sct/edition",
        "http://snomed.info/sct/900000000000207008/version/",
        "http://snomed.info/sct/900000000000207008/version/2026-02-01",
        "https://snomed.info/sct/900000000000207008",
    ] {
        let (system, written) = written(&format!("SNOMED-CT({version})"))?;
        assert_eq!(system, ehn::SNOMED_CT, "{version}");
        assert_eq!(written, None, "{version} names no edition");
    }
    Ok(())
}

#[test]
fn a_snomed_ct_version_by_its_uri_terminology_id_follows_the_same_rule() -> TestResult {
    let (_, kept) = written(&format!("{}({EDITION_AND_RELEASE})", ehn::SNOMED_CT))?;
    assert_eq!(kept.as_deref(), Some(EDITION_AND_RELEASE));
    let (_, left_out) = written(&format!("{}(20260201)", ehn::SNOMED_CT))?;
    assert_eq!(left_out, None);
    Ok(())
}

#[test]
fn every_snomed_ct_version_of_the_vendored_packages_is_in_form() -> TestResult {
    let mut seen = 0_usize;
    for included in includes(FRANCE_CORE)?.into_values() {
        for (system, version) in included {
            if let (ehn::SNOMED_CT, Some(version)) = (system.as_str(), version) {
                assert_eq!(
                    fhir_version(ehn::SNOMED_CT, &version),
                    Some(version.as_str())
                );
                seen += 1;
            }
        }
    }
    assert!(
        seen > 0,
        "the vendored guide includes SNOMED CT at no version"
    );
    Ok(())
}

#[test]
fn an_hl7_version_that_is_a_dotted_number_is_written_as_recorded() -> TestResult {
    for (system, version) in [
        (ehn::HL7_ADMINISTRATIVE_GENDER, "4.0.0"),
        (ehn::HL7_ROLE_CLASS, "5.0.0"),
        (ehn::HL7_ROLE_CODE, "4.0.0"),
        (ehn::HL7_OBSERVATION_CATEGORY, "2.0.0"),
    ] {
        let (written_system, written) = written(&format!("{system}({version})"))?;
        assert_eq!(written_system, system);
        assert_eq!(written.as_deref(), Some(version), "{system}");
    }
    Ok(())
}

#[test]
fn an_hl7_version_that_is_no_dotted_number_is_left_out() -> TestResult {
    for version in ["R2", "v4", "4.0.", ".4", "4..0", "4.0.0-ballot"] {
        let (_, written) = written(&format!("{}({version})", ehn::HL7_ROLE_CODE))?;
        assert_eq!(written, None, "{version}");
    }
    Ok(())
}

#[test]
fn every_hl7_version_of_the_catalogue_is_in_form() -> TestResult {
    let hl7 = [
        ehn::HL7_ADMINISTRATIVE_GENDER,
        ehn::HL7_ROLE_CLASS,
        ehn::HL7_ROLE_CODE,
        ehn::HL7_OBSERVATION_CATEGORY,
    ];
    let mut seen = 0_usize;
    for included in includes(CATALOGUE)?.into_values() {
        for (system, version) in included {
            let Some(system) = hl7.into_iter().find(|hl7| *hl7 == system) else {
                continue;
            };
            let version =
                version.ok_or(format!("the catalogue includes {system} at no version"))?;
            assert_eq!(fhir_version(system, &version), Some(version.as_str()));
            seen += 1;
        }
    }
    assert!(
        seen >= hl7.len(),
        "every HL7 system is included at a version"
    );
    Ok(())
}

#[test]
fn a_system_with_no_vendored_version_form_writes_the_version_as_recorded() -> TestResult {
    for (terminology, system, version) in [
        ("LOINC(v2_77)", ehn::LOINC, "v2_77"),
        ("ICD10(edition_2019)", ehn::ICD_10, "edition_2019"),
    ] {
        let (written_system, written) = written(terminology)?;
        assert_eq!(written_system, system);
        assert_eq!(written.as_deref(), Some(version), "{terminology}");
    }
    for system in [
        ehn::ORPHACODE,
        ehn::ATC,
        ehn::EDQM_STANDARD_TERMS,
        ehn::UCUM,
        ehn::NPU,
        ehn::ISO_3166,
    ] {
        assert_eq!(fhir_version(system, "v2026"), Some("v2026"), "{system}");
    }
    Ok(())
}

#[test]
fn a_terminology_with_no_version_writes_none() -> TestResult {
    for terminology in ["SNOMED-CT", "LOINC", "ICD10"] {
        assert_eq!(written(terminology)?.1, None, "{terminology}");
    }
    Ok(())
}
