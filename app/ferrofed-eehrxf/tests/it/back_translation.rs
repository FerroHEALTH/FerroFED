// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received dispensation's codes translated back into the code systems
//! its owner records (Implementing Regulation (EU) 2026/2083 Art 3(4)): a
//! medicine coded in ATC gains its SNOMED CT coding after the recorded one,
//! a concept no exact translation reaches is refused by its element path
//! and its code systems, and a server that refuses fails the translation.

use std::error::Error;
use std::sync::Arc;

use eehrxf::terminology::Terminology;
use ferrofed_eehrxf::back_translation::{Untranslated, translated};
use ferrofed_testkit::dispensation_codes::{self, ATC, RECORDED, SENT, SNOMED};
use ferrofed_testkit::mock::Server;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// The terminology `server` answers under `/fhir`.
fn asking(server: &Server) -> Result<Terminology, Box<dyn Error>> {
    let endpoint = serde_json::from_value(serde_json::json!({
        "url": format!("{}/fhir", server.uri()),
        "timeout": "5s",
        "retry": { "max_attempts": 1 },
    }))?;
    let client = ferrofed_eehrxf::terminology::BridgeTerminology::from_endpoint(
        &endpoint,
        "eehrxf.terminology.endpoint",
    )?;
    Ok(Terminology::server(Arc::new(client)))
}

/// A synthetic dispensation whose medicine is `medication`, with `contained`
/// beside its patient.
fn dispensation(medication: &str, contained: &str) -> String {
    format!(
        r##"{{"resourceType":"MedicationDispense","contained":[{{"resourceType":"Patient","id":"pat","identifier":[{{"system":"urn:oid:2.999.1","value":"synthetic-patient-0001"}}]}}{contained}],
  "identifier":[{{"system":"urn:oid:2.999.6.1","value":"SYNTHETIC-MD-1"}}],
  "status":"completed","medicationCodeableConcept":{medication},
  "subject":{{"reference":"#pat"}},
  "authorizingPrescription":[{{"identifier":{{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-1"}}}}],
  "quantity":{{"value":30,"unit":"tablet"}}}}"##
    )
}

/// The codings of the concept at `pointer` in `text`, each as system and
/// code.
fn codings(text: &str, pointer: &str) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let value: Value = serde_json::from_str(text)?;
    Ok(value
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|coding| {
            Some((
                coding.get("system")?.as_str()?.to_owned(),
                coding.get("code")?.as_str()?.to_owned(),
            ))
        })
        .collect())
}

#[tokio::test]
async fn a_medicine_coded_in_atc_gains_its_snomed_ct_coding_after_the_recorded_one() -> TestResult {
    let server = dispensation_codes::start().await;
    let text = dispensation(&dispensation_codes::medication(), "");
    let written = translated(&text, &asking(&server)?, &[SNOMED])
        .await?
        .ok_or("a coding is added")?;
    assert_eq!(
        vec![
            (ATC.to_owned(), SENT.0.to_owned()),
            (SNOMED.to_owned(), RECORDED.0.to_owned())
        ],
        codings(&written, "/medicationCodeableConcept/coding")?
    );
    let (sent, kept): (Value, Value) = (
        serde_json::from_str(&text)?,
        serde_json::from_str(&written)?,
    );
    assert_eq!(
        sent.get("identifier"),
        kept.get("identifier"),
        "the business identifier stays"
    );
    Ok(())
}

#[tokio::test]
async fn a_contained_medication_is_translated_as_the_dispensation_is() -> TestResult {
    let server = dispensation_codes::start().await;
    let contained = format!(
        r#",{{"resourceType":"Medication","id":"med","code":{}}}"#,
        dispensation_codes::medication()
    );
    let text = dispensation(r#"{"text":"Synthetic medicine A"}"#, &contained);
    let written = translated(&text, &asking(&server)?, &[SNOMED])
        .await?
        .ok_or("a coding is added")?;
    assert_eq!(
        Some(&(SNOMED.to_owned(), RECORDED.0.to_owned())),
        codings(&written, "/contained/1/code/coding")?.last()
    );
    Ok(())
}

#[tokio::test]
async fn a_dispensation_already_in_the_owners_systems_travels_as_it_came() -> TestResult {
    let server = dispensation_codes::start().await;
    let text = dispensation(&dispensation_codes::medication(), "");
    assert_eq!(
        None,
        translated(&text, &asking(&server)?, &[ATC, SNOMED]).await?
    );
    assert!(
        ferrofed_testkit::mock::recorded(&server).await?.is_empty(),
        "nothing is asked"
    );
    Ok(())
}

#[tokio::test]
async fn a_concept_no_exact_translation_reaches_is_refused_by_its_path_and_system() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
            "application/fhir+json",
        ))
        .mount(&server)
        .await;
    let text = dispensation(&dispensation_codes::medication(), "");
    let refused = translated(&text, &asking(&server)?, &[SNOMED]).await;
    let Err(Untranslated::Unreached { location, systems }) = &refused else {
        return Err(format!("refused as unreached: {refused:?}").into());
    };
    assert_eq!("MedicationDispense.medicationCodeableConcept", location);
    assert_eq!(&vec![ATC.to_owned()], systems);
    let text = refused
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(!text.contains(SENT.0), "no code is quoted: {text}");
    Ok(())
}

#[tokio::test]
async fn a_server_that_refuses_fails_the_translation() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let text = dispensation(&dispensation_codes::medication(), "");
    let refused = translated(&text, &asking(&server)?, &[SNOMED]).await;
    assert!(
        matches!(refused, Err(Untranslated::Terminology(_))),
        "{refused:?}"
    );
    Ok(())
}
