// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An openEHR text item as FHIR codings and narrative: a preferred code as
//! recorded, an `'='` mapping as a second coding, a broader, narrower or
//! unknown mapping in the narrative alone, and an untranscodable code under
//! eHN PS Art 10(7) (RM Release 1.1.0 `TERM_MAPPING.match`; eHN PS Art
//! 10(3), 10(7)).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use eehrxf::terminology::Terminology;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::transcode::PreferredSystems;
use eehrxf::terminology::transcode::TranscodeError;
use eehrxf::terminology::transcode::Transcoded;
use eehrxf::terminology::transcode::transcode;
use eehrxf::terminology::transcode::transcode_with;
use fhir_types::r4::coding::Coding;
use http::StatusCode;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;

use super::LOCAL;
use super::PREFERRED;
use super::Script;
use super::Scripted;

type TestResult = Result<(), Box<dyn Error>>;

/// The preferred systems of every test.
fn preferred() -> PreferredSystems {
    PreferredSystems::new([PREFERRED])
}

/// A canonical-JSON code phrase.
fn phrase(system: &str, code: &str, term: Option<&str>) -> String {
    let term = term.map_or_else(String::new, |term| {
        format!(r#", "preferred_term": "{term}""#)
    });
    format!(
        r#"{{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "{system}" }}, "code_string": "{code}"{term} }}"#
    )
}

/// A canonical-JSON `TERM_MAPPING` with `relation` to `target`.
fn mapping(relation: char, target: &str) -> String {
    format!(r#"{{ "_type": "TERM_MAPPING", "match": "{relation}", "target": {target} }}"#)
}

/// A `DV_CODED_TEXT` of `system` and `code`, with `mappings`.
fn coded(system: &str, code: &str, mappings: &[String]) -> Result<DvText, Box<dyn Error>> {
    let mappings = if mappings.is_empty() {
        String::new()
    } else {
        format!(r#", "mappings": [{}]"#, mappings.join(","))
    };
    let text = format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "Synthetic finding", "defining_code": {}{mappings} }}"#,
        phrase(system, code, None)
    );
    Ok(serde_json::from_str(&text)?)
}

