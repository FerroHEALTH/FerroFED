// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Coding.system` of an openEHR terminology id: a URI for every
//! coding written, the openEHR names mapped to their URIs, and a terminology
//! with no known system written as no coding, in the narrative and reported
//! (FHIR R4 `Coding.system`; openEHR BASE 1.1.0 `TERMINOLOGY_CODE`; RM 1.1.0
//! Support IM, Terminology Identifiers; eHN PS Art 10(7)).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::terminology::ehn;
use eehrxf::terminology::system::ARCHETYPE_LOCAL;
use eehrxf::terminology::system::archetype_local;
use eehrxf::terminology::system::fhir_system;
use eehrxf::terminology::transcode::PreferredSystems;
use eehrxf::terminology::transcode::Transcoded;
use eehrxf::terminology::transcode::transcode;
use openehr_base::v1_3::base_types::identification::archetype_id::ArchetypeId;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;
use url::Url;

use super::PREFERRED;

type TestResult = Result<(), Box<dyn Error>>;

/// A `DV_CODED_TEXT` of the terminology `terminology` and `code`, with an
/// optional `'='` mapping to `equivalent`, a terminology and a code.
fn coded(
    terminology: &str,
    code: &str,
    equivalent: Option<(&str, &str)>,
) -> Result<DvText, Box<dyn Error>> {
    let phrase = |terminology: &str, code: &str| {
        format!(
            r#"{{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "{terminology}" }}, "code_string": "{code}" }}"#
        )
    };
    let mappings = equivalent.map_or_else(String::new, |(terminology, code)| {
        format!(
            r#", "mappings": [{{ "_type": "TERM_MAPPING", "match": "=", "target": {} }}]"#,
            phrase(terminology, code)
        )
    });
    let text = format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "Synthetic finding", "defining_code": {}{mappings} }}"#,
        phrase(terminology, code)
    );
    Ok(serde_json::from_str(&text)?)
}

/// The system and code of every coding.
fn codes(transcoded: &Transcoded) -> Vec<(&str, &str)> {
    transcoded
        .codings()
        .iter()
        .filter_map(|coding| {
            Some((
                coding.system.as_ref()?.value.as_deref()?,
                coding.code.as_ref()?.value.as_deref()?,
            ))
        })
        .collect()
}

/// The terminology and code of every unmapped code.
fn unmapped(transcoded: &Transcoded) -> Vec<(&str, &str)> {
    transcoded
        .unmapped()
        .iter()
        .map(|unmapped| (unmapped.terminology(), unmapped.code()))
        .collect()
}

#[test]
fn the_openehr_names_map_to_their_fhir_systems() -> TestResult {
    for (name, uri) in [
        ("SNOMED-CT", ehn::SNOMED_CT),
        ("LOINC", ehn::LOINC),
        ("ICD10", ehn::ICD_10),
    ] {
        assert_eq!(fhir_system(name, None)?.as_deref(), Some(uri), "{name}");
        let transcoded = transcode(&coded(name, "1001", None)?, &PreferredSystems::default())?;
        assert_eq!(codes(&transcoded), [(uri, "1001")], "{name}");
        assert!(!transcoded.untranscoded(), "{name} is preferred");
        assert_eq!(transcoded.unmapped(), []);
    }
    Ok(())
}

#[test]
fn a_terminology_with_no_known_system_is_no_coding_and_is_reported() -> TestResult {
    for name in ["openehr", "ISO_639-1", "ICD10AM", "ICPC2E"] {
        assert_eq!(fhir_system(name, None)?, None, "{name}");
        let transcoded = transcode(&coded(name, "C-1", None)?, &PreferredSystems::default())?;
        assert!(
            transcoded.codings().is_empty(),
            "{name} is never written raw"
        );
        assert_eq!(
            transcoded.narrative(),
            [
                String::from("Synthetic finding"),
                format!("Synthetic finding: {name} C-1")
            ],
            "Art 10(7): in narrative form"
        );
        assert_eq!(unmapped(&transcoded), [(name, "C-1")]);
        assert!(transcoded.untranscoded(), "{name}");
    }
    Ok(())
}

#[test]
fn an_equivalent_mapping_with_no_known_system_is_narrated_and_reported() -> TestResult {
    let transcoded = transcode(
        &coded("SNOMED-CT", "1001", Some(("ICPC2E", "K86")))?,
        &PreferredSystems::default(),
    )?;
    assert_eq!(codes(&transcoded), [(ehn::SNOMED_CT, "1001")]);
    assert_eq!(
        transcoded.narrative(),
        [
            "Synthetic finding",
            "Synthetic finding: equivalent term ICPC2E K86"
        ]
    );
    assert_eq!(unmapped(&transcoded), [("ICPC2E", "K86")]);
    assert!(!transcoded.untranscoded(), "the original is preferred");
    Ok(())
}

#[test]
fn a_terminology_id_that_is_an_absolute_uri_is_its_own_system() -> TestResult {
    assert_eq!(fhir_system(PREFERRED, None)?.as_deref(), Some(PREFERRED));
    assert_eq!(
        fhir_system(ehn::ISO_3166, None)?.as_deref(),
        Some(ehn::ISO_3166)
    );
    let transcoded = transcode(
        &coded(PREFERRED, "1001", None)?,
        &PreferredSystems::new([PREFERRED]),
    )?;
    assert_eq!(codes(&transcoded), [(PREFERRED, "1001")]);
    Ok(())
}

#[test]
fn a_local_code_has_the_system_of_its_archetype() -> TestResult {
    let archetype = ArchetypeId {
        value: String::from("openEHR-EHR-OBSERVATION.ferrofed_pulse.v2"),
    };
    let system = archetype_local(&archetype)?;
    assert_eq!(
        system,
        format!("{ARCHETYPE_LOCAL}openEHR-EHR-OBSERVATION.ferrofed_pulse.v2")
    );
    assert_eq!(
        fhir_system("local", Some(&archetype))?.as_deref(),
        Some(system.as_str())
    );
    assert_eq!(fhir_system("local", None)?, None, "no archetype, no system");
    Ok(())
}

#[test]
fn every_system_written_is_an_absolute_uri() -> TestResult {
    let archetype = ArchetypeId {
        value: String::from("openEHR-EHR-OBSERVATION.ferrofed_pulse.v2"),
    };
    let mut systems = vec![archetype_local(&archetype)?];
    for (name, _) in eehrxf::terminology::system::OPENEHR_NAMES {
        systems.extend(fhir_system(name, None)?);
    }
    for system in systems {
        assert!(Url::parse(&system).is_ok(), "{system} is no absolute URI");
    }
    Ok(())
}

#[test]
fn an_archetype_id_that_is_not_one_makes_no_system() {
    let malformed = ArchetypeId {
        value: String::from("openEHR-EHR.concept.v1"),
    };
    assert!(archetype_local(&malformed).is_err());
    assert!(fhir_system("local", Some(&malformed)).is_err());
}
