// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An aggregate through the façade, against two mock nodes (§11.6.3, N14,
//! N39): with the default declaration every node is asked the aggregate,
//! `AVG` as its `SUM` and `COUNT`, scoped to its own `ehr_id` and carrying
//! no patient identifier (§5.4.1, N33), and the client receives one row in
//! its own columns. With nothing declared, the query is refused `400`, and a
//! request for `partial` is refused `400` before any node is asked.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use openehr_federation::headers::COMPLETENESS;
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, body, crossref, gateway, node_failing, post, received,
    registry, schema, wire,
};
use crate::support::{call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

const MAGNITUDE: &str = "o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude";

/// The façade query: the patient's composition count and mean magnitude.
fn query() -> String {
    format!(
        "SELECT COUNT(*) AS n, AVG({MAGNITUDE}) AS mean \
         FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// A node answering the dispatched `COUNT(*), SUM(x), COUNT(x)` with `row`.
async fn node(row: &str) -> Server {
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0"}},{{"name":"#1"}},{{"name":"#2"}}],"rows":[{row}]}}"##
    );
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// A gateway in `dir` resolving the patient at both nodes, with the
/// `[federation]` keys `federation`.
fn federation(
    dir: &tempfile::TempDir,
    a: &Server,
    b: &Server,
    federation: &str,
) -> Result<axum::Router, Box<dyn Error>> {
    let tables = format!(
        "{federation}\n{}",
        crossref(&[("node-a", EHR_A), ("node-b", EHR_B)])
    );
    gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &tables,
    )
}

#[derive(Debug, Deserialize)]
struct Recombined {
    columns: Vec<Column>,
    rows: Vec<(u64, f64)>,
}

#[derive(Debug, Deserialize)]
struct Column {
    name: String,
}

// conformance: CP-10 CP-32
#[tokio::test]
async fn the_client_receives_one_recombined_row_in_its_own_columns() -> TestResult {
    let a = node("[3, 12, 2]").await;
    let b = node("[2, 0.5, 1]").await;
    let dir = tempfile::tempdir()?;
    let app = federation(&dir, &a, &b, "")?;

    let (status, text) = call(app, post(body(&query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Recombined = serde_json::from_str(&text)?;
    let names: Vec<&str> = answer.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["n", "mean"], "N17, §9.2: the client's columns");
    assert_eq!(
        answer.rows,
        [(5, 12.5 / 3.0)],
        "§11.6.3: one row, never one per node"
    );
    for (server, ehr_id) in [(&a, EHR_A), (&b, EHR_B)] {
        let bodies = received(server).await?;
        let [sent] = bodies.as_slice() else {
            return Err(format!("each node is asked once: {bodies:?}").into());
        };
        assert!(
            sent.contains(&format!("SUM({MAGNITUDE}), COUNT({MAGNITUDE})"))
                && sent.contains(ehr_id),
            "AVG is asked as its SUM and COUNT, scoped to the node's ehr_id: {sent}"
        );
        assert!(
            !wire(server).await?.contains(PATIENT),
            "§5.4.1, N33: no node receives the patient identifier"
        );
    }
    Ok(())
}

// conformance: CP-10 CP-32
#[tokio::test]
async fn with_nothing_declared_the_aggregate_is_refused_400() -> TestResult {
    let a = node("[3, 12, 2]").await;
    let b = node("[2, 0.5, 1]").await;
    let dir = tempfile::tempdir()?;
    let app = federation(&dir, &a, &b, "decomposable_aggregates = []")?;

    let (status, text) = call(app, post(body(&query())?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!("undirected-aggregate", error_body(&text)?.code, "N14");
    for server in [&a, &b] {
        assert!(received(server).await?.is_empty(), "no node is asked");
    }
    Ok(())
}

// conformance: CP-10 CP-32
#[tokio::test]
async fn a_partial_request_for_a_recombined_aggregate_is_refused_400() -> TestResult {
    let a = node("[3, 12, 2]").await;
    let b = node("[2, 0.5, 1]").await;
    let dir = tempfile::tempdir()?;
    let app = federation(&dir, &a, &b, "")?;
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .header(COMPLETENESS, "partial")
        .body(Body::from(body(&query())?))?;

    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(
        "partial-aggregate",
        error_body(&text)?.code,
        "§11.6.3, §11.4"
    );
    assert!(!text.contains(PATIENT), "§5.4.3: {text}");
    for server in [&a, &b] {
        assert!(received(server).await?.is_empty(), "no node is asked");
    }
    Ok(())
}

// conformance: CP-10 CP-32
#[tokio::test]
async fn a_failing_node_fails_the_aggregate_424_with_no_row() -> TestResult {
    let a = node("[3, 12, 2]").await;
    let b = node_failing(500).await;
    let dir = tempfile::tempdir()?;
    let app = federation(&dir, &a, &b, "")?;

    let (status, text) = call(app, post(body(&query())?)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.4: {text}");
    schema::validate(&text)?;
    let answer: Recombined = serde_json::from_str(&text)?;
    assert!(
        answer.rows.is_empty(),
        "a node's count alone is never the answer"
    );
    Ok(())
}
