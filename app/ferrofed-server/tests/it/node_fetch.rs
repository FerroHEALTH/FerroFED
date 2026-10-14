// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The explicit `fetch` every node query carries, and the ITS-REST `offset`
//! and `fetch` members as a window over the AQL result (§11.5, §11.6, N39).
//!
//! ITS-REST leaves the rows a node answers without `fetch` to the node
//! ("the default depends on the implementation", Query API `Fetch`), so the
//! gateway sends each node the `LIMIT` its AQL carries as `fetch`, or the
//! configured `federation.max_node_rows` plus one, and a node that answers
//! more rows than that ceiling is `node-error`: its share may be cut, which
//! §11.5 forbids reading as complete. No specification governs the ceiling
//! itself: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use openehr_federation::headers::COMPLETENESS;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{Answer, body, gateway, post, received, registry, schema, statuses};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// A query with no `LIMIT`: every node is asked for every row it matches.
const UNBOUNDED: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";

/// The ordered façade query of the window tests.
const ORDERED: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
                       ORDER BY c/context/start_time/value DESC";

/// A node answering every query with `(uid, start_time)` rows.
async fn node(rows: &[(&str, &str)]) -> Server {
    let rows: Vec<String> = rows
        .iter()
        .map(|(uid, time)| format!("[\"{uid}\",\"{time}\"]"))
        .collect();
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}},{{"name":"#1","path":"c/context/start_time/value"}}],"rows":[{}]}}"##,
        rows.join(",")
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

/// A gateway over nodes A and B, with `federation` added to its
/// `[federation]` table, which the helper writes last.
fn gateway_over(
    a: &Server,
    b: &Server,
    dir: &std::path::Path,
    federation: &str,
) -> Result<Router, Box<dyn Error>> {
    gateway(dir, &registry(&a.uri(), &b.uri(), ""), "", federation)
}

/// The gateway over nodes A and B whose ceiling is `ceiling` rows.
fn gateway_with_ceiling(
    a: &Server,
    b: &Server,
    dir: &std::path::Path,
    ceiling: u32,
) -> Result<Router, Box<dyn Error>> {
    gateway_over(a, b, dir, &format!("max_node_rows = {ceiling}\n"))
}

/// The `POST` body carrying `aql` with the paging members `offset` and
/// `fetch`.
fn paged(aql: &str, offset: Option<i64>, fetch: Option<i64>) -> Result<String, Box<dyn Error>> {
    let mut members = vec![format!("\"q\":{}", serde_json::to_string(aql)?)];
    if let Some(offset) = offset {
        members.push(format!("\"offset\":{offset}"));
    }
    if let Some(fetch) = fetch {
        members.push(format!("\"fetch\":{fetch}"));
    }
    Ok(format!("{{{}}}", members.join(",")))
}

