// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The codes of a summary's entries: an RM term mapping reaches a coding or
//! the narrative by its match, and a code in no preferred system is
//! translated through the terminology server.

use std::error::Error;

use eehrxf::terminology::translation::TranslationError;
use eehrxf::terminology::{Operation, Terminology, TerminologyError};
use ferrofed_eehrxf::summary::error::AssemblyError;
use ferrofed_eehrxf::summary::mappings::Mappings;
use ferrofed_eehrxf::summary::{Answer, Assembled, Held};
use ferrofed_testkit::eps;

use super::{allergies, answers, document, document_asking, header, section};

/// The SNOMED CT code the term mappings of [`mapped_allergy`] name.
const MAPPED_CODE: &str = "762952008";

/// One allergy composition whose substance carries an RM `TERM_MAPPING` of
/// `match` to [`MAPPED_CODE`].
fn mapped_allergy(relation_code: &str) -> Result<Held, Box<dyn Error>> {
    let uid = "7c4e1d20-0000-4000-8000-0000000000c1::cdr-a.example.org::1";
    let mut composition: serde_json::Value = serde_json::from_str(&eps::allergy_composition(
        uid,
        eps::TEMPLATE,
        "Synthetic substance",
    ))?;
    let substance = composition
        .pointer_mut("/content/0/data/items/0/value")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("the substance")?;
    substance.insert(
        String::from("mappings"),
        serde_json::json!([{
            "_type": "TERM_MAPPING",
            "match": relation_code,
            "target": {
                "_type": "CODE_PHRASE",
                "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "SNOMED-CT" },
                "code_string": MAPPED_CODE
            }
        }]),
    );
    Ok(Held {
        uid: uid.to_owned(),
        template: eps::TEMPLATE.to_owned(),
        composition: composition.to_string(),
    })
}

/// The allergies section and its narrative in the document node A's
/// composition with a term mapping of `match` makes.
fn mapped_section(relation_code: &str) -> Result<(serde_json::Value, String), Box<dyn Error>> {
    let assembled = document(&answers(
        &[mapped_allergy(relation_code)?],
        &Answer::Answered(Vec::new()),
    ))?;
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::profile::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    let allergies = section(&bundle, "48765-2").ok_or("the allergies section")?;
    let div = allergies["text"]["div"]
        .as_str()
        .ok_or("a narrative")?
        .to_owned();
    Ok((bundle, div))
}

/// Holds that a term mapping of `match` is written as no coding and as the
/// narrative line naming `words`, the target and the member.
fn withheld_into_the_narrative(relation_code: &str, words: &str) -> Result<(), Box<dyn Error>> {
    let (bundle, div) = mapped_section(relation_code)?;
    let line =
        format!("AllergyIntolerance.code: {words} SNOMED-CT {MAPPED_CODE}, from Synthetic org-a.");
    assert!(div.contains(&line), "eHN PS Art 10(3): {div}");
    assert!(
        !bundle
            .to_string()
            .contains(&format!("\"code\":\"{MAPPED_CODE}\"")),
        "RM TERM_MAPPING `{relation_code}`: a mapping that is no equivalence is no coding"
    );
    Ok(())
}

#[test]
fn a_broader_term_mapping_reaches_the_narrative_and_no_coding() -> Result<(), Box<dyn Error>> {
    withheld_into_the_narrative(">", "broader term")
}

#[test]
fn a_narrower_term_mapping_reaches_the_narrative_and_no_coding() -> Result<(), Box<dyn Error>> {
    withheld_into_the_narrative("<", "narrower term")
}

#[test]
fn an_unknown_term_mapping_reaches_the_narrative_and_no_coding() -> Result<(), Box<dyn Error>> {
    withheld_into_the_narrative("?", "related term of unknown kind")
}

#[test]
fn an_equivalent_term_mapping_is_a_coding_and_no_narrative_line() -> Result<(), Box<dyn Error>> {
    let (bundle, div) = mapped_section("=")?;
    assert!(
        bundle
            .to_string()
            .contains(&format!("\"code\":\"{MAPPED_CODE}\"")),
        "RM TERM_MAPPING `=`: an equivalent term is written as a coding"
    );
    assert!(!div.contains(MAPPED_CODE), "{div}");
    Ok(())
}

/// A code system no deployment prefers, as the recording system names it.
const LOCAL: &str = "http://example.org/fhir/CodeSystem/local";

