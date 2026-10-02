// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `SELECT DISTINCT` through the façade (N13): each node receives the query
//! with nothing added to its projection, and the client receives every value
//! once, removed before the `LIMIT` and the `OFFSET` (AQL 1.1.0 §LIMIT), with
//! each endpoint's `row_count` what it contributed (§9.5).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::facade::{Answer, body, gateway, post, received, registry, schema};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// A node answering the one-column distinct query with `names`.
async fn node(names: &[&str]) -> MockServer {
    let rows: Vec<String> = names.iter().map(|name| format!("[\"{name}\"]")).collect();
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c/name/value"}}],"rows":[{}]}}"##,
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

fn row_counts(answer: &Answer) -> Vec<(&str, Option<u64>)> {
    answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|endpoint| (endpoint.id.as_str(), endpoint.row_count))
        .collect()
}

// conformance: CP-8 CP-32
#[tokio::test]
async fn a_duplicate_straddling_the_page_edge_is_answered_once() -> TestResult {
    let a = node(&["alpha", "beta", "gamma"]).await;
    let b = node(&["beta", "delta", "gamma"]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
    let query = "SELECT DISTINCT c/name/value FROM EHR e CONTAINS COMPOSITION c \
                 ORDER BY c/name/value LIMIT 2 OFFSET 1";

    let (status, text) = call(app, post(body(query)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![vec!["beta".to_owned()], vec!["delta".to_owned()]],
        answer.rows,
        "N13, §11.6.2: rows 1 and 2 of alpha, beta, delta, gamma, with beta once"
    );
    assert_eq!(
        vec![("node-a-pub", Some(3)), ("node-b-pub", Some(3))],
        row_counts(&answer),
        "§9.5: row_count is counted before the federation-level DISTINCT"
    );
    for server in [&a, &b] {
        let bodies = received(server).await?;
        let [sent] = bodies.as_slice() else {
            return Err(format!("each node is asked once: {bodies:?}").into());
        };
        assert!(
            sent.contains("SELECT DISTINCT c/name/value FROM EHR e CONTAINS COMPOSITION c"),
            "no column is added under DISTINCT: {sent}"
        );
        assert!(
            sent.contains("LIMIT 3"),
            "k + n rows reach the node: {sent}"
        );
        assert!(!sent.contains("OFFSET"), "§11.6.2: {sent}");
    }
    Ok(())
}

// conformance: CP-8
#[tokio::test]
async fn without_distinct_the_duplicate_is_answered_twice() -> TestResult {
    let a = node(&["alpha"]).await;
    let b = node(&["alpha"]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
    let query = "SELECT c/name/value FROM EHR e CONTAINS COMPOSITION c";

    let (status, text) = call(app, post(body(query)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![vec!["alpha".to_owned()], vec!["alpha".to_owned()]],
        answer.rows,
        "§10.1, N15: duplicates pass through unless the client asks for DISTINCT"
    );
    Ok(())
}
