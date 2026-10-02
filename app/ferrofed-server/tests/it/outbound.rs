// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! No identifier leaks into the composed query (§5.4.1, N33, CP-26), asserted
//! on what each mock node received: the patient named through either carrier,
//! in a projection, or sent by the client in its own query string and
//! headers, reaches a node as the node's `ehr_id` and nothing else; a query
//! that cannot be brought into that state is a `400` that asks nobody; and
//! every strip and refusal is a security event that names no value (§5.4.3).

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode, header};

use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, body, dev_gateway, gateway, node_answering, post, received,
    registry, wire,
};
use crate::request_log::logged;
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The façade queries naming the patient: each carrier on its own, both at
/// once, and the identifier selected back for re-injection.
fn carriers() -> Vec<(&'static str, String)> {
    let external = format!(
        "e/ehr_status/subject/external_ref/id/value = '{PATIENT}' AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let entry = format!(
        "o/subject/identifiers/id = '{PATIENT}' AND o/subject/identifiers/issuer = '{NAMESPACE}'"
    );
    let from = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
    vec![
        (
            "external_ref",
            format!("SELECT c/uid/value {from} WHERE {external}"),
        ),
        (
            "ENTRY subject",
            format!("SELECT c/uid/value {from} WHERE {entry}"),
        ),
        (
            "both carriers",
            format!("SELECT c/uid/value {from} WHERE {external} AND {entry}"),
        ),
        (
            "a projection",
            format!(
                "SELECT e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value {from} WHERE {external}"
            ),
        ),
    ]
}

/// Asserts that `server` was asked once, keyed on `own`, with no trace of the
/// patient identifier anywhere in what it received.
async fn asked_by_ehr_id_alone(server: &wiremock::MockServer, own: &str, case: &str) -> TestResult {
    let bodies = received(server).await?;
    assert_eq!(1, bodies.len(), "{case}: each node is asked once");
    let sent = bodies.first().ok_or("one request")?;
    assert!(
        sent.contains(&format!("e/ehr_id/value='{own}'")),
        "{case}: the node query is keyed on the node's own ehr_id: {sent}"
    );
    assert!(
        !sent.contains("ehr_status/subject") && !sent.contains("subject/identifiers"),
        "{case}: no patient carrier reaches a node: {sent}"
    );
    let all = wire(server).await?;
    assert!(
        !all.contains(PATIENT),
        "{case}: the identifier reaches no request line, header or body: {all}"
    );
    Ok(())
}

