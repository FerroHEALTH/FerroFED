// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ORDER BY` with `LIMIT` through the façade (§11.6.1, N13, N39): each node
//! is asked for the key as a hidden column, the uid as the last key and the
//! client's `LIMIT n`, and the client receives the global top `n` with only
//! the columns it selected (decisions A28 and A43).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::facade::{Answer, body, gateway, post, received, registry, schema, statuses};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The façade query: one selected column, ordered on one it does not select.
const QUERY: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
                     ORDER BY c/context/start_time/value DESC LIMIT 2";

/// A node answering the pushed query with `(uid, start_time)` rows.
async fn node(rows: &[(&str, &str)]) -> MockServer {
    let rows: Vec<String> = rows
        .iter()
        .map(|(uid, time)| format!("[\"{uid}\",\"{time}\"]"))
        .collect();
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}},{{"name":"#1","path":"c/context/start_time/value"}}],"rows":[{}]}}"##,
        rows.join(",")
    );
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

// conformance: CP-8 CP-32
#[tokio::test]
async fn the_client_gets_the_global_top_n_without_the_hidden_column() -> TestResult {
    let a = node(&[
        ("a1", "2026-01-03T00:00:00Z"),
        ("a2", "2026-01-01T00:00:00Z"),
    ])
    .await;
    let b = node(&[("b1", "2026-01-02T00:00:00Z")]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;

    let (status, text) = call(app, post(body(QUERY)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![vec!["a1".to_owned()], vec!["b1".to_owned()]],
        answer.rows,
        "N39: the latest two across both nodes, and only the selected column (A28)"
    );
    for server in [&a, &b] {
        let bodies = received(server).await?;
        let [sent] = bodies.as_slice() else {
            return Err(format!("each node is asked once: {bodies:?}").into());
        };
        assert!(
            sent.contains(
                "SELECT c/uid/value, c/context/start_time/value FROM EHR e CONTAINS COMPOSITION c \
                 ORDER BY c/context/start_time/value DESC, c/uid/value ASC LIMIT 2"
            ),
            "the hidden key, the uid tie-break and the client's LIMIT 2 reach the node: {sent}"
        );
    }
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn a_node_out_of_the_federation_order_fails_the_query_424() -> TestResult {
    let a = node(&[
        ("a1", "2026-01-01T00:00:00Z"),
        ("a2", "2026-01-03T00:00:00Z"),
    ])
    .await;
    let b = node(&[("b1", "2026-01-02T00:00:00Z")]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;

    let (status, text) = call(app, post(body(QUERY)?)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "N37, A43: {text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(answer.rows.is_empty(), "a failing query returns no rows");
    assert_eq!(
        vec![("node-a-pub", "node-error"), ("node-b-pub", "active")],
        statuses(&answer),
        "§11.1: an answer the gateway could not use"
    );
    assert!(
        text.contains("result order disagrees with the federation order"),
        "{text}"
    );
    Ok(())
}
