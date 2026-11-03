// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a node is sent and where: the request id the gateway minted, byte
//! for byte (CP-26), only the headers of the inventory and the query only in
//! the body (§5.4.1, N33), the base URL verbatim (N28), and one client per
//! endpoint with its own credentials.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::dispatch::{
    NodeClients, NodeQuery, REQUEST_ID_HEADER, SetupError, SharedCredentials,
};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_registry::id::EndpointId;
use ferrofed_testkit::mock::Server;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::Credentials;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    EMPTY_RESULT_SET, NODE_AQL, TestResult, client_at, error_text, json, node_answering, received,
    snapshot, transport, within,
};

// conformance: CP-26
#[tokio::test]
async fn the_request_id_a_node_receives_is_the_minted_outbound_id_byte_for_byte() -> TestResult {
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/openehr", server.uri()))?;
    let id = OutboundId::mint();
    let options = within(Duration::from_secs(5))?.with_request_id(id);
    client.query(&NodeQuery::new(NODE_AQL), &options).await?;
    let requests = received(&server).await?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one request, got {}", requests.len()).into());
    };
    let sent: Vec<&[u8]> = request
        .headers
        .get_all(REQUEST_ID_HEADER)
        .iter()
        .map(http::HeaderValue::as_bytes)
        .collect();
    assert_eq!(vec![id.to_string().as_bytes()], sent, "one field line");
    Ok(())
}

/// Every header name a node request may carry, lowercase, with where its
/// value comes from: the inventory of `ferrofed_engine::outbound_id`.
const INVENTORY: [&str; 8] = [
    "accept",                    // the client runtime's default
    "accept-encoding",           // the HTTP engine, from its decompression features
    "authorization",             // the endpoint's configured onward credential
    "content-length",            // the HTTP engine, from the composed body
    "content-type",              // the client runtime, for the JSON body
    "host",                      // the HTTP engine, from the registry URL
    "openehr-federation-client", // the caller's identity, signed for the node
    "x-request-id",              // the minted outbound id
];

// conformance: CP-26
#[tokio::test]
async fn every_header_a_node_receives_is_in_the_inventory() -> TestResult {
    let server = node_answering("/openehr", json(200, EMPTY_RESULT_SET)).await;
    let client = client_at(&format!("{}/openehr", server.uri()))?
        .with_credentials_provider(Arc::new(Credentials::bearer("synthetic-token")));
    let options = within(Duration::from_secs(5))?.with_request_id(OutboundId::mint());
    client.query(&NodeQuery::new(NODE_AQL), &options).await?;
    let requests = received(&server).await?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one request, got {}", requests.len()).into());
    };
    let names: Vec<&str> = {
        let mut names: Vec<&str> = request
            .headers
            .keys()
            .map(http::HeaderName::as_str)
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    };
    assert_eq!(INVENTORY.to_vec(), names, "no header outside the inventory");
    for name in INVENTORY {
        assert_eq!(
            1,
            request.headers.get_all(name).iter().count(),
            "{name} is one field line"
        );
    }
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
    let id = OutboundId::mint();
    let options = within(Duration::from_secs(5))?.with_request_id(id);
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
        Some(id.to_string().as_str())
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
    let options = within(Duration::from_secs(5))?.with_request_id(OutboundId::mint());
    client.query(&NodeQuery::new(aql), &options).await?;
    let requests = received(&server).await?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one request, got {}", requests.len()).into());
    };
    assert!(!request.url.as_str().contains(sentinel), "{}", request.url);
    for (name, value) in &request.headers {
        let carries = value
            .as_bytes()
            .windows(sentinel.len())
            .any(|window| window == sentinel.as_bytes());
        assert!(
            !name.as_str().contains(sentinel) && !carries,
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
    let first = Server::start().await;
    let second = Server::start().await;
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
