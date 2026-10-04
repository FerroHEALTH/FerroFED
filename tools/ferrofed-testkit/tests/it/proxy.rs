// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The capturing and fault proxy, offline, in front of a `wiremock` node.

use ferrofed_testkit::mock::Server;
use ferrofed_testkit::proxy::{CapturingProxy, Fault};
use http::StatusCode;
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// A stub node that answers `GET /ferroehr/rest/openehr/v1/ehr/{id}` with a
/// body and an `ETag`, and records what reached it.
async fn node() -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/ferroehr/rest/openehr/v1/ehr/7f4c"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"7f4c\"")
                .set_body_string("{\"ehr_id\":{\"value\":\"7f4c\"}}"),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/ferroehr/rest/openehr/v1/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"rows\":[]}"))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn forwards_unmodified_and_journals_every_carrier() {
    let node = node().await;
    let proxy = CapturingProxy::start(node.uri()).await.unwrap();
    let client = reqwest::Client::new();

    let answer = client
        .post(format!(
            "{}/ferroehr/rest/openehr/v1/query/aql?fetch=5",
            proxy.origin()
        ))
        .header("openEHR-federation-client", "audit-subject")
        .body("{\"q\":\"SELECT e/ehr_id/value FROM EHR e\"}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "the node's own status passes through"
    );
    assert_eq!(
        answer.text().await.unwrap(),
        "{\"rows\":[]}",
        "the node's own body passes through"
    );

    let journal = proxy.journal();
    assert_eq!(journal.len(), 1, "one request was made");
    let capture = &journal[0];
    assert_eq!(capture.method, "POST", "the method is journalled");
    assert_eq!(
        capture.path, "/ferroehr/rest/openehr/v1/query/aql",
        "the path is journalled without the query"
    );
    assert_eq!(
        capture.query.as_deref(),
        Some("fetch=5"),
        "the query is journalled"
    );
    assert!(
        capture
            .headers
            .iter()
            .any(|(name, value)| name == "openehr-federation-client" && value == b"audit-subject"),
        "every header is journalled"
    );
    assert_eq!(
        capture.body, b"{\"q\":\"SELECT e/ehr_id/value FROM EHR e\"}",
        "the body is journalled byte for byte"
    );

    let received = node.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "the node received the request once");
    assert_eq!(
        received[0].body, b"{\"q\":\"SELECT e/ehr_id/value FROM EHR e\"}",
        "the node received the body byte for byte"
    );
}

#[tokio::test]
async fn the_journal_finds_a_needle_in_any_carrier_and_only_there() {
    let node = node().await;
    let proxy = CapturingProxy::start(node.uri()).await.unwrap();
    let client = reqwest::Client::new();
    let base = format!("{}/ferroehr/rest/openehr/v1", proxy.origin());

    client
        .get(format!("{base}/ehr/7f4c?subject_id=ffd-test-0001"))
        .send()
        .await
        .unwrap();
    assert!(
        proxy.journal_contains(b"ffd-test-0001"),
        "a needle in the query is found"
    );
    assert!(
        !proxy.journal_contains(b"ffd-test-0002"),
        "an absent needle is not found"
    );

    proxy.clear_journal();
    assert!(proxy.journal().is_empty(), "clearing forgets every capture");

    client
        .get(format!("{base}/ehr/7f4c"))
        .header("x-carrier", "ffd-test-0003")
        .send()
        .await
        .unwrap();
    assert!(
        proxy.journal_contains(b"ffd-test-0003"),
        "a needle in a header value is found"
    );

    client
        .post(format!("{base}/query/aql"))
        .body("{\"q\":\"... = 'ffd-test-0004'\"}")
        .send()
        .await
        .unwrap();
    assert!(
        proxy.journal_contains(b"ffd-test-0004"),
        "a needle in the body is found"
    );
}

#[tokio::test]
async fn a_status_fault_answers_without_reaching_the_node() {
    let node = node().await;
    let proxy = CapturingProxy::start(node.uri()).await.unwrap();
    proxy.set_fault(Fault::Status(StatusCode::INTERNAL_SERVER_ERROR));

    let answer = reqwest::get(format!(
        "{}/ferroehr/rest/openehr/v1/ehr/7f4c",
        proxy.origin()
    ))
    .await
    .unwrap();
    assert_eq!(
        answer.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "the injected status is answered"
    );
    assert_eq!(
        proxy.journal().len(),
        1,
        "the refused request is still journalled"
    );
    assert!(
        node.received_requests().await.unwrap().is_empty(),
        "an injected status never reaches the node"
    );

    proxy.clear_fault();
    let answer = reqwest::get(format!(
        "{}/ferroehr/rest/openehr/v1/ehr/7f4c",
        proxy.origin()
    ))
    .await
    .unwrap();
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "clearing the fault forwards again"
    );
}

