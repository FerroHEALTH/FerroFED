// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[audit] destination = "log"` names no patient (§5.4, N33): a sentinel
//! identifier run through ITI-83, ITI-90 and ITI-93 reaches no captured
//! tracing output, and each record is logged at the `ferrofed::audit` target
//! by its transaction code, action and outcome alone.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mcsd::HarnessDirectory;
use ferrofed_testkit::pmir::merge_message;
use http::{Request, StatusCode, header};

use crate::facade::{PATIENT, body, gateway, node_answering, patient_query, post, registry};
use crate::pmir::{PATH, TOKEN, text};
use crate::registry_mcsd::{Gateway, config, members};
use crate::support::{Logs, call, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// The `[audit]` table that writes every record to the log.
const LOG: &str = "\n[audit]\ndestination = \"log\"\n";

/// A sentinel identity value, unlike any other text the gateway writes.
const SENTINEL: &str = "Qz7-SENTINEL-4714";

/// Captures every thread's tracing output for the rest of the test process,
/// which nextest runs alone: a directory read runs on a thread of its own.
fn captured() -> Result<Logs, Box<dyn Error>> {
    let logs = Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    )?;
    tracing::subscriber::set_global_default(capture)?;
    Ok(logs)
}

/// The audit records of `log` for the transaction `code`.
fn records<'a>(log: &'a str, code: &str) -> Vec<&'a str> {
    log.lines()
        .filter(|line| line.contains("BALP audit record") && line.contains(code))
        .collect()
}

#[tokio::test]
async fn an_iti_83_record_in_the_log_names_no_patient() -> TestResult {
    let logs = captured()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = ferrofed_testkit::mock::Server::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(r#"{"resourceType":"Parameters"}"#, "application/fhir+json"),
        )
        .mount(&pix)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &format!(
            "[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"urn:oid:2.999.10\"\n\"node-b\" = \"urn:oid:2.999.20\"\n{LOG}",
            pix.uri()
        ),
    )?;
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let log = logs.text();
    let [record] = <[&str; 1]>::try_from(records(&log, "ITI-83"))
        .map_err(|_all| format!("one ITI-83 record: {log}"))?;
    assert!(record.contains("ferrofed::audit"), "{record}");
    assert!(record.contains("\"outcome\":\"0\""), "{record}");
    assert!(
        !log.contains(PATIENT),
        "no source identifier in the log: {log}"
    );
    assert!(
        !log.contains("sourceIdentifier"),
        "no query parameter in the log: {log}"
    );
    Ok(())
}

#[tokio::test]
async fn iti_90_records_in_the_log_name_no_query_parameter() -> TestResult {
    let logs = captured()?;
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let _gateway = Gateway::boot_from(&format!("{}{LOG}", config(&harness.base(), 5_000)))?;
    let log = logs.text();
    assert_eq!(2, records(&log, "ITI-90").len(), "{log}");
    for asked in harness.requests().await {
        let Some((_, query)) = asked.split_once('?') else {
            continue;
        };
        assert!(
            !log.contains(query),
            "the search's query parameters are not logged: {log}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_iti_93_record_in_the_log_names_no_patient_master_identity() -> TestResult {
    let logs = captured()?;
    let dir = tempfile::tempdir()?;
    let gateway = crate::pmir::gateway(&text(
        dir.path(),
        "http://127.0.0.1:9/fhir/",
        "http://127.0.0.1:9/pmir/feed",
        LOG,
    )?)?;
    let body = merge_message(SENTINEL, &[("urn:oid:2.999.1.999", SENTINEL)], SENTINEL)?;
    let request = Request::post(PATH)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::from(body))?;
    let response = send_as_is(gateway.app.clone(), request).await?;
    assert_eq!(StatusCode::OK, response.status());
    let log = logs.text();
    let [record] = <[&str; 1]>::try_from(records(&log, "ITI-93"))
        .map_err(|_all| format!("one ITI-93 record: {log}"))?;
    assert!(
        record.contains("\"patients\":2"),
        "counted, not named: {record}"
    );
    assert!(!log.contains(SENTINEL), "no identity in the log: {log}");
    Ok(())
}
