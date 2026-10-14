// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology server over HTTP, against a stub server: each operation
//! posted as its `Parameters` to its type-level path, the credential sent,
//! the answer read as the operation's `Parameters`, and a refusal, a
//! time-out and an answer that does not read each a typed error naming the
//! operation (FHIR R4 `$translate`, `$lookup`, `$validate-code`).

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use eehrxf::terminology::Candidate;
use eehrxf::terminology::Operation;
use eehrxf::terminology::Terminology;
use eehrxf::terminology::TerminologyError;
use ferrofed_eehrxf::terminology::FhirTerminology;
use ferrofed_identity::fhir::Authentication;
use ferrofed_identity::fhir::Tls;
use ferrofed_testkit::mock::Server;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Uri;
use http::StatusCode;
use secrecy::SecretString;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

type TestResult = Result<(), Box<dyn Error>>;

const LOCAL: &str = "http://example.org/fhir/CodeSystem/local";
const PREFERRED: &str = "http://example.org/fhir/CodeSystem/preferred";
const FHIR_JSON: &str = "application/fhir+json";

/// The terminology answered by `server`, under `/fhir`, with a bearer token.
fn terminology(server: &Server, timeout: Duration) -> Result<Terminology, Box<dyn Error>> {
    let client = FhirTerminology::new(
        &format!("{}/fhir", server.uri()),
        &Authentication::Bearer(SecretString::from("synthetic-token")),
        &Tls::default(),
        timeout,
    )?;
    Ok(Terminology::server(Arc::new(client)))
}

fn answer(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_owned(), FHIR_JSON)
}

#[tokio::test]
async fn a_translation_posts_its_parameters_and_keeps_only_equivalent_matches() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("content-type", FHIR_JSON))
        .and(body_string_contains("targetsystem"))
        .respond_with(answer(&format!(
            r#"{{"resourceType":"Parameters","parameter":[
              {{"name":"result","valueBoolean":true}},
              {{"name":"match","part":[{{"name":"equivalence","valueCode":"equivalent"}},{{"name":"concept","valueCoding":{{"system":"{PREFERRED}","code":"1001"}}}}]}},
              {{"name":"match","part":[{{"name":"equivalence","valueCode":"wider"}},{{"name":"concept","valueCoding":{{"system":"{PREFERRED}","code":"1002"}}}}]}}
            ]}}"#
        )))
        .expect(1)
        .mount(&server)
        .await;
    let found = terminology(&server, Duration::from_secs(5))?
        .translate(LOCAL, "L-1", PREFERRED)
        .await?;
    let codes: Vec<Option<String>> = found
        .iter()
        .map(|coding| coding.code.as_ref().and_then(|code| code.value.clone()))
        .collect();
    assert_eq!(codes, [Some(String::from("1001"))]);
    Ok(())
}

#[tokio::test]
async fn a_lookup_answers_the_display() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/CodeSystem/$lookup"))
        .respond_with(answer(
            r#"{"resourceType":"Parameters","parameter":[{"name":"name","valueString":"Synthetic"},{"name":"display","valueString":"Synthetic term"}]}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let display = terminology(&server, Duration::from_secs(5))?
        .display(LOCAL, "L-1")
        .await?;
    assert_eq!(display, "Synthetic term");
    Ok(())
}

#[tokio::test]
async fn a_validation_answers_its_result() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/ValueSet/$validate-code"))
        .respond_with(answer(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let coding = Coding {
        system: Some(Uri::from(LOCAL)),
        code: Some(Code::from("L-1")),
        ..Coding::default()
    };
    let valid = terminology(&server, Duration::from_secs(5))?
        .validate(
            "http://example.org/fhir/ValueSet/synthetic",
            &Candidate::Coding(Box::new(coding)),
        )
        .await?;
    assert!(!valid);
    Ok(())
}

#[tokio::test]
async fn a_refusal_carries_the_status() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let refused = terminology(&server, Duration::from_secs(5))?
        .display(LOCAL, "L-1")
        .await;
    assert!(
        matches!(
            refused,
            Err(TerminologyError::Status {
                operation: Operation::Lookup,
                status: StatusCode::SERVICE_UNAVAILABLE,
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_slow_server_is_a_time_out() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .respond_with(
            answer(r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":true}]}"#)
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;
    let refused = terminology(&server, Duration::from_millis(100))?
        .translate(LOCAL, "L-1", PREFERRED)
        .await;
    assert!(
        matches!(
            refused,
            Err(TerminologyError::Timeout {
                operation: Operation::Translate
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_answer_that_is_no_parameters_is_refused() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .respond_with(answer(r#"{"resourceType":"OperationOutcome","issue":[]}"#))
        .mount(&server)
        .await;
    let refused = terminology(&server, Duration::from_secs(5))?
        .display(LOCAL, "L-1")
        .await;
    assert!(
        matches!(
            refused,
            Err(TerminologyError::Answer {
                operation: Operation::Lookup,
                ..
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_lookup_with_no_display_is_refused() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .respond_with(answer(
            r#"{"resourceType":"Parameters","parameter":[{"name":"name","valueString":"Synthetic"}]}"#,
        ))
        .mount(&server)
        .await;
    let refused = terminology(&server, Duration::from_secs(5))?
        .display(LOCAL, "L-1")
        .await;
    assert!(
        matches!(refused, Err(TerminologyError::Answer { .. })),
        "the display is 1..1: {refused:?}"
    );
    Ok(())
}
