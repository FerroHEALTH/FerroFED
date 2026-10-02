// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ORDER BY` with `LIMIT` through the façade (§11.6.1, N13, N39): each node
//! is asked for the key as a hidden column, the row key (the uid, or the
//! `ehr_id` of a row with no uid) as the last key and the client's `LIMIT n`,
//! and the client receives the global top `n` with only the columns it
//! selected (no specification governs the hidden column or the order check:
//! our own design). A page at `OFFSET k` asks each node for `k + n` rows
//! within the configured bound, or is refused under the reject strategy
//! (§11.6.2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};

use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

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
        "N39: the latest two across both nodes, and only the selected column"
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
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "N37, a node out of the Tier order: {text}"
    );
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

/// One synthetic `EHR` a node holds: its `ehr_id`, its `time_created` and its
/// `EHR_STATUS` uid.
type Ehr = (&'static str, &'static str, &'static str);

/// The AQL of a node request body.
#[derive(serde::Deserialize)]
struct Sent {
    q: String,
}

/// A node answering the `EHR`-only query of these tests as a CDR would: it
/// orders on `time_created`, then on `ehr_id` when it is asked to, and cuts at
/// the `LIMIT` it was sent. Asked for no tie-break, it returns a different
/// choice among the rows tied on `time_created` on every call, as AQL lets it
/// (AQL master03-syntax §LIMIT).
struct EhrNode {
    ehrs: Vec<Ehr>,
    calls: AtomicUsize,
}

impl Respond for EhrNode {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let Ok(sent) = serde_json::from_slice::<Sent>(&request.body) else {
            return ResponseTemplate::new(400);
        };
        let keyed = sent.q.contains("e/ehr_id/value ASC");
        let limit = sent
            .q
            .rsplit_once("LIMIT ")
            .and_then(|(_, count)| count.trim().parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        let mut ehrs = self.ehrs.clone();
        ehrs.sort_by(|a, b| a.1.cmp(b.1));
        for tied in ehrs.chunk_by_mut(|a, b| a.1 == b.1) {
            if keyed {
                tied.sort_by(|a, b| a.0.cmp(b.0));
            } else {
                let turn = call % tied.len();
                tied.rotate_left(turn);
            }
        }
        ehrs.truncate(limit);
        let rows: Vec<String> = ehrs
            .iter()
            .map(|(ehr_id, created, status)| {
                if keyed {
                    format!(r#"["{status}","{created}","{ehr_id}"]"#)
                } else {
                    format!(r#"["{status}","{created}"]"#)
                }
            })
            .collect();
        let answer = format!(r#"{{"q":"node","rows":[{}]}}"#, rows.join(","));
        ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json")
    }
}

async fn ehr_node(ehrs: &[Ehr]) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(EhrNode {
            ehrs: ehrs.to_vec(),
            calls: AtomicUsize::new(0),
        })
        .mount(&server)
        .await;
    server
}

const T1: &str = "2026-01-01T00:00:00Z";
const T2: &str = "2026-01-02T00:00:00Z";
const T3: &str = "2026-01-03T00:00:00Z";

/// Node A holds three `EHR`s created at `T1`, stored out of `ehr_id` order,
/// and one at `T2`; node B one at `T1` and one at `T3`.
const NODE_A: [Ehr; 4] = [
    ("a0000000-0000-4000-8000-000000000003", T1, "status-a3"),
    ("a0000000-0000-4000-8000-000000000001", T1, "status-a1"),
    ("a0000000-0000-4000-8000-000000000002", T1, "status-a2"),
    ("a0000000-0000-4000-8000-000000000004", T2, "status-a4"),
];
const NODE_B: [Ehr; 2] = [
    ("b0000000-0000-4000-8000-000000000001", T1, "status-b1"),
    ("b0000000-0000-4000-8000-000000000002", T3, "status-b2"),
];

/// Runs `aql` through `app` and returns the client's rows, checking the
/// answer and that `columns[]` is the client's one column (N17, §9.2).
async fn rows_of(app: &axum::Router, aql: &str) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    let (status, text) = call(app.clone(), post(body(aql)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![Some("/ehr_status/uid/value")],
        answer.column_paths(),
        "N17, §9.2: columns[] renders the client's query, never the pushed ehr_id"
    );
    Ok(answer.rows)
}

fn statuses_of(rows: &[&str]) -> Vec<Vec<String>> {
    rows.iter().map(|row| vec![(*row).to_owned()]).collect()
}

// conformance: CP-32
#[tokio::test]
async fn an_ehr_only_query_returns_the_same_rows_on_every_repeat() -> TestResult {
    let a = ehr_node(&NODE_A).await;
    let b = ehr_node(&NODE_B).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
    let aql = "SELECT e/ehr_status/uid/value AS status FROM EHR e \
               ORDER BY e/time_created/value LIMIT 2";

    let first = rows_of(&app, aql).await?;
    let second = rows_of(&app, aql).await?;
    assert_eq!(
        statuses_of(&["status-a1", "status-a2"]),
        first,
        "§11.6.1: tied on time_created, broken on endpoint_id, then ehr_id"
    );
    assert_eq!(
        first, second,
        "§11.6.1: repeating a query returns the same rows"
    );
    for server in [&a, &b] {
        let bodies = received(server).await?;
        assert_eq!(bodies.len(), 2, "each node is asked once per run");
        for sent in &bodies {
            assert!(
                sent.contains(
                    "e/time_created/value, e/ehr_id/value FROM EHR e \
                     ORDER BY e/time_created/value, e/ehr_id/value ASC LIMIT 2"
                ),
                "the ehr_id reaches the node as a hidden column and the last key: {sent}"
            );
        }
    }
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn bounded_pages_tied_across_their_edges_repeat_and_tile() -> TestResult {
    // Node A's fourth EHR at T1 sits past the first page's k + n = 3 cut.
    let mut node_a = NODE_A.to_vec();
    node_a.insert(1, ("a0000000-0000-4000-8000-000000000005", T1, "status-a5"));
    let a = ehr_node(&node_a).await;
    let b = ehr_node(&NODE_B).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
    let page = |offset: u32| {
        format!(
            "SELECT e/ehr_status/uid/value AS status FROM EHR e \
             ORDER BY e/time_created/value LIMIT 2 OFFSET {offset}"
        )
    };

    let mut pages = Vec::new();
    for offset in [1, 3] {
        let first = rows_of(&app, &page(offset)).await?;
        let second = rows_of(&app, &page(offset)).await?;
        assert_eq!(
            first, second,
            "§11.6.1: the page at OFFSET {offset} repeats"
        );
        pages.push(first);
    }
    assert_eq!(
        vec![
            statuses_of(&["status-a2", "status-a3"]),
            statuses_of(&["status-a5", "status-b1"]),
        ],
        pages,
        "§11.6.2: rows [1, 3) and [3, 5) of one order, with no tied row lost or repeated \
         across the edge"
    );
    let bodies = received(&a).await?;
    for (sent, window) in bodies.iter().zip(["3", "3", "5", "5"]) {
        assert!(
            sent.contains(&format!("e/ehr_id/value ASC LIMIT {window}")),
            "k + n rows, tie-broken on the ehr_id: {sent}"
        );
        assert!(
            !sent.contains("OFFSET"),
            "§11.6.2: never pushed down: {sent}"
        );
    }
    Ok(())
}
