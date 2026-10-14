// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The row ceiling: every query a node is sent carries an explicit `fetch`,
//! so no node applies its own default page (ITS-REST Query API `Fetch`: "the
//! default depends on the implementation"), and a node that answers more
//! rows than the ceiling is `node-error`, never an answer read as complete
//! (§11.1, §11.5).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::{Contact, DispatchOptions, NodeClient, NodeClients, NodeQuery};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::ReqwestTransport;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic node query with no `LIMIT`, scoped to an `ehr_id` under no
/// real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// The ceiling of the fixture clients.
const CEILING: u32 = 3;

/// A node answering every query with `rows` one-cell rows.
async fn node_with(rows: usize) -> Server {
    let cells: Vec<String> = (0..rows).map(|row| format!(r#"["row-{row}"]"#)).collect();
    let body = format!(
        r##"{{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{{"name":"#0","path":"c/uid/value"}}],"rows":[{}]}}"##,
        cells.join(",")
    );
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// The client of the one endpoint at `url`, its clients holding answers to
/// [`CEILING`] rows.
fn client_at(url: &str) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let document = format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
    );
    let snapshot = RegistrySnapshot::from_toml_str(&document)?;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(10))?;
    let ceiling = NonZeroU32::new(CEILING).ok_or("a ceiling is positive")?;
    let clients = NodeClients::from_snapshot(&snapshot, &transport, &BTreeMap::new())?
        .with_row_ceiling(ceiling);
    Ok(clients
        .get(&EndpointId::new("node-a-pub")?)
        .ok_or("the endpoint has a client")?
        .clone())
}

/// Options with a deadline five seconds from now.
fn options() -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    Ok(DispatchOptions::new(
        deadline,
        crate::conveyed::conveyance(),
    ))
}

/// The body of the one request `server` received.
async fn sent_body(server: &Server) -> Result<String, Box<dyn Error>> {
    let requests = server.received_requests().await.unwrap_or_default();
    let [request] = requests.as_slice() else {
        return Err(format!("expected one request, got {}", requests.len()).into());
    };
    Ok(String::from_utf8(request.body.clone())?)
}

#[tokio::test]
async fn a_query_with_no_fetch_is_sent_the_ceiling_plus_one() -> TestResult {
    let server = node_with(0).await;
    let client = client_at(&server.uri())?;
    client.query(&NodeQuery::new(NODE_AQL), &options()?).await?;
    let body = sent_body(&server).await?;
    assert!(body.contains(r#""fetch":4"#), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_query_with_a_fetch_keeps_it() -> TestResult {
    let server = node_with(0).await;
    let client = client_at(&server.uri())?;
    client
        .query(&NodeQuery::new(NODE_AQL).with_fetch(25), &options()?)
        .await?;
    let body = sent_body(&server).await?;
    assert!(body.contains(r#""fetch":25"#), "{body}");
    Ok(())
}

// NOTE: §11.5: a node that filled the ceiling may hold more rows, so its
// share is unknown and is never merged as if it were whole.
#[tokio::test]
async fn a_node_that_fills_the_ceiling_is_node_error() -> TestResult {
    let server = node_with(4).await;
    let client = client_at(&server.uri())?;
    let reply = client.query(&NodeQuery::new(NODE_AQL), &options()?).await?;
    assert_eq!(reply.status(), EndpointStatus::NodeError);
    assert_eq!(reply.contact(), Contact::Answered(StatusCode::OK));
    let outcome = reply.outcome();
    let Some(ErrorDetail::Text(message)) = outcome.error() else {
        return Err("a node-error carries its message".into());
    };
    assert!(message.contains("3 rows"), "{message}");
    assert!(
        !message.contains("row-"),
        "no node data is quoted: {message}"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_that_answers_the_ceiling_is_active() -> TestResult {
    let server = node_with(3).await;
    let client = client_at(&server.uri())?;
    let reply = client.query(&NodeQuery::new(NODE_AQL), &options()?).await?;
    assert_eq!(reply.status(), EndpointStatus::Active);
    Ok(())
}

#[tokio::test]
async fn a_query_with_a_fetch_is_not_held_to_the_ceiling() -> TestResult {
    let server = node_with(5).await;
    let client = client_at(&server.uri())?;
    let reply = client
        .query(&NodeQuery::new(NODE_AQL).with_fetch(5), &options()?)
        .await?;
    assert_eq!(
        reply.status(),
        EndpointStatus::Active,
        "its LIMIT bounds it, and the merge checks the rows against that"
    );
    Ok(())
}
