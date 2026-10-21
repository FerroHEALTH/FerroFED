// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An archetype-local code rendered from the term definitions of the
//! synthetic operational template, in the language asked, and the eHN
//! Patient Summary §4 systems the rules prefer by default (openEHR BASE
//! `OPENEHR_DEFINITIONS.Local_terminology_id`; eHN PS Art 10(7), §4).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use eehrxf::terminology::Terminology;
use eehrxf::terminology::ehn;
use eehrxf::terminology::local::Term;
use eehrxf::terminology::local::TermDefinitions;
use eehrxf::terminology::transcode::Local;
use eehrxf::terminology::transcode::PreferredSystems;
use eehrxf::terminology::transcode::TranscodeError;
use eehrxf::terminology::transcode::Transcoded;
use eehrxf::terminology::transcode::transcode;
use eehrxf::terminology::transcode::transcode_local;
use eehrxf::terminology::transcode::transcode_local_with;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::archetype_id::ArchetypeId;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;

use super::PREFERRED;
use super::Script;
use super::Scripted;

type TestResult = Result<(), Box<dyn Error>>;

/// The evaluation archetype of the synthetic template.
const EVALUATION: &str = "openEHR-EHR-EVALUATION.ferrofed_allergy.v1";

/// The composition archetype of the synthetic template.
const COMPOSITION: &str = "openEHR-EHR-COMPOSITION.ferrofed_summary.v1";

/// The system of the evaluation archetype's local codes.
const EVALUATION_SYSTEM: &str = "https://ferrofed.eu/fhir/CodeSystem/archetype-local/openEHR-EHR-EVALUATION.ferrofed_allergy.v1";

/// The term definitions of the synthetic allergy template, written in
/// English.
fn terms() -> Result<TermDefinitions, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opt/allergy.opt");
    Ok(TermDefinitions::from_opt14(&std::fs::read_to_string(
        path,
    )?)?)
}

fn archetype(value: &str) -> ArchetypeId {
    ArchetypeId {
        value: value.to_owned(),
    }
}

/// A `DV_CODED_TEXT` of `system` and `code`, recorded as `value`.
fn coded(system: &str, code: &str, value: &str) -> Result<DvText, Box<dyn Error>> {
    let text = format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "{value}", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "{system}" }}, "code_string": "{code}" }} }}"#
    );
    Ok(serde_json::from_str(&text)?)
}

/// The display of the first coding.
fn display(transcoded: &Transcoded) -> Option<&str> {
    transcoded
        .codings()
        .first()
        .and_then(|coding| coding.display.as_ref())
        .and_then(|display| display.value.as_deref())
}

/// The system and code of the first coding.
fn first(transcoded: &Transcoded) -> Option<(&str, &str)> {
    let coding = transcoded.codings().first()?;
    Some((
        coding.system.as_ref()?.value.as_deref()?,
        coding.code.as_ref()?.value.as_deref()?,
    ))
}

#[test]
fn a_local_code_is_rendered_from_the_template_in_the_language_asked() -> TestResult {
    let terms = terms()?;
    let evaluation = archetype(EVALUATION);
    let local = Local::new(&terms, &evaluation, "en");
    let transcoded = transcode_local(
        &coded("local", "at0002", "Substance")?,
        &PreferredSystems::default(),
        &local,
    )?;
    assert_eq!(
        first(&transcoded),
        Some((EVALUATION_SYSTEM, "at0002")),
        "the code as recorded, under the system of its archetype"
    );
    assert_eq!(transcoded.unmapped(), []);
    assert_eq!(display(&transcoded), Some("Substance"));
    assert_eq!(transcoded.narrative(), ["Substance"]);
    assert!(transcoded.is_from_template());
    assert!(
        !transcoded.untranscoded(),
        "a template term is not Art 10(7)"
    );
    Ok(())
}

#[test]
fn the_template_term_opens_the_narrative_before_a_differing_recorded_text() -> TestResult {
    let terms = terms()?;
    let evaluation = archetype(EVALUATION);
    let local = Local::new(&terms, &evaluation, "en");
    let transcoded = transcode_local(
        &coded("local", "at0003", "Synthetic recorded status")?,
        &PreferredSystems::default(),
        &local,
    )?;
    assert_eq!(display(&transcoded), Some("Clinical status"));
    assert_eq!(
        transcoded.narrative(),
        ["Clinical status", "Synthetic recorded status"],
        "the recorded text is kept"
    );
    Ok(())
}

#[test]
fn a_local_code_is_read_in_the_archetype_the_item_sits_in() -> TestResult {
    let terms = terms()?;
    assert_eq!(
        terms.term("en", &archetype(EVALUATION), "at0000"),
        Term::Defined("Synthetic allergy")
    );
    assert_eq!(
        terms.term("en", &archetype(COMPOSITION), "at0000"),
        Term::Defined("Synthetic allergy summary")
    );
    assert_eq!(terms.language(), "en");
    Ok(())
}

#[test]
fn a_language_the_template_does_not_carry_follows_article_10_7() -> TestResult {
    let terms = terms()?;
    let evaluation = archetype(EVALUATION);
    let local = Local::new(&terms, &evaluation, "nl");
    let text = coded("local", "at0002", "Substance")?;
    let transcoded = transcode_local(&text, &PreferredSystems::default(), &local)?;
    assert_eq!(
        first(&transcoded),
        Some((EVALUATION_SYSTEM, "at0002")),
        "transferred encoded, under the system of its archetype"
    );
    assert_eq!(
        display(&transcoded),
        Some("Substance"),
        "displayed as recorded"
    );
    assert_eq!(transcoded.narrative(), ["Substance"]);
    assert_eq!(transcoded.unmapped(), []);
    assert!(transcoded.untranscoded());
    assert!(!transcoded.is_from_template());
    Ok(())
}