/// Asserts that every node in `servers` was asked once, with `fetch`.
async fn each_asked_with(servers: [&Server; 2], fetch: &str) -> TestResult {
    for server in servers {
        let bodies = received(server).await?;
        let [sent] = bodies.as_slice() else {
            return Err(format!("each node is asked once: {bodies:?}").into());
        };
        assert!(
            sent.contains(&format!("\"fetch\":{fetch}")),
            "the node is sent an explicit fetch of {fetch}: {sent}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_query_with_no_limit_sends_every_node_the_default_ceiling_plus_one() -> TestResult {
    let a = node(&[("a1", "2026-01-01T00:00:00Z")]).await;
    let b = node(&[("b1", "2026-01-02T00:00:00Z")]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(&a, &b, dir.path(), "")?;
    let (status, text) = call(app, post(body(UNBOUNDED)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    each_asked_with([&a, &b], "10001").await
}

#[test]
fn a_ceiling_of_zero_is_refused_at_load() -> TestResult {
    let refused = ferrofed_server::config::Config::from_sources(
        Some("[federation]\nmax_node_rows = 0\n"),
        &std::collections::BTreeMap::new(),
    )?
    .resolve()
    .err()
    .ok_or("a ceiling of zero is refused")?;
    assert!(
        refused.to_string().contains("federation.max_node_rows"),
        "{refused}"
    );
    Ok(())
}

#[tokio::test]
async fn a_query_with_no_limit_sends_the_configured_ceiling_plus_one() -> TestResult {
    let a = node(&[("a1", "2026-01-01T00:00:00Z")]).await;
    let b = node(&[("b1", "2026-01-02T00:00:00Z")]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway_with_ceiling(&a, &b, dir.path(), 2)?;
    let (status, text) = call(app, post(body(UNBOUNDED)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    each_asked_with([&a, &b], "3").await
}

// conformance: CP-32
#[tokio::test]
async fn a_query_with_a_limit_sends_every_node_that_limit_as_fetch() -> TestResult {
    let a = node(&[("a1", "2026-01-01T00:00:00Z")]).await;
    let b = node(&[("b1", "2026-01-02T00:00:00Z")]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(&a, &b, dir.path(), "")?;
    let (status, text) = call(app, post(body(&format!("{ORDERED} LIMIT 2"))?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    each_asked_with([&a, &b], "2").await
}

// conformance: CP-32
#[tokio::test]
async fn a_bounded_page_sends_every_node_k_plus_n_as_fetch() -> TestResult {
    let a = node(&[("a1", "2026-01-01T00:00:00Z")]).await;
    let b = node(&[("b1", "2026-01-02T00:00:00Z")]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(&a, &b, dir.path(), "")?;
    let (status, text) = call(app, post(body(&format!("{ORDERED} LIMIT 2 OFFSET 1"))?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    each_asked_with([&a, &b], "3").await
}

/// The nodes of the ceiling tests: A answers three rows, one past a ceiling
/// of two, as a node whose own default page is the ceiling plus one would.
async fn past_the_ceiling() -> (Server, Server) {
    let a = node(&[
        ("a1", "2026-01-01T00:00:00Z"),
        ("a2", "2026-01-02T00:00:00Z"),
        ("a3", "2026-01-03T00:00:00Z"),
    ])
    .await;
    let b = node(&[("b1", "2026-01-04T00:00:00Z")]).await;
    (a, b)
}

// NOTE: §11.4, §11.5: a node that filled the ceiling may hold more rows, so
// its share is unknown and the query fails under all-or-nothing.
#[tokio::test]
async fn a_node_that_fills_the_ceiling_fails_the_query_424_and_is_never_merged() -> TestResult {
    let (a, b) = past_the_ceiling().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_with_ceiling(&a, &b, dir.path(), 2)?;
    let (status, text) = call(app, post(body(UNBOUNDED)?)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(!answer.meta.federation.complete, "§11.4: {text}");
    assert!(answer.rows.is_empty(), "a failing query returns no rows");
    assert_eq!(
        vec![("node-a-pub", "node-error"), ("node-b-pub", "active")],
        statuses(&answer),
        "§11.1: an answer the gateway could not use"
    );
    assert!(
        text.contains("more than the 2 rows the gateway reads of one node"),
        "the error names the ceiling: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn under_best_effort_a_node_that_fills_the_ceiling_is_named_and_its_rows_left_out()
-> TestResult {
    let (a, b) = past_the_ceiling().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_with_ceiling(&a, &b, dir.path(), 2)?;
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .header(COMPLETENESS, "partial")
        .body(Body::from(body(UNBOUNDED)?))?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        !answer.meta.federation.complete,
        "§11.4: the missing node clears complete: {text}"
    );
    assert_eq!(vec![vec!["b1".to_owned()]], answer.rows, "{text}");
    assert_eq!(
        vec![("node-a-pub", "node-error"), ("node-b-pub", "active")],
        statuses(&answer)
    );
    Ok(())
}

/// The nodes of the window tests, the merged order by start time
/// descending being b1, a1, b2, a2.
async fn window_nodes() -> (Server, Server) {
    let a = node(&[
        ("a1", "2026-01-03T00:00:00Z"),
        ("a2", "2026-01-01T00:00:00Z"),
    ])
    .await;
    let b = node(&[
        ("b1", "2026-01-04T00:00:00Z"),
        ("b2", "2026-01-02T00:00:00Z"),
    ])
    .await;
    (a, b)
}

/// The rows the gateway answers `request` with, after asserting a `200`.
async fn rows_of(app: Router, request: String) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    let (status, text) = call(app, post(request)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    Ok(serde_json::from_str::<Answer>(&text)?.rows)
}

// ITS-REST Query API `offset`, "the row number in result-set to start
// result-set from", is a row of the AQL result past its OFFSET (§11.6).
// conformance: CP-32
#[tokio::test]
async fn the_offset_member_starts_past_the_aql_offset() -> TestResult {
    let (a, b) = window_nodes().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(&a, &b, dir.path(), "")?;
    let rows = rows_of(
        app,
        paged(&format!("{ORDERED} LIMIT 3 OFFSET 1"), Some(1), None)?,
    )
    .await?;
    assert_eq!(
        vec![vec!["b2".to_owned()], vec!["a2".to_owned()]],
        rows,
        "rows 2 and 3 of the merged order: the AQL's rows 1 to 3, from its second"
    );
    each_asked_with([&a, &b], "4").await
}

// conformance: CP-32
#[tokio::test]
async fn a_fetch_member_smaller_than_the_aql_limit_is_the_count() -> TestResult {
    // Each node answers its first row alone, as a node sent `LIMIT 1` does.
    let a = node(&[("a1", "2026-01-03T00:00:00Z")]).await;
    let b = node(&[("b1", "2026-01-04T00:00:00Z")]).await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(&a, &b, dir.path(), "")?;
    let rows = rows_of(app, paged(&format!("{ORDERED} LIMIT 3"), None, Some(1))?).await?;
    assert_eq!(vec![vec!["b1".to_owned()]], rows);
    each_asked_with([&a, &b], "1").await
}

// conformance: CP-32
#[tokio::test]
async fn a_fetch_member_larger_than_the_aql_limit_keeps_the_limit() -> TestResult {
    let (a, b) = window_nodes().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(&a, &b, dir.path(), "")?;
    let rows = rows_of(app, paged(&format!("{ORDERED} LIMIT 2"), None, Some(10))?).await?;
    assert_eq!(vec![vec!["b1".to_owned()], vec!["a1".to_owned()]], rows);
    each_asked_with([&a, &b], "2").await
}

// conformance: CP-32
#[tokio::test]
async fn an_offset_member_past_the_aql_limit_answers_no_row() -> TestResult {
    let (a, b) = window_nodes().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(&a, &b, dir.path(), "")?;
    let rows = rows_of(app, paged(&format!("{ORDERED} LIMIT 2"), Some(5), None)?).await?;
    assert!(rows.is_empty(), "{rows:?}");
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn the_composed_window_is_held_to_the_configured_bound() -> TestResult {
    let (a, b) = window_nodes().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(&a, &b, dir.path(), "max_offset_window = 3\n")?;
    let request = paged(&format!("{ORDERED} LIMIT 3 OFFSET 1"), Some(1), None)?;
    let (status, text) = call(app, post(request)?).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        status,
        "§11.6.2: skip 1 + 1 and count 3 - 1 ask 4 rows of a node, past the bound of 3: {text}"
    );
    assert!(text.contains("offset-page"), "{text}");
    Ok(())
}
