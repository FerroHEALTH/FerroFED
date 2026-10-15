// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The composition written to the receiving member by its own `ehr_id`
//! (§12.4, N23, §5.4.1, N33, CP-26), and the member's failures answered
//! as failures (§11).

use std::error::Error;

use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    EHR_SYSTEM, OTHER, OTHER_A, OTHER_NAMESPACE, SUBSTANCE, VERSION_A, bound, creating, document,
    gateway, gateway_with, issue, post, receiving, rows, untouched,
};
use crate::auth::{bearing, claims, minted};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, received};
use crate::fhir::TestResult;
use crate::support::call;

/// Every request target and header `server` received, the bodies left out.
pub(super) async fn composed(server: &Server) -> Result<String, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut text = String::new();
    for request in requests {
        text.push_str(request.url.as_str());
        for (name, value) in &request.headers {
            text.push_str(name.as_str());
            text.push_str(value.to_str().unwrap_or("<opaque>"));
        }
    }
    Ok(text)
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_document_is_written_to_its_member_by_its_ehr_id_alone() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("informational", code);
    for named in ["node-a", "node-a-pub", VERSION_A, "Patient-Summaries"] {
        assert!(diagnostics.contains(named), "{named}: {diagnostics}");
    }
    let requests = a.received_requests().await.ok_or("recording is on")?;
    assert_eq!(1, requests.len(), "one write");
    let write = requests.first().ok_or("the write")?;
    assert_eq!(write.url.path(), format!("/v1/ehr/{EHR_A}/composition"));
    assert_eq!(write.url.query(), None, "N33: no query string");
    let wire = composed(&a).await?;
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !wire.contains(withheld),
            "N33: no identifier in the path or the headers: {wire}"
        );
    }
    let body = String::from_utf8(write.body.clone())?;
    assert!(body.contains(SUBSTANCE), "the mapped content: {body}");
    assert!(
        body.contains("original_content") && body.contains(PATIENT),
        "the received document is kept whole in the composition: {body}"
    );
    assert!(
        received(&b).await?.is_empty(),
        "N23: no other member is asked"
    );
    Ok(())
}

#[tokio::test]
async fn a_caller_confined_to_one_patient_writes_only_into_its_ehr() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (EHR_SYSTEM, EHR_A, "node-a", EHR_A),
        (OTHER_NAMESPACE, OTHER, "node-a", OTHER_A),
    ]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (
            &crossref,
            &receiving("node-a", &[NAMESPACE, OTHER_NAMESPACE]),
        ),
        &bound()?,
    )?;
    let mut granted = claims();
    granted.scope = Some(String::from("patient/composition-*.c"));
    granted.ehr_id = Some(EHR_A.to_owned());
    let token = minted(&granted)?;
    let other = eps::received_document(&[(OTHER_NAMESPACE, OTHER)], SUBSTANCE);
    let (status, text) = call(app.clone(), bearing(post(&other)?, &token)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "§5.2: {text}");
    assert!(
        untouched(&a, &b).await?,
        "nothing is written for another patient"
    );
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "the token's own patient: {text}");
    assert_eq!(1, received(&a).await?.len());
    Ok(())
}

#[tokio::test]
async fn a_member_that_refuses_the_composition_is_named_with_its_status() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(ResponseTemplate::new(422).set_body_raw(
            br#"{"message":"synthetic validation failure"}"#.to_vec(),
            "application/json",
        ))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(
        StatusCode::BAD_GATEWAY,
        status,
        "§11: never a success: {text}"
    );
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("processing", code);
    assert!(
        diagnostics.contains("422") && diagnostics.contains("node-a-pub"),
        "{diagnostics}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_that_names_no_version_is_no_success() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(ResponseTemplate::new(201))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "§11: {text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("exception", code);
    assert!(diagnostics.contains("node-a-pub"), "{diagnostics}");
    Ok(())
}

#[tokio::test]
async fn a_member_that_does_not_answer_is_a_time_out() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(ResponseTemplate::new(201).set_delay(std::time::Duration::from_secs(5)))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "{text}");
    assert_eq!("timeout", issue(&text)?.0);
    Ok(())
}