/// The system and code of every coding.
fn codes(transcoded: &Transcoded) -> Vec<(String, String)> {
    transcoded
        .codings()
        .iter()
        .map(|coding: &Coding| {
            (
                coding
                    .system
                    .as_ref()
                    .and_then(|system| system.value.clone())
                    .unwrap_or_default(),
                coding
                    .code
                    .as_ref()
                    .and_then(|code| code.value.clone())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn pair(system: &str, code: &str) -> (String, String) {
    (system.to_owned(), code.to_owned())
}

#[test]
fn a_code_in_a_preferred_system_passes_as_recorded() -> TestResult {
    let transcoded = transcode(&coded(PREFERRED, "1001", &[])?, &preferred())?;
    assert_eq!(codes(&transcoded), vec![pair(PREFERRED, "1001")]);
    let display = transcoded
        .codings()
        .first()
        .and_then(|coding| coding.display.as_ref())
        .and_then(|display| display.value.as_deref());
    assert_eq!(
        display,
        Some("Synthetic finding"),
        "the rubric is the display"
    );
    assert!(!transcoded.untranscoded());
    assert_eq!(transcoded.narrative(), ["Synthetic finding"], "Art 10(3)");
    Ok(())
}

#[test]
fn an_equivalent_mapping_adds_a_coding_after_the_original() -> TestResult {
    let equivalent = mapping('=', &phrase(PREFERRED, "2002", Some("Synthetic term")));
    let transcoded = transcode(&coded(LOCAL, "L-1", &[equivalent])?, &preferred())?;
    assert_eq!(
        codes(&transcoded),
        vec![pair(LOCAL, "L-1"), pair(PREFERRED, "2002")],
        "the original stays first"
    );
    assert!(!transcoded.untranscoded());
    assert_eq!(transcoded.narrative(), ["Synthetic finding"]);
    Ok(())
}

/// Transcodes a local code with one mapping of `relation`, and checks that
/// it adds no coding and one narrative line naming `words`.
fn narrative_only(relation: char, words: &str) -> TestResult {
    let related = mapping(relation, &phrase(PREFERRED, "3003", Some("Related term")));
    let transcoded = transcode(&coded(LOCAL, "L-1", &[related])?, &preferred())?;
    assert_eq!(
        codes(&transcoded),
        vec![pair(LOCAL, "L-1")],
        "a {relation} mapping is no coding"
    );
    assert_eq!(
        transcoded.narrative(),
        [
            String::from("Synthetic finding"),
            format!("Synthetic finding: {words} {PREFERRED} 3003 (Related term)"),
        ]
    );
    assert!(
        transcoded.untranscoded(),
        "no preferred coding, so Art 10(7)"
    );
    Ok(())
}

#[test]
fn a_broader_mapping_produces_no_coding_and_appears_in_the_narrative() -> TestResult {
    narrative_only('>', "broader term")
}

#[test]
fn a_narrower_mapping_produces_no_coding_and_appears_in_the_narrative() -> TestResult {
    narrative_only('<', "narrower term")
}

#[test]
fn an_unknown_mapping_produces_no_coding_and_appears_in_the_narrative() -> TestResult {
    narrative_only('?', "related term of unknown kind")
}

#[test]
fn an_untranscodable_code_is_transferred_encoded_with_its_narrative() -> TestResult {
    let transcoded = transcode(&coded(LOCAL, "L-1", &[])?, &preferred())?;
    assert_eq!(
        codes(&transcoded),
        vec![pair(LOCAL, "L-1")],
        "Art 10(7): transferred encoded"
    );
    let display = transcoded
        .codings()
        .first()
        .and_then(|coding| coding.display.as_ref())
        .and_then(|display| display.value.as_deref());
    assert_eq!(display, Some("Synthetic finding"), "displayed as recorded");
    assert_eq!(
        transcoded.narrative(),
        ["Synthetic finding"],
        "and in narrative form"
    );
    assert!(transcoded.untranscoded());
    Ok(())
}

#[test]
fn a_match_the_rm_does_not_admit_is_refused() -> TestResult {
    let invalid = mapping('~', &phrase(PREFERRED, "3003", None));
    let refused = transcode(&coded(LOCAL, "L-1", &[invalid])?, &preferred());
    assert!(matches!(refused, Err(TranscodeError::InvalidMatch)));
    Ok(())
}

#[test]
fn a_plain_text_carries_only_its_equivalent_mappings() -> TestResult {
    let text = format!(
        r#"{{ "_type": "DV_TEXT", "value": "Synthetic note", "mappings": [{}] }}"#,
        mapping('=', &phrase(PREFERRED, "4004", None))
    );
    let transcoded = transcode(&serde_json::from_str(&text)?, &preferred())?;
    assert_eq!(codes(&transcoded), vec![pair(PREFERRED, "4004")]);
    assert_eq!(transcoded.narrative(), ["Synthetic note"]);
    assert!(!transcoded.untranscoded());
    Ok(())
}

#[tokio::test]
async fn translation_without_a_server_fails_closed() {
    let refused = Terminology::none().translate(LOCAL, "L-1", PREFERRED).await;
    assert!(matches!(
        refused,
        Err(TerminologyError::Unconfigured { .. })
    ));
}

#[tokio::test]
async fn with_no_server_an_untranscodable_code_follows_article_10_7() -> TestResult {
    let text = coded(LOCAL, "L-1", &[])?;
    let transcoded = transcode_with(&text, &preferred(), &Terminology::none()).await?;
    assert_eq!(transcoded, transcode(&text, &preferred())?);
    assert!(transcoded.untranscoded());
    Ok(())
}

#[tokio::test]
async fn a_translation_adds_only_equivalent_and_equal_matches() -> TestResult {
    let server = Scripted(Script::Translate(vec![
        ("equivalent", PREFERRED, "5005"),
        ("wider", PREFERRED, "5006"),
        ("narrower", PREFERRED, "5007"),
        ("inexact", PREFERRED, "5008"),
        ("equal", PREFERRED, "5009"),
    ]));
    let terminology = Terminology::server(Arc::new(server));
    let transcoded = transcode_with(&coded(LOCAL, "L-1", &[])?, &preferred(), &terminology).await?;
    assert_eq!(
        codes(&transcoded),
        vec![
            pair(LOCAL, "L-1"),
            pair(PREFERRED, "5005"),
            pair(PREFERRED, "5009")
        ]
    );
    assert!(!transcoded.untranscoded());
    Ok(())
}

#[tokio::test]
async fn a_refused_translation_is_an_error_carrying_the_status() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Refuse(
        StatusCode::SERVICE_UNAVAILABLE,
    ))));
    let refused = transcode_with(&coded(LOCAL, "L-1", &[])?, &preferred(), &terminology).await;
    assert!(
        matches!(
            refused,
            Err(TranscodeError::Terminology(TerminologyError::Status {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }))
        ),
        "never an untranscoded success: {refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_preferred_code_asks_no_server() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Refuse(
        StatusCode::INTERNAL_SERVER_ERROR,
    ))));
    let transcoded =
        transcode_with(&coded(PREFERRED, "1001", &[])?, &preferred(), &terminology).await?;
    assert_eq!(codes(&transcoded), vec![pair(PREFERRED, "1001")]);
    Ok(())
}
