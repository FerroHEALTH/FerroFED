// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The request id toward the nodes (§5.4.1, N33, CP-26): the `x-request-id` a
//! client sends is free text the gateway cannot classify, so no node receives
//! it. Every node a façade query reaches receives the one id the gateway
//! minted for the request, and the client's own id names the request only in
//! the answer. Asserted on the raw bytes each mock node received.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};

use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, body, crossref, dev_gateway, gateway, node_answering,
    registry,
};
use crate::request_log::logged;
use crate::support::{self, error_body, request_lines, send};

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic identifier the gateway never resolved on: visibly synthetic,
/// under no real scheme, and passing no national check digit.
const UNRESOLVED: &str = "SYNTHETIC-NATIONAL-ID-0001";

/// A client id in the very form the gateway mints, a lowercase hyphenated
/// version 4 UUID, which the node-wire scan records as its placeholder.
const CLIENT_UUID: &str = "7d0c2b1e-4f3a-4c5b-8d6e-9f0a1b2c3d4e";

/// The façade query naming [`PATIENT`] through `external_ref`.
fn patient_query() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// `POST /v1/query/aql` with `aql`, naming the request `client_id` when given.
fn query(aql: &str, client_id: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request =
        Request::post("/v1/query/aql").header(header::CONTENT_TYPE, "application/json");
    if let Some(id) = client_id {
        request = request.header("x-request-id", id);
    }
    Ok(request.body(Body::from(body(aql)?))?)
}

/// Whether `haystack` holds `needle` anywhere, byte for byte.
fn holds(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// The one `x-request-id` `server` received, after asserting that `absent`
/// is in no byte of the request line, any header or the body.
async fn the_id_at(server: &Server, absent: &str) -> Result<String, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one request, got {}", requests.len()).into());
    };
    assert!(!request.url.as_str().contains(absent), "{}", request.url);
    for (name, value) in &request.headers {
        assert!(
            !holds(name.as_str().as_bytes(), absent) && !holds(value.as_bytes(), absent),
            "the {name} header carries the client's id"
        );
    }
    assert!(!holds(&request.body, absent), "the body carries it");
    let ids: Vec<&[u8]> = request
        .headers
        .get_all("x-request-id")
        .iter()
        .map(http::HeaderValue::as_bytes)
        .collect();
    let [id] = ids.as_slice() else {
        return Err(format!("expected one x-request-id, got {}", ids.len()).into());
    };
    let id = std::str::from_utf8(id)?.to_owned();
    let parsed = uuid::Uuid::parse_str(&id)?;
    assert_eq!(
        Some(uuid::Version::Random),
        parsed.get_version(),
        "the node receives a minted version 4 UUID: {id}"
    );
    Ok(id)
}

/// The `x-request-id` a response carries.
fn answered_id(response: &http::Response<Body>) -> Result<String, Box<dyn Error>> {
    Ok(response
        .headers()
        .get("x-request-id")
        .ok_or("every response carries an x-request-id")?
        .to_str()?
        .to_owned())
}

// conformance: CP-26
#[tokio::test]
async fn a_client_request_id_never_reaches_a_node_on_the_fan_out() -> TestResult {
    for client_id in [UNRESOLVED, PATIENT, CLIENT_UUID] {
        let a = node_answering("uid-at-a").await;
        let b = node_answering("uid-at-b").await;
        let dir = tempfile::tempdir()?;
        let app = dev_gateway(
            dir.path(),
            &a.uri(),
            &b.uri(),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
        )?;
        let response = send(app, query(&patient_query(), Some(client_id))?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{client_id}");
        assert_eq!(
            client_id,
            answered_id(&response)?,
            "the response echoes the client's own id"
        );
        let at_a = the_id_at(&a, client_id).await?;
        let at_b = the_id_at(&b, client_id).await?;
        assert_eq!(at_a, at_b, "one outbound id for every node of the request");
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn with_no_client_id_the_response_and_every_node_share_the_minted_id() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let response = send(app, query(&patient_query(), None)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    let answered = answered_id(&response)?;
    assert_eq!(answered, the_id_at(&a, PATIENT).await?);
    assert_eq!(answered, the_id_at(&b, PATIENT).await?);
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn two_requests_reach_the_nodes_under_two_ids() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    for _ in 0..2 {
        let response = send(app.clone(), query(&patient_query(), Some(UNRESOLVED))?).await?;
        assert_eq!(StatusCode::OK, response.status());
    }
    let requests = a.received_requests().await.ok_or("recording is on")?;
    let ids: Vec<&[u8]> = requests
        .iter()
        .filter_map(|request| request.headers.get("x-request-id"))
        .map(http::HeaderValue::as_bytes)
        .collect();
    let [first, second] = ids.as_slice() else {
        return Err(format!("expected two ids, got {}", ids.len()).into());
    };
    assert_ne!(first, second, "each client request gets its own id");
    Ok(())
}

#[tokio::test]
async fn a_refused_query_names_the_clients_own_id_in_its_error_body() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let refused = format!(
        "{} AND c/composer/identifiers/id = '{PATIENT}'",
        patient_query()
    );
    let response = send(app, query(&refused, Some("corr-refused-1"))?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!("corr-refused-1", answered_id(&response)?);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let error = error_body(std::str::from_utf8(&bytes)?)?;
    assert_eq!("corr-refused-1", error.request_id);
    for server in [&a, &b] {
        let asked = server.received_requests().await.ok_or("recording is on")?;
        assert!(asked.is_empty(), "no node is asked");
    }
    Ok(())
}

// conformance: CP-26
#[test]
fn a_failed_query_is_logged_under_the_id_its_request_line_records() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (a, b) = runtime.block_on(async {
        (
            node_answering("uid-at-a").await,
            node_answering("uid-at-b").await,
        )
    });
    // NOTE: §5.4.1, N33, a registry URL that carries the patient identifier
    // is stopped by the outbound gate, which fails the query on the gateway.
    let leaking = format!("{}/{PATIENT}", a.uri());
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &registry(&leaking, &b.uri(), ""),
        "profile = \"development\"",
        &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
    )?;
    let client_id = "SYNTHETIC-NATIONAL-ID-0004";
    let text = logged(
        &app,
        "info",
        vec![query(&patient_query(), Some(client_id))?],
    )?;
    let lines = support::lines(&text)?;
    let request = request_lines(&text)?
        .into_iter()
        .next()
        .ok_or("the request is logged")?;
    assert_eq!(
        Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
        request.status,
        "the gate failed the query: {text}"
    );
    let gateway_id = request.request_id.as_deref().ok_or("under an id")?;
    let failed = lines
        .iter()
        .find(|line| line.message == "the federated query failed")
        .ok_or("the failure is logged")?;
    assert_eq!(
        Some(gateway_id),
        failed.request_id.as_deref(),
        "the failure line names the request its request line records: {text}"
    );
    assert!(
        !text.contains(client_id),
        "the client's id reached the log: {text}"
    );
    let asked = runtime
        .block_on(a.received_requests())
        .ok_or("recording is on")?;
    assert!(asked.is_empty(), "the gate sent nothing to node A");
    Ok(())
}
