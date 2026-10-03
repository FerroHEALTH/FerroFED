// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A federated query the client scoped to one `ehr_id`, in either form of
//! N29, against two mock nodes (CP-22): both forms reach the owning node as
//! the same query and answer the same rows, and the query reaches that node
//! alone, found in the order of §12.5.1 (N41), with every other member
//! reported `excluded` (§11.1) and the acting endpoint named (N31). Every
//! assertion on what a node received reads the node's own capture.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::facade::{
    Answer, EHR_A, EHR_B, PATIENT, body, dev_gateway, patient_query, post, received, schema,
    statuses, wire,
};
use crate::support::{call, error_body, send};

type TestResult = Result<(), Box<dyn Error>>;

/// The query in the canonical form, scoped to node A's `ehr_id`.
fn where_form() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_id/value = '{EHR_A}' \
         AND c/archetype_node_id = 'openEHR-EHR-COMPOSITION.encounter.v1'"
    )
}

/// The same query scoped through the `EHR` class predicate.
fn from_form() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_A}'] CONTAINS COMPOSITION c \
         WHERE c/archetype_node_id = 'openEHR-EHR-COMPOSITION.encounter.v1'"
    )
}

/// A node that answers the probe for [`EHR_A`] with `holds` (`200` or
/// `404`) and every query with one row holding `uid`.
async fn node(holds: u16, uid: &str) -> MockServer {
    let server = MockServer::start().await;
    mount(&server, holds, uid).await;
    server
}

/// Mounts the answers of [`node`] on `server`.
async fn mount(server: &MockServer, holds: u16, uid: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_A}")))
        .respond_with(
            ResponseTemplate::new(holds)
                .set_body_raw(br#"{"message":"synthetic"}"#.to_vec(), "application/json"),
        )
        .mount(server)
        .await;
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}}],"rows":[["{uid}"]]}}"##
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json"),
        )
        .mount(server)
        .await;
}

/// The gateway over `a` and `b`, the patient resolving at both.
fn gateway(
    dir: &std::path::Path,
    a: &MockServer,
    b: &MockServer,
) -> Result<Router, Box<dyn Error>> {
    dev_gateway(
        dir,
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )
}

/// The methods and paths every request `server` received was sent with.
async fn requests(server: &MockServer) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    Ok(requests
        .iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect())
}

/// The queries a node was sent.
async fn queries(server: &MockServer) -> Result<Vec<String>, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut bodies = Vec::new();
    for request in requests
        .iter()
        .filter(|request| request.method.as_str() == "POST")
    {
        bodies.push(String::from_utf8(request.body.clone())?);
    }
    Ok(bodies)
}

/// What the gateway answered `aql` with, and what each node was sent.
struct Run {
    status: StatusCode,
    text: String,
    endpoint: Option<String>,
    a: Vec<String>,
    b: Vec<(String, String)>,
}

/// Runs `aql` against a fresh gateway over a node A that holds [`EHR_A`] and
/// a node B that does not.
async fn run(aql: &str) -> Result<Run, Box<dyn Error>> {
    let a = node(200, "uid-at-a::cdr-a.example.org::1").await;
    let b = node(404, "uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let response = send(app, post(body(aql)?)?).await?;
    let status = response.status();
    let endpoint = response
        .headers()
        .get("openEHR-federation-endpoint")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok(Run {
        status,
        text: String::from_utf8(bytes.to_vec())?,
        endpoint,
        a: queries(&a).await?,
        b: requests(&b).await?,
    })
}

// conformance: CP-22
#[tokio::test]
async fn both_forms_send_the_owner_the_same_query_and_answer_the_same_rows() -> TestResult {
    let where_run = run(&where_form()).await?;
    let from_run = run(&from_form()).await?;
    for (form, run) in [("WHERE", &where_run), ("FROM", &from_run)] {
        assert_eq!(StatusCode::OK, run.status, "{form}: {}", run.text);
        schema::validate(&run.text)?;
        assert_eq!(1, run.a.len(), "{form}: node A is asked the query once");
    }
    assert_eq!(
        where_run.a, from_run.a,
        "N29: the forms are equivalent, and node A is sent the same query for both"
    );
    let where_rows = serde_json::from_str::<Answer>(&where_run.text)?.rows;
    let from_rows = serde_json::from_str::<Answer>(&from_run.text)?.rows;
    assert_eq!(where_rows, from_rows, "the same rows");
    assert_eq!(
        vec![vec!["uid-at-a::cdr-a.example.org::1".to_owned()]],
        from_rows
    );
    Ok(())
}

// conformance: CP-22
#[tokio::test]
async fn a_scoped_query_reaches_only_the_member_that_holds_the_ehr_id() -> TestResult {
    let run = run(&from_form()).await?;
    assert_eq!(StatusCode::OK, run.status, "{}", run.text);
    assert_eq!(
        vec![("GET".to_owned(), format!("/v1/ehr/{EHR_A}"))],
        run.b,
        "node B is probed for the ehr_id and never sent the query (§12.5.1 step 4)"
    );
    let answer: Answer = serde_json::from_str(&run.text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "excluded")],
        statuses(&answer),
        "§11.1: the owner answered, and the routing decision ruled the other out"
    );
    assert!(
        answer.meta.federation.complete,
        "an excluded member is never in scope"
    );
    assert_eq!(
        Some("node-a-pub"),
        run.endpoint.as_deref(),
        "N31: a query dispatched to one node names the acting endpoint"
    );
    Ok(())
}

