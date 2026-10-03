// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The provenance headers on the federated AQL answer, against three mock
//! nodes (§7a.3, §9.6, §11.1; N31, N33; CP-24).
//!
//! A query dispatched to a single node, because it was directed at one
//! endpoint or the patient resolves at one member alone, answers naming
//! that endpoint in `openEHR-federation-endpoint` and the node's
//! `system_id` in `openEHR-federation-system-id`, whatever the node answered,
//! and only when the node was asked (N31). A fan-out answer lists the
//! endpoints that contributed rows, and their `system_id`s, in registry
//! order, in the comma-separated form of the request header (§7a.3, §8.4); a
//! node that failed or answered no rows is not listed, and
//! `meta.federation.endpoints[]` still names it (§11.1). Neither header ever
//! carries anything but registry endpoint ids and `system_id`s (§5.4.1, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{HeaderMap, Request, StatusCode, header};
use openehr_federation::headers::{COMPLETENESS, ENDPOINT, SYSTEM_ID};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::directive::{EHR_C, directed, members, patient};
use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, PATIENT_TAIL, body, crossref, gateway, node_answering,
    node_failing, received, statuses,
};
use crate::support::send;

type TestResult = Result<(), Box<dyn Error>>;

/// The patient at every member.
const EVERYWHERE: [(&str, &str); 3] = [("node-a", EHR_A), ("node-b", EHR_B), ("node-c", EHR_C)];

/// The endpoint ids the registry of [`members`] holds.
const ENDPOINT_IDS: [&str; 3] = ["node-a-pub", "node-b-pub", "node-c-pub"];

/// The `system_id`s the registry of [`members`] holds.
const SYSTEM_IDS: [&str; 3] = [
    "cdr-a.example.org",
    "cdr-b.example.org",
    "cdr-c.example.org",
];

/// A node answering `POST /v1/query/aql` with a result set of no rows.
async fn node_empty() -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            br##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##.to_vec(),
            "application/json",
        ))
        .mount(&server)
        .await;
    server
}

/// A development gateway over `a`, `b` and `c` that offers best-effort
/// completion, resolving the patient at the members `rows` name.
fn three(
    dir: &std::path::Path,
    (a, b, c): (&Server, &Server, &Server),
    rows: &[(&str, &str)],
) -> Result<Router, Box<dyn Error>> {
    // The tables follow the gateway's own `[federation]` keys, so the
    // first line of them sets one more of those keys.
    gateway(
        dir,
        &members(&a.uri(), &b.uri(), &c.uri()),
        "profile = \"development\"",
        &format!("best_effort = true\n{}", crossref(rows)),
    )
}

/// The undirected query for the patient's compositions.
fn undirected() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient()
    )
}

/// `POST /v1/query/aql` with the ad hoc query `aql` and the header lines
/// `fields`.
fn with(aql: &str, fields: &[(&str, &str)]) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request =
        Request::post("/v1/query/aql").header(header::CONTENT_TYPE, "application/json");
    for (name, value) in fields {
        request = request.header(*name, *value);
    }
    Ok(request.body(Body::from(body(aql)?))?)
}

/// What the gateway answered: the status, the response headers, and the
/// parsed body.
struct Answered {
    status: StatusCode,
    headers: HeaderMap,
    answer: Answer,
}

impl Answered {
    /// Every value of the response header `name`.
    fn values(&self, name: &str) -> Result<Vec<&str>, Box<dyn Error>> {
        let mut values = Vec::new();
        for value in self.headers.get_all(name) {
            values.push(value.to_str()?);
        }
        Ok(values)
    }
}

