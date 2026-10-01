// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Node dispatch against a mock node: every generated outcome of
//! `POST {base}/v1/query/aql` and every `ClientError` kind maps to exactly one
//! §11.1 endpoint status (N16) or to a gateway-side `DispatchError`, the base
//! URL is used verbatim (N28), and nothing the client composes carries a value
//! that was not put in the body (§5.4.1, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::{
    DispatchError, DispatchOptions, NodeClient, NodeClients, NodeQuery, NodeReply,
    REQUEST_ID_HEADER, SetupError, SharedCredentials,
};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::{Endpoint, RegistrySnapshot};
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{
    Credentials, CredentialsError, CredentialsProvider, ReqwestTransport,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic node query, scoped to an `ehr_id` under no real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// An empty ITS-REST `RESULT_SET`.
const EMPTY_RESULT_SET: &str = r##"{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// A one-node federation with `endpoints` as its `(endpoint_id, url)` pairs.
fn snapshot(endpoints: &[(&str, &str)]) -> Result<RegistrySnapshot, Box<dyn Error>> {
    let mut document = String::from(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n",
    );
    for (id, url) in endpoints {
        write!(
            document,
            "\n[[endpoint]]\nid = \"{id}\"\nnode = \"node-a\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
        )?;
    }
    Ok(RegistrySnapshot::from_toml_str(&document)?)
}

/// The one endpoint of a single-endpoint snapshot.
fn only_endpoint(snapshot: &RegistrySnapshot) -> Result<&Endpoint, Box<dyn Error>> {
    snapshot
        .endpoints()
        .next()
        .ok_or_else(|| "the snapshot holds no endpoint".into())
}

/// The engine over the default `reqwest` transport.
fn transport() -> Result<ReqwestTransport, Box<dyn Error>> {
    Ok(ReqwestTransport::with_timeout(Duration::from_secs(10))?)
}

/// A client for an endpoint at `url`.
fn client_at(url: &str) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let snapshot = snapshot(&[("node-a-pub", url)])?;
    Ok(NodeClient::new(only_endpoint(&snapshot)?, transport()?)?)
}

/// Options with a deadline `budget` from now.
fn within(budget: Duration) -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(budget)
        .ok_or("the deadline is past the platform clock")?;
    Ok(DispatchOptions::new(deadline))
}

/// A mock node answering `POST {prefix}/v1/query/aql` with `answer`.
async fn node_answering(prefix: &str, answer: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("{prefix}/v1/query/aql")))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

/// A JSON answer with `status` and `body`.
fn json(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(body.as_bytes().to_vec(), "application/json")
}

/// Dispatches `NODE_AQL` to `server` under `/openehr` and returns the reply.
async fn dispatch_to(server: &MockServer) -> Result<NodeReply, Box<dyn Error>> {
    let client = client_at(&format!("{}/openehr", server.uri()))?;
    Ok(client
        .query(&NodeQuery::new(NODE_AQL), &within(Duration::from_secs(5))?)
        .await?)
}

/// The text of a reply's `error`, which every failure carries.
fn error_text(reply: &NodeReply) -> Result<String, Box<dyn Error>> {
    match reply.outcome().error() {
        Some(ErrorDetail::Text(text)) => Ok(text.clone()),
        Some(other) => Err(format!("an unexpected structured error: {other:?}").into()),
        None => Err("the failure carries no error".into()),
    }
}

/// The requests the mock node received.
async fn received(server: &MockServer) -> Result<Vec<Request>, Box<dyn Error>> {
    server
        .received_requests()
        .await
        .ok_or_else(|| "request recording is off".into())
}