// conformance: CP-22
#[tokio::test]
async fn the_index_routes_a_scoped_query_and_nothing_is_probed() -> TestResult {
    let a = node(200, "uid-at-a::cdr-a.example.org::1").await;
    let b = node(404, "uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the resolution teaches the index: {text}"
    );
    a.reset().await;
    b.reset().await;
    mount(&a, 200, "uid-at-a::cdr-a.example.org::1").await;
    mount(&b, 404, "uid-at-b::cdr-b.example.org::1").await;
    let (status, text) = call(app, post(body(&where_form())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![("POST".to_owned(), "/v1/query/aql".to_owned())],
        requests(&a).await?,
        "§12.5.1 step 3: the index names node A, so no member is probed"
    );
    assert!(requests(&b).await?.is_empty(), "node B is never asked");
    Ok(())
}

// conformance: CP-22
#[tokio::test]
async fn a_targeted_scoped_query_goes_to_the_named_endpoint_and_nothing_is_probed() -> TestResult {
    let a = node(200, "uid-at-a::cdr-a.example.org::1").await;
    let b = node(404, "uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let request = Request::post("/v1/query/aql")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("openEHR-federation-endpoint", "node-b-pub")
        .body(Body::from(body(&from_form())?))?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(
        requests(&a).await?.is_empty(),
        "§12.5.1 step 1: node A is never asked"
    );
    assert_eq!(
        vec![("POST".to_owned(), "/v1/query/aql".to_owned())],
        requests(&b).await?,
        "the explicit target is followed, never probed (§8.4, §12.5.1)"
    );
    Ok(())
}

// conformance: CP-22 CP-33
#[tokio::test]
async fn two_members_holding_the_ehr_id_are_a_409_and_neither_is_queried() -> TestResult {
    let a = node(200, "uid-at-a::cdr-a.example.org::1").await;
    let b = node(200, "uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(body(&where_form())?)?).await?;
    assert_eq!(StatusCode::CONFLICT, status, "§12.5.2, N42: {text}");
    assert_eq!("ehr-id-collision", error_body(&text)?.code);
    for node in [&a, &b] {
        assert!(
            queries(node).await?.is_empty(),
            "neither claimant is sent the query (§12.5.2)"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_ehr_id_no_member_holds_has_no_destination() -> TestResult {
    let a = node(404, "uid-at-a::cdr-a.example.org::1").await;
    let b = node(404, "uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(body(&from_form())?)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "§11.2: {text}");
    assert_eq!("no-destination", error_body(&text)?.code);
    for node in [&a, &b] {
        assert!(
            queries(node).await?.is_empty(),
            "no member is sent the query"
        );
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn an_ehr_id_that_is_no_uuid_is_never_probed_nor_sent() -> TestResult {
    let a = node(200, "uid-at-a::cdr-a.example.org::1").await;
    let b = node(200, "uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let aql = "SELECT c/uid/value FROM EHR e[ehr_id/value='2.999.38'] CONTAINS COMPOSITION c";
    let (status, text) = call(app, post(body(aql)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "§5.4.1, N33: {text}");
    assert_eq!("probe-requires-uuid", error_body(&text)?.code);
    for node in [&a, &b] {
        assert!(wire(node).await?.is_empty(), "no member is asked anything");
    }
    Ok(())
}

#[tokio::test]
async fn an_ehr_id_that_is_no_hier_object_id_is_a_400_before_anything_is_sent() -> TestResult {
    let a = node(200, "uid-at-a::cdr-a.example.org::1").await;
    let b = node(200, "uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let aql = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = ''";
    let (status, text) = call(app, post(body(aql)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "§12.5: {text}");
    assert_eq!("ehr-id-invalid", error_body(&text)?.code);
    for node in [&a, &b] {
        assert!(wire(node).await?.is_empty(), "no member is asked anything");
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn neither_form_carries_the_patient_identifier_to_a_node() -> TestResult {
    let a = node(200, "uid-at-a::cdr-a.example.org::1").await;
    let b = node(404, "uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    for aql in [where_form(), from_form(), patient_query()] {
        let (status, text) = call(app.clone(), post(body(&aql)?)?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
    }
    for node in [&a, &b] {
        assert!(!wire(node).await?.contains(PATIENT), "N33: no node sees it");
        for sent in received(node).await? {
            assert!(!sent.contains(PATIENT), "N33: {sent}");
        }
    }
    Ok(())
}