#[tokio::test]
async fn a_reply_fault_answers_its_status_and_json_body_without_reaching_the_node() {
    let node = node().await;
    let proxy = CapturingProxy::start(node.uri()).await.unwrap();
    let error = r#"{"message":"synthetic refusal","code":"synthetic-code"}"#;
    proxy.set_fault(Fault::Reply(StatusCode::FORBIDDEN, error));

    let answer = reqwest::get(format!(
        "{}/ferroehr/rest/openehr/v1/ehr/7f4c",
        proxy.origin()
    ))
    .await
    .unwrap();
    assert_eq!(answer.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        answer
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(answer.text().await.unwrap(), error, "the body as given");
    assert_eq!(proxy.journal().len(), 1, "the request is journalled");
    assert!(
        node.received_requests().await.unwrap().is_empty(),
        "an injected reply never reaches the node"
    );
}

#[tokio::test]
async fn a_delay_fault_holds_the_request_then_forwards_it() {
    let node = node().await;
    let proxy = CapturingProxy::start(node.uri()).await.unwrap();
    let delay = Duration::from_millis(400);
    proxy.set_fault(Fault::Delay(delay));

    let started = Instant::now();
    let answer = reqwest::get(format!(
        "{}/ferroehr/rest/openehr/v1/ehr/7f4c",
        proxy.origin()
    ))
    .await
    .unwrap();
    assert!(
        started.elapsed() >= delay,
        "the answer comes no sooner than the delay"
    );
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "the delayed request is forwarded"
    );
    assert_eq!(
        node.received_requests().await.unwrap().len(),
        1,
        "the node received it once"
    );
}

#[tokio::test]
async fn a_delay_past_the_client_budget_is_a_time_out() {
    let node = node().await;
    let proxy = CapturingProxy::start(node.uri()).await.unwrap();
    proxy.set_fault(Fault::Delay(Duration::from_secs(5)));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let outcome = client
        .get(format!(
            "{}/ferroehr/rest/openehr/v1/ehr/7f4c",
            proxy.origin()
        ))
        .send()
        .await;
    assert!(
        outcome.is_err_and(|error| error.is_timeout()),
        "a client whose budget is shorter than the delay times out"
    );
}

#[tokio::test]
async fn a_refuse_fault_closes_the_connection_without_an_answer() {
    let node = node().await;
    let proxy = CapturingProxy::start(node.uri()).await.unwrap();
    proxy.set_fault(Fault::Refuse);

    let outcome = reqwest::get(format!(
        "{}/ferroehr/rest/openehr/v1/ehr/7f4c",
        proxy.origin()
    ))
    .await;
    assert!(
        outcome.is_err(),
        "a refused connection yields no answer at all"
    );
    assert!(
        node.received_requests().await.unwrap().is_empty(),
        "a refused request never reaches the node"
    );
    assert!(
        proxy.journal().is_empty(),
        "a connection closed unread leaves no capture"
    );
    let refused = proxy.refused();
    assert!(refused > 0, "the refused connection is counted");

    proxy.clear_fault();
    let answer = reqwest::get(format!(
        "{}/ferroehr/rest/openehr/v1/ehr/7f4c",
        proxy.origin()
    ))
    .await
    .unwrap();
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "clearing the fault serves again"
    );
    assert_eq!(
        refused,
        proxy.refused(),
        "a served connection is not counted"
    );
}

#[tokio::test]
async fn a_proxy_that_refused_nothing_counts_nothing() {
    let node = node().await;
    let proxy = CapturingProxy::start(node.uri()).await.unwrap();
    proxy.set_fault(Fault::Refuse);
    assert_eq!(0, proxy.refused(), "no connection was tried");
}

#[tokio::test]
async fn an_unreachable_node_is_a_bad_gateway() {
    // A dropped `MockServer` returns to wiremock's pool still serving, so the
    // node is the base nothing can listen on.
    let proxy = CapturingProxy::start(ferrofed_testkit::unreachable::BASE.to_owned())
        .await
        .unwrap();

    let answer = reqwest::get(format!(
        "{}/ferroehr/rest/openehr/v1/ehr/7f4c",
        proxy.origin()
    ))
    .await
    .unwrap();
    assert_eq!(
        answer.status(),
        StatusCode::BAD_GATEWAY,
        "a node that does not answer is a 502 from the proxy"
    );
}

#[tokio::test]
async fn faults_are_per_proxy() {
    let first = node().await;
    let second = node().await;
    let a = CapturingProxy::start(first.uri()).await.unwrap();
    let b = CapturingProxy::start(second.uri()).await.unwrap();
    b.set_fault(Fault::Status(StatusCode::SERVICE_UNAVAILABLE));

    let via_a = reqwest::get(format!("{}/ferroehr/rest/openehr/v1/ehr/7f4c", a.origin()))
        .await
        .unwrap();
    let via_b = reqwest::get(format!("{}/ferroehr/rest/openehr/v1/ehr/7f4c", b.origin()))
        .await
        .unwrap();
    assert_eq!(
        via_a.status(),
        StatusCode::OK,
        "a fault on one proxy leaves the other forwarding"
    );
    assert_eq!(
        via_b.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "the faulted proxy answers its status"
    );
}
