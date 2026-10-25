// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Fixed ITI-83 answers for the offline suites.
//!
//! The offline suites test the gateway's side of PIXm: what it sends, how
//! often, and what it does with an answer, a refusal or a Manager that does
//! not answer. They need fixed answers, not a cross-reference, so
//! [`manager`] mounts one `wiremock` answer per patient on a
//! [`Server`]: a `Parameters` of `targetId` and `targetIdentifier`, ITI TF-2
//! §3.83.4.2.2.1 (Case 1), and `404` `not-found` for any other
//! `sourceIdentifier`, §3.83.4.2.2.2 (Case 2). It decides nothing: patient
//! identity is FerroPIX's, and the end-to-end lane resolves through the
//! pinned FerroPIX image ([`ferropix`](crate::containers::ferropix)). No
//! specification governs the stub: our own design.

use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::parameters::{Parameters, ParametersParameter, ParametersParameterValue};
use fhir_types::r4::reference::Reference;
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use crate::mock::Server;
use crate::seed::CrossReferenceSeed;

/// The path of the `$ihe-pix` operation under the stub's FHIR base.
pub const PATH: &str = "/fhir/Patient/$ihe-pix";

/// The FHIR JSON media type every answer carries.
const FHIR_JSON: &str = "application/fhir+json";

/// The priority of a patient's answer, ahead of the `not-found` default.
const KNOWN: u8 = 1;

/// The priority of the `not-found` answer every other request gets.
const UNKNOWN: u8 = 10;

/// Starts a stub Manager that answers ITI-83 for each patient of `known`
/// with the `ehr_id`s it names, by `POST` and by `GET`, and `404` for any
/// other patient.
pub async fn manager(known: &[CrossReferenceSeed]) -> Server {
    let server = Server::start().await;
    for (index, cross_reference) in known.iter().enumerate() {
        let patient = cross_reference.patient;
        let answer = ResponseTemplate::new(200)
            .set_body_raw(answer(index, cross_reference).into_bytes(), FHIR_JSON);
        Mock::given(method("POST"))
            .and(path(PATH))
            .and(body_string_contains(patient.value()))
            .respond_with(answer.clone())
            .with_priority(KNOWN)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(PATH))
            .and(query_param(
                "sourceIdentifier",
                format!("{}|{}", patient.namespace(), patient.value()),
            ))
            .respond_with(answer)
            .with_priority(KNOWN)
            .mount(&server)
            .await;
    }
    Mock::given(path(PATH))
        .respond_with(ResponseTemplate::new(404).set_body_raw(not_found().into_bytes(), FHIR_JSON))
        .with_priority(UNKNOWN)
        .mount(&server)
        .await;
    server
}

/// Returns the FHIR base of `server`, with its trailing slash, that a
/// `[[pixm.manager]] url` names.
#[must_use]
pub fn base_url(server: &Server) -> String {
    format!("{}/fhir/", server.uri())
}

/// Returns how many ITI-83 requests `server` received.
///
/// # Panics
///
/// Never: `wiremock` records every request unless recording is switched
/// off, and [`manager`] leaves it on.
#[expect(
    clippy::expect_used,
    reason = "a server started by Server::start records its requests, the wiremock default"
)]
pub async fn queries(server: &Server) -> usize {
    server
        .received_requests()
        .await
        .expect("the stub should record its requests")
        .iter()
        .filter(|request| request.url.path() == PATH)
        .count()
}

/// The `Parameters` answer for the patient `cross_reference` names, the
/// `index`-th the stub knows.
#[expect(
    clippy::expect_used,
    reason = "a typed Parameters of strings and identifiers always serialises"
)]
fn answer(index: usize, cross_reference: &CrossReferenceSeed) -> String {
    let mut parameter = vec![ParametersParameter {
        name: "targetId".into(),
        value: Some(ParametersParameterValue::Reference(Box::new(Reference {
            reference: Some(format!("Patient/stub-{index}").as_str().into()),
            ..Reference::default()
        }))),
        ..ParametersParameter::default()
    }];
    for (domain, ehr_id) in &cross_reference.ehrs {
        parameter.push(ParametersParameter {
            name: "targetIdentifier".into(),
            value: Some(ParametersParameterValue::Identifier(Box::new(Identifier {
                system: Some(domain.system().as_str().into()),
                value: Some(ehr_id.to_string().as_str().into()),
                ..Identifier::default()
            }))),
            ..ParametersParameter::default()
        });
    }
    serde_json::to_string(&Parameters {
        parameter,
        ..Parameters::default()
    })
    .expect("the answer should serialise")
}

/// The `not-found` outcome of ITI TF-2 §3.83.4.2.2.2.
#[expect(
    clippy::expect_used,
    reason = "a typed OperationOutcome of strings always serialises"
)]
fn not_found() -> String {
    serde_json::to_string(&OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "error".into(),
            code: "not-found".into(),
            diagnostics: Some("sourceIdentifier Patient Identifier not found".into()),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    })
    .expect("the outcome should serialise")
}