/// One allergy composition whose substance is coded `L-1` in [`LOCAL`],
/// a system the eHealth Network guideline does not prefer.
fn locally_coded_allergy() -> Result<Held, Box<dyn Error>> {
    let uid = "7c4e1d20-0000-4000-8000-0000000000d1::cdr-a.example.org::1";
    let mut composition: serde_json::Value = serde_json::from_str(&eps::allergy_composition(
        uid,
        eps::TEMPLATE,
        "Synthetic substance",
    ))?;
    let substance = composition
        .pointer_mut("/content/0/data/items/0/value")
        .ok_or("the substance")?;
    *substance = serde_json::json!({
        "_type": "DV_CODED_TEXT",
        "value": "Synthetic substance",
        "defining_code": {
            "_type": "CODE_PHRASE",
            "terminology_id": { "_type": "TERMINOLOGY_ID", "value": LOCAL },
            "code_string": "L-1"
        }
    });
    Ok(Held {
        uid: uid.to_owned(),
        template: eps::TEMPLATE.to_owned(),
        composition: composition.to_string(),
    })
}

/// The summary of node A's [`locally_coded_allergy`], translated through
/// `terminology`.
async fn locally_coded_summary(terminology: &Terminology) -> Result<Assembled, Box<dyn Error>> {
    document_asking(
        &answers(&[locally_coded_allergy()?], &Answer::Answered(Vec::new())),
        (header(), &[]),
        (&Mappings::compile(&[allergies()?])?, terminology),
    )
    .await
}

/// The codings of the allergy's code in `bundle`, each as system and code.
fn allergy_codes(bundle: &serde_json::Value) -> Vec<(String, String)> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == "AllergyIntolerance")
        .flat_map(|resource| resource["code"]["coding"].as_array().into_iter().flatten())
        .filter_map(|coding| {
            Some((
                coding["system"].as_str()?.to_owned(),
                coding["code"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

/// The terminology answered by `server` under `/fhir`.
fn stub_terminology(
    server: &ferrofed_testkit::mock::Server,
) -> Result<Terminology, Box<dyn Error>> {
    let client = ferrofed_eehrxf::terminology::BridgeTerminology::new(
        &format!("{}/fhir", server.uri()),
        &ferrofed_identity::fhir::Authentication::None,
        &ferrofed_identity::fhir::Tls::default(),
        std::time::Duration::from_secs(5),
    )?;
    Ok(Terminology::server(std::sync::Arc::new(client)))
}

/// A `$translate` answer of FHIR JSON `body`.
fn translated(body: String) -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(200).set_body_raw(body, "application/fhir+json")
}

#[tokio::test]
async fn a_code_in_no_preferred_system_gains_its_equivalent_translation()
-> Result<(), Box<dyn Error>> {
    use wiremock::Mock;
    use wiremock::matchers::{body_string_contains, method, path};
    let server = ferrofed_testkit::mock::Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .and(body_string_contains(LOCAL))
        .and(body_string_contains("http://snomed.info/sct"))
        .respond_with(translated(format!(
            r#"{{"resourceType":"Parameters","parameter":[
              {{"name":"result","valueBoolean":true}},
              {{"name":"match","part":[{{"name":"equivalence","valueCode":"equivalent"}},{{"name":"concept","valueCoding":{{"system":"http://snomed.info/sct","code":"{MAPPED_CODE}"}}}}]}},
              {{"name":"match","part":[{{"name":"equivalence","valueCode":"wider"}},{{"name":"concept","valueCoding":{{"system":"http://snomed.info/sct","code":"105590001"}}}}]}}
            ]}}"#
        )))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(translated(String::from(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
        )))
        .with_priority(5)
        .mount(&server)
        .await;
    let assembled = locally_coded_summary(&stub_terminology(&server)?).await?;
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::profile::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        allergy_codes(&bundle),
        [
            (LOCAL.to_owned(), String::from("L-1")),
            (
                String::from("http://snomed.info/sct"),
                MAPPED_CODE.to_owned()
            ),
        ],
        "the original coding first, then the equivalent translation alone"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_translation_fails_the_summary() -> Result<(), Box<dyn Error>> {
    use wiremock::Mock;
    use wiremock::matchers::{method, path};
    let server = ferrofed_testkit::mock::Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(wiremock::ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let refused = locally_coded_summary(&stub_terminology(&server)?).await;
    let error = refused
        .err()
        .ok_or("a summary was written past a refusal")?;
    assert!(
        matches!(
            error.downcast_ref::<AssemblyError>(),
            Some(AssemblyError::Translation {
                source: TranslationError::Terminology(TerminologyError::Status {
                    operation: Operation::Translate,
                    status,
                }),
                ..
            }) if *status == http::StatusCode::SERVICE_UNAVAILABLE
        ),
        "a refusal is never taken as no translation: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn with_no_server_a_code_in_no_preferred_system_is_kept_as_recorded()
-> Result<(), Box<dyn Error>> {
    let assembled = locally_coded_summary(&Terminology::none()).await?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    assert_eq!(
        allergy_codes(&bundle),
        [(LOCAL.to_owned(), String::from("L-1"))],
        "eHN PS Art 10(7): transferred as recorded"
    );
    Ok(())
}