#[test]
fn the_same_local_code_of_two_archetypes_is_two_systems() -> TestResult {
    let terms = terms()?;
    let evaluation = archetype(EVALUATION);
    let composition = archetype(COMPOSITION);
    let text = coded("local", "at0000", "Synthetic")?;
    let preferred = PreferredSystems::default();
    let of_evaluation = transcode_local(&text, &preferred, &Local::new(&terms, &evaluation, "en"))?;
    let of_composition =
        transcode_local(&text, &preferred, &Local::new(&terms, &composition, "en"))?;
    assert_eq!(first(&of_evaluation), Some((EVALUATION_SYSTEM, "at0000")));
    assert_eq!(
        first(&of_composition),
        Some((
            "https://ferrofed.eu/fhir/CodeSystem/archetype-local/openEHR-EHR-COMPOSITION.ferrofed_summary.v1",
            "at0000"
        ))
    );
    Ok(())
}

#[test]
fn a_local_code_read_in_an_archetype_id_that_is_not_one_is_refused() -> TestResult {
    let terms = terms()?;
    let malformed = archetype("not an archetype");
    let local = Local::new(&terms, &malformed, "en");
    let text = r#"{ "_type": "DV_TEXT", "value": "Substance", "mappings": [{ "_type": "TERM_MAPPING", "match": "=", "target": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "local" }, "code_string": "at0002" } }] }"#;
    let refused = transcode_local(
        &serde_json::from_str(text)?,
        &PreferredSystems::default(),
        &local,
    );
    assert!(
        matches!(
            &refused,
            Err(TranscodeError::ArchetypeId { archetype, .. }) if archetype == "not an archetype"
        ),
        "no system URI is made of it: {refused:?}"
    );
    Ok(())
}

#[test]
fn a_local_code_the_archetype_does_not_define_is_refused() -> TestResult {
    let terms = terms()?;
    let evaluation = archetype(EVALUATION);
    let local = Local::new(&terms, &evaluation, "en");
    let refused = transcode_local(
        &coded("local", "at0099", "Synthetic")?,
        &PreferredSystems::default(),
        &local,
    );
    assert!(
        matches!(
            &refused,
            Err(TranscodeError::UndefinedLocalCode { archetype, code })
                if archetype == EVALUATION && code == "at0099"
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn without_a_template_a_local_code_follows_article_10_7() -> TestResult {
    let transcoded = transcode(
        &coded("local", "at0002", "Substance")?,
        &PreferredSystems::default(),
    )?;
    assert!(
        transcoded.codings().is_empty(),
        "with no archetype the code has no system, so it is no coding"
    );
    assert_eq!(
        transcoded.narrative(),
        ["Substance", "Substance: local at0002"],
        "Art 10(7): in narrative form"
    );
    let unmapped: Vec<(&str, &str)> = transcoded
        .unmapped()
        .iter()
        .map(|unmapped| (unmapped.terminology(), unmapped.code()))
        .collect();
    assert_eq!(unmapped, [("local", "at0002")], "and reported");
    assert!(transcoded.untranscoded());
    assert!(!transcoded.is_from_template());
    Ok(())
}

#[tokio::test]
async fn a_template_term_asks_no_terminology_server() -> TestResult {
    let terms = terms()?;
    let evaluation = archetype(EVALUATION);
    let local = Local::new(&terms, &evaluation, "en");
    let terminology = Terminology::server(Arc::new(Scripted(Script::Refuse(
        StatusCode::INTERNAL_SERVER_ERROR,
    ))));
    let transcoded = transcode_local_with(
        &coded("local", "at0002", "Substance")?,
        &PreferredSystems::new([PREFERRED]),
        &local,
        &terminology,
    )
    .await?;
    assert_eq!(display(&transcoded), Some("Substance"));
    Ok(())
}

#[test]
fn the_rules_prefer_every_ehn_patient_summary_system_by_default() {
    let preferred = PreferredSystems::default();
    for system in ehn::PATIENT_SUMMARY {
        assert!(preferred.contains(system), "{system} is not preferred");
    }
    assert_eq!(preferred.iter().count(), ehn::PATIENT_SUMMARY.len());
}

#[test]
fn a_snomed_ct_code_passes_as_recorded_under_the_default_systems() -> TestResult {
    let transcoded = transcode(
        &coded(ehn::SNOMED_CT, "1001", "Synthetic finding")?,
        &PreferredSystems::default(),
    )?;
    assert_eq!(first(&transcoded), Some((ehn::SNOMED_CT, "1001")));
    assert!(!transcoded.untranscoded());
    Ok(())
}

#[test]
fn a_caller_may_name_its_own_systems() -> TestResult {
    let transcoded = transcode(
        &coded(ehn::SNOMED_CT, "1001", "Synthetic finding")?,
        &PreferredSystems::new([PREFERRED]),
    )?;
    assert!(
        transcoded.untranscoded(),
        "SNOMED CT is not in the set named"
    );
    Ok(())
}