/// Sends `request` to the gateway `app` and reads the answer.
async fn answered(app: Router, request: Request<Body>) -> Result<Answered, Box<dyn Error>> {
    let response = send(app, request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    let answer: Answer = serde_json::from_str(&text).map_err(|e| format!("{e}: {text}"))?;
    Ok(Answered {
        status,
        headers,
        answer,
    })
}

// conformance: CP-24
#[tokio::test]
async fn a_query_directed_at_one_endpoint_names_it_and_its_system_id() -> TestResult {
    let by_header = with(&undirected(), &[(ENDPOINT, "node-b-pub")])?;
    let by_directive = with(&directed(r#"ENDPOINT p ["node-b-pub"]"#), &[])?;
    for (mechanism, request) in [("header", by_header), ("directive", by_directive)] {
        let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
        let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
        let c = node_answering("uid-at-c::cdr-c.example.org::1").await;
        let dir = tempfile::tempdir()?;
        let run = answered(three(dir.path(), (&a, &b, &c), &EVERYWHERE)?, request).await?;
        assert_eq!(StatusCode::OK, run.status, "{mechanism}");
        assert_eq!(
            vec!["node-b-pub"],
            run.values(ENDPOINT)?,
            "{mechanism}: N31, the one endpoint the query was dispatched to"
        );
        assert_eq!(
            vec!["cdr-b.example.org"],
            run.values(SYSTEM_ID)?,
            "{mechanism}: N31, §9.6, its node's system_id"
        );
        assert_eq!(
            1,
            received(&b).await?.len(),
            "{mechanism}: node B was asked"
        );
    }
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn a_directed_query_names_the_node_that_failed_it_once_the_node_was_asked() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_failing(500).await;
    let c = node_answering("uid-at-c::cdr-c.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = three(dir.path(), (&a, &b, &c), &EVERYWHERE)?;
    let run = answered(app, with(&undirected(), &[(ENDPOINT, "node-b-pub")])?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        run.status,
        "§11.4: a node error fails an all-or-nothing query"
    );
    assert_eq!(
        vec!["node-b-pub"],
        run.values(ENDPOINT)?,
        "N31: the request was dispatched to node B alone, whatever it answered"
    );
    assert_eq!(vec!["cdr-b.example.org"], run.values(SYSTEM_ID)?);
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn an_undirected_query_the_patient_resolves_for_at_one_member_names_it() -> TestResult {
    for (outcome, status) in [
        ("zero rows", StatusCode::OK),
        ("a node error", StatusCode::FAILED_DEPENDENCY),
    ] {
        let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
        let b = if status == StatusCode::OK {
            node_empty().await
        } else {
            node_failing(500).await
        };
        let c = node_answering("uid-at-c::cdr-c.example.org::1").await;
        let dir = tempfile::tempdir()?;
        let app = three(dir.path(), (&a, &b, &c), &[("node-b", EHR_B)])?;
        let run = answered(app, with(&undirected(), &[])?).await?;
        assert_eq!(status, run.status, "{outcome}: §11.4");
        assert_eq!(
            [0, 1, 0],
            [
                received(&a).await?.len(),
                received(&b).await?.len(),
                received(&c).await?.len(),
            ]
        );
        assert_eq!(
            vec!["node-b-pub"],
            run.values(ENDPOINT)?,
            "{outcome}: N31, the request was dispatched to node B alone"
        );
        assert_eq!(
            vec!["cdr-b.example.org"],
            run.values(SYSTEM_ID)?,
            "{outcome}: N31, §9.6"
        );
    }
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn a_directed_query_that_asked_no_node_names_none() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let c = node_answering("uid-at-c::cdr-c.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-c", EHR_C)];
    let app = three(dir.path(), (&a, &b, &c), &rows)?;
    let run = answered(app, with(&undirected(), &[(ENDPOINT, "node-b-pub")])?).await?;
    assert_eq!(
        StatusCode::OK,
        run.status,
        "§8.1: not-resolved is reported, never errored"
    );
    assert!(
        received(&b).await?.is_empty(),
        "node B does not know the patient, so it is never asked"
    );
    assert_eq!(
        vec![
            ("node-a-pub", "excluded"),
            ("node-b-pub", "not-resolved"),
            ("node-c-pub", "excluded"),
        ],
        statuses(&run.answer)
    );
    assert!(
        run.values(ENDPOINT)?.is_empty() && run.values(SYSTEM_ID)?.is_empty(),
        "N31: no node acted, so the answer names none"
    );
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn a_fan_out_lists_the_endpoints_that_contributed_rows_in_registry_order() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_empty().await;
    let c = node_answering("uid-at-c::cdr-c.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = three(dir.path(), (&a, &b, &c), &EVERYWHERE)?;
    let run = answered(app, with(&undirected(), &[])?).await?;
    assert_eq!(StatusCode::OK, run.status);
    assert_eq!(
        vec!["node-a-pub, node-c-pub"],
        run.values(ENDPOINT)?,
        "§7a.3, §8.4: the endpoints that contributed rows, as one comma-separated list"
    );
    assert_eq!(
        vec!["cdr-a.example.org, cdr-c.example.org"],
        run.values(SYSTEM_ID)?,
        "§7a.3: their system_ids, position for position"
    );
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "active"),
        ],
        statuses(&run.answer),
        "§11.1: the body still names node B"
    );
    let empty = run
        .answer
        .meta
        .federation
        .endpoints
        .iter()
        .find(|endpoint| endpoint.id == "node-b-pub")
        .ok_or("node B is reported")?;
    assert_eq!(
        Some(0),
        empty.row_count,
        "§9.5: node B answered and contributed no row, so it is not listed"
    );
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn under_partial_a_failed_node_and_an_empty_one_are_not_listed() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_empty().await;
    let c = node_failing(500).await;
    let dir = tempfile::tempdir()?;
    let app = three(dir.path(), (&a, &b, &c), &EVERYWHERE)?;
    let run = answered(app, with(&undirected(), &[(COMPLETENESS, "partial")])?).await?;
    assert_eq!(StatusCode::OK, run.status, "§11.4: best-effort");
    assert!(!run.answer.meta.federation.complete);
    assert_eq!(
        vec!["node-a-pub"],
        run.values(ENDPOINT)?,
        "§7a.3: node A alone contributed rows"
    );
    assert_eq!(vec!["cdr-a.example.org"], run.values(SYSTEM_ID)?);
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "node-error"),
        ],
        statuses(&run.answer),
        "§11.1: meta.federation names every node, listed or not"
    );
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn an_all_or_nothing_failure_lists_no_endpoint() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let c = node_failing(500).await;
    let dir = tempfile::tempdir()?;
    let app = three(dir.path(), (&a, &b, &c), &EVERYWHERE)?;
    let run = answered(app, with(&undirected(), &[])?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, run.status, "§11.4");
    assert!(
        run.answer.rows.is_empty(),
        "§11.4: a failing query returns no rows"
    );
    assert!(
        run.values(ENDPOINT)?.is_empty() && run.values(SYSTEM_ID)?.is_empty(),
        "§7a.3: no endpoint contributed rows to a failing answer"
    );
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "node-error"),
        ],
        statuses(&run.answer),
        "§11.1: meta.federation names every node"
    );
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn the_provenance_headers_carry_registry_ids_and_nothing_the_request_sent() -> TestResult {
    let requests = [
        with(&undirected(), &[])?,
        with(&undirected(), &[(ENDPOINT, "node-a-pub")])?,
        with(&directed(r#"ENDPOINT p ["node-c-pub"]"#), &[])?,
    ];
    for request in requests {
        let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
        let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
        let c = node_answering("uid-at-c::cdr-c.example.org::1").await;
        let dir = tempfile::tempdir()?;
        let run = answered(three(dir.path(), (&a, &b, &c), &EVERYWHERE)?, request).await?;
        assert_eq!(StatusCode::OK, run.status);
        let endpoints = run.values(ENDPOINT)?;
        let system_ids = run.values(SYSTEM_ID)?;
        assert_eq!(1, endpoints.len(), "one endpoint header");
        assert_eq!(1, system_ids.len(), "one system-id header");
        for (values, known) in [(&endpoints, ENDPOINT_IDS), (&system_ids, SYSTEM_IDS)] {
            for value in values {
                for item in value.split(", ") {
                    assert!(
                        known.contains(&item),
                        "§5.4.1, N33: {item:?} is a registry id, never a request value"
                    );
                }
            }
        }
        for (name, value) in &run.headers {
            let value = value.to_str()?;
            assert!(
                !value.contains(PATIENT_TAIL) && !value.contains(NAMESPACE),
                "§5.4.1: the response header {name} carries no part of the patient identifier"
            );
        }
    }
    Ok(())
}