// conformance: CP-26
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn either_carrier_and_a_projection_reach_a_node_as_its_ehr_id_alone() -> TestResult {
    for (case, aql) in carriers() {
        let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
        let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
        let dir = tempfile::tempdir()?;
        let app = dev_gateway(
            dir.path(),
            &a.uri(),
            &b.uri(),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
        )?;
        let (status, text) = call(app, post(body(&aql)?)?).await?;
        assert_eq!(StatusCode::OK, status, "{case}: {text}");
        asked_by_ehr_id_alone(&a, EHR_A, case).await?;
        asked_by_ehr_id_alone(&b, EHR_B, case).await?;
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn an_identifier_in_the_clients_query_string_and_headers_is_never_forwarded() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let (_, aql) = carriers().into_iter().next().ok_or("one carrier")?;
    let request = Request::post(format!("/v1/query/aql?patient={PATIENT}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-patient", PATIENT)
        .header("x-request-id", "req-outbound-1")
        .body(Body::from(body(&aql)?))?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for (server, own) in [(&a, EHR_A), (&b, EHR_B)] {
        asked_by_ehr_id_alone(server, own, "client query string and headers").await?;
        let all = wire(server).await?;
        assert!(
            !all.to_ascii_lowercase().contains("x-patient"),
            "a client header is never forwarded to a node: {all}"
        );
    }
    Ok(())
}

// conformance: CP-26 CP-38
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_clinician_predicate_reaches_the_node_unchanged() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let aql = patient_and("c/composer/identifiers/id = 'clinician-77'");
    let (status, text) = call(app, post(body(&aql)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for server in [&a, &b] {
        let bodies = received(server).await?;
        let sent = bodies.first().ok_or("one request")?;
        assert!(
            sent.contains("c/composer/identifiers/id='clinician-77'"),
            "the clinician predicate is dispatched as written (§5.4.3): {sent}"
        );
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn the_identifier_on_another_path_is_a_400_that_asks_nobody() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let aql = patient_and(&format!("c/composer/identifiers/id = '{PATIENT}'"));
    let (status, text) = call(app, post(body(&aql)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert!(
        text.contains("appears elsewhere in the query"),
        "refused for the identifier on another path, not for something else: {text}"
    );
    assert!(!text.contains(PATIENT), "the 400 quotes nothing: {text}");
    for server in [&a, &b] {
        assert!(received(server).await?.is_empty(), "no node is asked");
    }
    Ok(())
}

/// A short identifier that occurs, by chance, inside [`EHR_A`].
const INSIDE_EHR_A: &str = "2222";

// conformance: CP-26
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_short_identifier_inside_the_nodes_ehr_id_is_answered() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let crossref = format!(
        "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{INSIDE_EHR_A}\"\nmember = \"node-a\"\nehr_id = \"{EHR_A}\"\n"
    );
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &crossref,
    )?;
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{INSIDE_EHR_A}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let (status, text) = call(app, post(body(&aql)?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the gate reads past the scope: {text}"
    );
    let bodies = received(&a).await?;
    assert_eq!(1, bodies.len(), "node A is asked once");
    let sent = bodies.first().ok_or("one request")?;
    assert!(
        sent.contains(&format!("e/ehr_id/value='{EHR_A}'")),
        "the node query is keyed on the node's own ehr_id: {sent}"
    );
    assert!(
        !sent.contains("ehr_status/subject"),
        "no patient carrier reaches the node: {sent}"
    );
    Ok(())
}

/// A façade query naming the patient with its namespace, with `extra` joined
/// by `AND`.
fn patient_and(extra: &str) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}' AND {extra}"
    )
}

/// The log lines of the security target in `text`.
fn security_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.contains("ferrofed::security"))
        .collect()
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn strips_and_refusals_are_security_events_that_name_no_value() -> TestResult {
    let dir = tempfile::tempdir()?;
    // NOTE: no node listens on ports 1 and 9, so the patient query fails after
    // its strip is logged; the events are what this test reads.
    let app = dev_gateway(
        dir.path(),
        "http://127.0.0.1:9",
        "http://127.0.0.1:1",
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let (_, stripped) = carriers().into_iter().next().ok_or("one carrier")?;
    let refused = patient_and(&format!("c/composer/identifiers/id = '{PATIENT}'"));
    let mut requests = vec![post(body(&stripped)?)?, post(body(&refused)?)?];
    for request in &mut requests {
        // NOTE: §5.4.3, the client's own request id names the patient here, and
        // an event carries the gateway's id, so the value still reaches no line.
        request
            .headers_mut()
            .insert("x-request-id", http::HeaderValue::from_static(PATIENT));
    }
    let text = logged(&app, "trace", requests)?;
    let events = security_lines(&text);
    let strips: Vec<&&str> = events
        .iter()
        .filter(|line| line.contains("patient-predicate-stripped"))
        .collect();
    assert_eq!(
        2,
        strips.len(),
        "one strip event per consumed predicate, the identifier and its namespace: {text}"
    );
    assert!(
        strips
            .iter()
            .all(|line| line.contains("\"at\":\"") && !line.contains("unknown")),
        "every strip is located by byte range: {text}"
    );
    let refusals: Vec<&&str> = events
        .iter()
        .filter(|line| line.contains("aql-refused"))
        .collect();
    assert_eq!(1, refusals.len(), "the refusal is one event: {text}");
    assert!(
        refusals
            .iter()
            .all(|line| line.contains("identifier-elsewhere") && line.contains("\"at\":\"")),
        "the refusal names its kind and position: {text}"
    );
    assert!(
        !text.contains(PATIENT),
        "no log line at any level carries the identifier: {text}"
    );
    Ok(())
}
