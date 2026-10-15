// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A coding received with no display, refused with no terminology server
//! and given its display by one.

use std::error::Error;

use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{creating, document, gateway, gateway_with, issue, post, receiving, rows, untouched};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, settings_with_room};
use crate::fhir::TestResult;
use crate::support::call;

/// The document with the display of its Problems section code taken out.
fn undisplayed() -> Result<String, Box<dyn Error>> {
    let mut value: Value = serde_json::from_str(&document())?;
    value
        .pointer_mut("/entry/0/resource/section/0/code/coding/0")
        .and_then(Value::as_object_mut)
        .and_then(|coding| coding.remove("display"))
        .ok_or("the Problems section code carries a display")?;
    Ok(value.to_string())
}

#[tokio::test]
async fn a_coding_with_no_display_is_refused_with_no_terminology_server() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&undisplayed()?)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("processing", code);
    assert!(diagnostics.contains("no display"), "{diagnostics}");
    assert!(untouched(&a, &b).await?, "nothing reaches a member");
    Ok(())
}

/// A terminology server that answers `$lookup` with a display and admits
/// every concept `$validate-code` is asked about.
async fn terminology_server() -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/CodeSystem/$lookup"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"Parameters","parameter":[{"name":"name","valueString":"Synthetic"},{"name":"display","valueString":"Synthetic problems"}]}"#,
            "application/fhir+json",
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/fhir/ValueSet/$validate-code"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":true}]}"#,
            "application/fhir+json",
        ))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_terminology_server_gives_the_missing_display() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let terminology = terminology_server().await;
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let fhir = format!(
        "{}\n[eehrxf.terminology]\nurl = \"{}/fhir\"\n",
        receiving("node-a", &[NAMESPACE]),
        terminology.uri()
    );
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &fhir),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, post(&undisplayed()?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let sent = terminology
        .received_requests()
        .await
        .ok_or("recording is on")?;
    let bodies: String = sent
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect();
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !bodies.contains(withheld),
            "the terminology server is sent codes alone: {bodies}"
        );
    }
    Ok(())
}