#[tokio::test]
async fn a_result_set_is_active_and_carries_the_rows() -> TestResult {
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let reply = dispatch_to(&server).await?;
    assert_eq!(reply.status(), EndpointStatus::Active);
    match reply {
        NodeReply::Answered { result_set, .. } => {
            assert!(result_set.rows.is_empty());
            assert_eq!(result_set.columns.map(|columns| columns.len()), Some(1));
        }
        NodeReply::Failed { outcome } => {
            return Err(format!("expected a result set, got {outcome:?}").into());
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_documented_400_is_a_node_error_with_the_nodes_status_and_message() -> TestResult {
    let server = node_answering(
        "/openehr",
        json(400, r#"{"message":"unknown archetype path in WHERE"}"#),
    )
    .await;
    let reply = dispatch_to(&server).await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    let error = error_text(&reply)?;
    assert!(error.contains("400 Bad Request"), "{error}");
    assert!(error.contains("unknown archetype path in WHERE"), "{error}");
    Ok(())
}

#[tokio::test]
async fn a_documented_408_is_a_node_error_not_a_time_out() -> TestResult {
    let server = node_answering(
        "/openehr",
        json(408, r#"{"message":"query took too long"}"#),
    )
    .await;
    let reply = dispatch_to(&server).await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    assert!(error_text(&reply)?.contains("408 Request Timeout"));
    Ok(())
}

#[tokio::test]
async fn a_401_is_a_node_error() -> TestResult {
    let server = node_answering("/openehr", ResponseTemplate::new(401)).await;
    let reply = dispatch_to(&server).await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    assert_eq!(error_text(&reply)?, "the node answered 401 Unauthorized");
    Ok(())
}

#[tokio::test]
async fn a_403_is_a_node_error() -> TestResult {
    let server = node_answering("/openehr", ResponseTemplate::new(403)).await;
    let reply = dispatch_to(&server).await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    assert_eq!(error_text(&reply)?, "the node answered 403 Forbidden");
    Ok(())
}

#[tokio::test]
async fn a_5xx_is_a_node_error_never_offline() -> TestResult {
    let server = node_answering(
        "/openehr",
        ResponseTemplate::new(503).set_body_string("maintenance window"),
    )
    .await;
    let reply = dispatch_to(&server).await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    let error = error_text(&reply)?;
    assert!(error.contains("503 Service Unavailable"), "{error}");
    assert!(error.contains("maintenance window"), "{error}");
    Ok(())
}

#[tokio::test]
async fn an_undocumented_status_is_a_node_error_with_that_status() -> TestResult {
    let server = node_answering("/openehr", ResponseTemplate::new(404)).await;
    let reply = dispatch_to(&server).await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    assert_eq!(error_text(&reply)?, "the node answered 404 Not Found");
    Ok(())
}

#[tokio::test]
async fn a_200_that_is_not_a_result_set_is_a_node_error() -> TestResult {
    let server = node_answering("/openehr", json(200, r#"{"rows":"not rows"}"#)).await;
    let reply = dispatch_to(&server).await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    let error = error_text(&reply)?;
    assert!(error.contains("not an ITS-REST RESULT_SET"), "{error}");
    assert!(error.contains("rows"), "{error}");
    assert!(
        !error.contains("not rows"),
        "the defect report echoed the node's data: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn no_answer_before_the_deadline_is_a_time_out() -> TestResult {
    let server = node_answering(
        "/openehr",
        json(200, EMPTY_RESULT_SET).set_delay(Duration::from_secs(3)),
    )
    .await;
    let client = client_at(&format!("{}/openehr", server.uri()))?;
    let reply = client
        .query(
            &NodeQuery::new(NODE_AQL),
            &within(Duration::from_millis(200))?,
        )
        .await?;
    assert_eq!(reply.status(), EndpointStatus::TimeOut);
    assert!(error_text(&reply)?.starts_with("no answer before the deadline"));
    assert!(reply.outcome().latency_ms().is_some());
    Ok(())
}

#[tokio::test]
async fn a_deadline_already_passed_is_a_time_out_and_sends_nothing() -> TestResult {
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/openehr", server.uri()))?;
    let reply = client
        .query(
            &NodeQuery::new(NODE_AQL),
            &DispatchOptions::new(Instant::now()),
        )
        .await?;
    assert_eq!(reply.status(), EndpointStatus::TimeOut);
    assert!(received(&server).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_refused_connection_is_offline_with_a_reason() -> TestResult {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let client = client_at(&format!("http://{address}/openehr"))?;
    let reply = client
        .query(&NodeQuery::new(NODE_AQL), &within(Duration::from_secs(5))?)
        .await?;
    assert_eq!(reply.status(), EndpointStatus::Offline);
    let error = error_text(&reply)?;
    assert!(
        error.starts_with("the node could not be reached: "),
        "{error}"
    );
    assert!(
        error.len() > "the node could not be reached: ".len(),
        "the refusal carries no reason: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn a_broken_stream_is_offline() -> TestResult {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let closer = tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            drop(stream);
        }
    });
    let client = client_at(&format!("http://{address}/openehr"))?;
    let reply = client
        .query(&NodeQuery::new(NODE_AQL), &within(Duration::from_secs(5))?)
        .await?;
    closer.await?;
    assert_eq!(reply.status(), EndpointStatus::Offline);
    assert!(!error_text(&reply)?.is_empty());
    Ok(())
}

/// A provider that never produces a credential.
#[derive(Debug)]
struct NoCredential;

#[async_trait::async_trait]
impl CredentialsProvider for NoCredential {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        Err(CredentialsError::new("the token endpoint is unreachable"))
    }
}

#[tokio::test]
async fn a_missing_credential_is_a_dispatch_error_and_sends_nothing() -> TestResult {
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/openehr", server.uri()))?
        .with_credentials_provider(Arc::new(NoCredential));
    let failed = client
        .query(&NodeQuery::new(NODE_AQL), &within(Duration::from_secs(5))?)
        .await;
    assert!(
        matches!(failed, Err(DispatchError::Credentials { .. })),
        "{failed:?}"
    );
    assert!(received(&server).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_request_id_that_is_no_header_value_is_a_dispatch_error() -> TestResult {
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/openehr", server.uri()))?;
    let options = within(Duration::from_secs(5))?.with_request_id("line\nbreak");
    let failed = client.query(&NodeQuery::new(NODE_AQL), &options).await;
    assert!(
        matches!(failed, Err(DispatchError::Compose { .. })),
        "{failed:?}"
    );
    assert!(received(&server).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_base_url_is_used_verbatim_with_a_path_prefix() -> TestResult {
    let server = node_answering("/rest/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/rest/openehr", server.uri()))?;
    assert_eq!(client.base().path(), "/rest/openehr/v1");
    let reply = client
        .query(&NodeQuery::new(NODE_AQL), &within(Duration::from_secs(5))?)
        .await?;
    assert_eq!(reply.status(), EndpointStatus::Active);
    Ok(())
}

#[tokio::test]
async fn the_base_url_is_used_verbatim_without_a_prefix() -> TestResult {
    let server = node_answering("", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&server.uri())?;
    assert_eq!(client.base().path(), "/v1");
    let reply = client
        .query(&NodeQuery::new(NODE_AQL), &within(Duration::from_secs(5))?)
        .await?;
    assert_eq!(reply.status(), EndpointStatus::Active);
    Ok(())
}

#[tokio::test]
async fn a_trailing_slash_on_the_base_url_adds_no_empty_segment() -> TestResult {
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/openehr/", server.uri()))?;
    assert_eq!(client.base().path(), "/openehr/v1");
    let reply = client
        .query(&NodeQuery::new(NODE_AQL), &within(Duration::from_secs(5))?)
        .await?;
    assert_eq!(reply.status(), EndpointStatus::Active);
    Ok(())
}

#[tokio::test]
async fn the_request_id_and_the_page_travel_and_nothing_else_is_added() -> TestResult {
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/openehr", server.uri()))?;
    let options = within(Duration::from_secs(5))?.with_request_id("3f0b2a6e-request");
    let query = NodeQuery::new(NODE_AQL).with_offset(0).with_fetch(11);
    client.query(&query, &options).await?;
    let requests = received(&server).await?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one request, got {}", requests.len()).into());
    };
    assert_eq!(request.method.as_str(), "POST");
    assert_eq!(request.url.query(), None);
    assert_eq!(
        request
            .headers
            .get(REQUEST_ID_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("3f0b2a6e-request")
    );
    assert_eq!(request.headers.get("authorization"), None);
    let body = std::str::from_utf8(&request.body)?;
    assert!(body.contains(r#""offset":0"#), "{body}");
    assert!(body.contains(r#""fetch":11"#), "{body}");
    assert!(!body.contains("query_parameters"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_sentinel_in_the_query_reaches_only_the_body() -> TestResult {
    let sentinel = "SENTINEL-2.999.4711";
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/openehr", server.uri()))?;
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = '{sentinel}'"
    );
    let options = within(Duration::from_secs(5))?.with_request_id("3f0b2a6e-request");
    client.query(&NodeQuery::new(aql), &options).await?;
    let requests = received(&server).await?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one request, got {}", requests.len()).into());
    };
    assert!(!request.url.as_str().contains(sentinel), "{}", request.url);
    for (name, value) in &request.headers {
        let value = String::from_utf8_lossy(value.as_bytes());
        assert!(
            !name.as_str().contains(sentinel) && !value.contains(sentinel),
            "the header {name} carries the sentinel"
        );
    }
    assert!(std::str::from_utf8(&request.body)?.contains(sentinel));
    Ok(())
}

#[tokio::test]
async fn a_failure_report_never_echoes_the_query() -> TestResult {
    let sentinel = "SENTINEL-2.999.4712";
    let server = node_answering("/openehr", ResponseTemplate::new(500)).await;
    let client = client_at(&format!("{}/openehr", server.uri()))?;
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = '{sentinel}'"
    );
    let reply = client
        .query(&NodeQuery::new(aql), &within(Duration::from_secs(5))?)
        .await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    assert!(!format!("{reply:?}").contains(sentinel));
    assert!(!error_text(&reply)?.contains(sentinel));
    Ok(())
}

#[tokio::test]
async fn the_snapshot_gives_one_client_per_endpoint_with_its_own_credentials() -> TestResult {
    let first = MockServer::start().await;
    let second = MockServer::start().await;
    for server in [&first, &second] {
        Mock::given(method("POST"))
            .and(path("/openehr/v1/query/aql"))
            .respond_with(json(200, EMPTY_RESULT_SET))
            .mount(server)
            .await;
    }
    let snapshot = snapshot(&[
        ("node-a-one", &format!("{}/openehr", first.uri())),
        ("node-a-two", &format!("{}/openehr", second.uri())),
    ])?;
    let mut credentials: BTreeMap<EndpointId, SharedCredentials> = BTreeMap::new();
    credentials.insert(
        EndpointId::new("node-a-one")?,
        Arc::new(Credentials::bearer("synthetic-token")),
    );
    let clients = NodeClients::from_snapshot(&snapshot, &transport()?, &credentials)?;
    assert_eq!(clients.len(), 2);
    let options = within(Duration::from_secs(5))?;
    for client in clients.iter() {
        client.query(&NodeQuery::new(NODE_AQL), &options).await?;
    }
    let with = received(&first).await?;
    let without = received(&second).await?;
    assert_eq!(
        with.first()
            .and_then(|request| request.headers.get("authorization"))
            .and_then(|value| value.to_str().ok()),
        Some("Bearer synthetic-token")
    );
    assert_eq!(
        without
            .first()
            .map(|request| request.headers.contains_key("authorization")),
        Some(false)
    );
    Ok(())
}

#[test]
fn credentials_for_an_endpoint_the_snapshot_lacks_are_refused() -> TestResult {
    let snapshot = snapshot(&[("node-a-pub", "https://cdr-a.example.org/openehr")])?;
    let mut credentials: BTreeMap<EndpointId, SharedCredentials> = BTreeMap::new();
    credentials.insert(
        EndpointId::new("node-z-pub")?,
        Arc::new(Credentials::bearer("synthetic-token")),
    );
    let refused = NodeClients::from_snapshot(&snapshot, &transport()?, &credentials);
    assert!(
        matches!(refused, Err(SetupError::UnknownEndpoint { .. })),
        "{refused:?}"
    );
    Ok(())
}
