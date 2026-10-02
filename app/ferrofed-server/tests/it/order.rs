// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ORDER BY` with `LIMIT` through the façade (§11.6.1, N13, N39): each node
//! is asked for the key as a hidden column, the uid as the last key and the
//! client's `LIMIT n`, and the client receives the global top `n` with only
//! the columns it selected (decisions A28 and A43). A page at `OFFSET k` asks
//! each node for `k + n` rows within the configured bound, or is refused under
//! the reject strategy (§11.6.2).
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

/// The façade query of the page tests: rows 1 and 2 of the latest first.
const PAGE: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
                    ORDER BY c/context/start_time/value DESC LIMIT 2 OFFSET 1";

// conformance: CP-32
#[tokio::test]
async fn a_bounded_offset_page_asks_each_node_for_k_plus_n_rows() -> TestResult {
    let a = node(&[
        ("a1", "2026-01-05T00:00:00Z"),
        ("a2", "2026-01-03T00:00:00Z"),
        ("a3", "2026-01-01T00:00:00Z"),
    ])
    .await;
    let b = node(&[
        ("b1", "2026-01-04T00:00:00Z"),
        ("b2", "2026-01-02T00:00:00Z"),
    ])
    .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;

    let (status, text) = call(app, post(body(PAGE)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![vec!["b1".to_owned()], vec!["a2".to_owned()]],
        answer.rows,
        "§11.6.2: rows 1 and 2 of the merged order, by default under the bounded strategy"
    );
    for server in [&a, &b] {
        let bodies = received(server).await?;
        let [sent] = bodies.as_slice() else {
            return Err(format!("each node is asked once: {bodies:?}").into());
        };
        assert!(
            sent.contains("c/uid/value ASC LIMIT 3"),
            "k + n rows reach the node: {sent}"
        );
        assert!(
            !sent.contains("OFFSET"),
            "§11.6.2: OFFSET is never pushed down: {sent}"
        );
    }
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn a_page_past_the_configured_window_is_refused_400_naming_the_bound() -> TestResult {
    let a = node(&[]).await;
    let b = node(&[]).await;
    let dir = tempfile::tempdir()?;
    // The key extends the gateway's `[federation]` table.
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "",
        "max_offset_window = 2",
    )?;

    let (status, text) = call(app, post(body(PAGE)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert!(
        text.contains("past this gateway's bound of 2 rows per node"),
        "§11.6.2: it MUST reject when it cannot bound k + n: {text}"
    );
    for server in [&a, &b] {
        assert!(received(server).await?.is_empty(), "no node is asked");
    }
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn the_reject_strategy_refuses_any_offset_past_zero_400() -> TestResult {
    let a = node(&[]).await;
    let b = node(&[]).await;
    let dir = tempfile::tempdir()?;
    // The key extends the gateway's `[federation]` table.
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "",
        "offset_strategy = \"reject\"",
    )?;

    let (status, text) = call(app, post(body(PAGE)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert!(
        text.contains("offset-based paging is not supported across a fan-out"),
        "§11.6.2 option 1: {text}"
    );
    for server in [&a, &b] {
        assert!(received(server).await?.is_empty(), "no node is asked");
    }
    Ok(())
}
