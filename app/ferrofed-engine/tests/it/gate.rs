// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound gate (§5.4.1, N33): a request that would carry a withheld
//! patient identifier is never sent, whichever part of it carries the value,
//! and the refusal names the part and never the value. Asserted on what the
//! mock node received.

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::{
    DispatchError, DispatchOptions, NodeClient, NodeClients, NodeQuery,
};
use ferrofed_engine::fanout::{Budget, FanOutError, Plan, fan_out};
use ferrofed_engine::hygiene::{Part, Withheld};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_its::rest::client::ReqwestTransport;
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic patient identifier, with a quote so the AQL-escaped form
/// differs from the raw one.
const PATIENT: &str = "O'Sentinel-5521";

/// A clean node query, keyed on the node's own `ehr_id` alone.
const CLEAN: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// The node query with the identifier as the printer writes a literal.
const LEAKING_ESCAPED: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398' AND c/name/value = 'O\\'Sentinel-5521'";

fn withheld() -> Withheld {
    Withheld::new([SecretString::from(PATIENT)])
}

fn registry(url: &str) -> Result<RegistrySnapshot, Box<dyn Error>> {
    Ok(RegistrySnapshot::from_toml_str(&format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
    ))?)
}

fn client(snapshot: &RegistrySnapshot) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(
        endpoint,
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
    )?)
}

fn options() -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    Ok(DispatchOptions::new(deadline).with_withheld(Arc::new(withheld())))
}

async fn node() -> MockServer {
    let server = MockServer::start().await;
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

async fn requests_at(server: &MockServer) -> Result<usize, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .len())
}

// conformance: CP-26
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn an_identifier_in_the_aql_text_is_never_sent() -> TestResult {
    let server = node().await;
    let snapshot = registry(&server.uri())?;
    for aql in [
        LEAKING_ESCAPED.to_owned(),
        format!("{CLEAN} AND c/name/value = '{PATIENT}'"),
    ] {
        let refused = client(&snapshot)?
            .query(&NodeQuery::new(aql), &options()?)
            .await;
        let Err(DispatchError::Withheld { part, .. }) = refused else {
            return Err(format!("the gate let a leaking query through: {refused:?}").into());
        };
        assert_eq!(Part::Aql, part);
    }
    assert_eq!(0, requests_at(&server).await?, "nothing reached the node");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn an_identifier_in_a_header_the_gateway_adds_is_never_sent() -> TestResult {
    let server = node().await;
    let snapshot = registry(&server.uri())?;
    let refused = client(&snapshot)?
        .query(
            &NodeQuery::new(CLEAN),
            &options()?.with_request_id(format!("req-{PATIENT}")),
        )
        .await;
    let Err(DispatchError::Withheld { part, .. }) = refused else {
        return Err(format!("the gate let a leaking header through: {refused:?}").into());
    };
    assert_eq!(Part::Header("X-Request-Id"), part);
    assert_eq!(0, requests_at(&server).await?, "nothing reached the node");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_clean_request_is_sent_with_identifiers_withheld() -> TestResult {
    let server = node().await;
    let snapshot = registry(&server.uri())?;
    client(&snapshot)?
        .query(
            &NodeQuery::new(CLEAN).with_fetch(10),
            &options()?.with_request_id("req-1"),
        )
        .await?;
    assert_eq!(1, requests_at(&server).await?, "the clean query was sent");
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn the_refusal_names_the_part_and_never_the_value() -> TestResult {
    let server = node().await;
    let snapshot = registry(&server.uri())?;
    let refused = client(&snapshot)?
        .query(&NodeQuery::new(LEAKING_ESCAPED), &options()?)
        .await
        .err()
        .ok_or("the gate refused")?;
    for shown in [refused.to_string(), format!("{refused:?}")] {
        assert!(
            !shown.contains("Sentinel"),
            "the refusal quotes nothing: {shown}"
        );
    }
    assert!(
        refused.to_string().contains("the AQL text"),
        "the refusal names the part: {refused}"
    );
    Ok(())
}

// conformance: CP-26
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_fan_out_with_a_leaking_query_fails_closed_and_asks_nobody() -> TestResult {
    let server = node().await;
    let snapshot = registry(&server.uri())?;
    let clients = NodeClients::from_snapshot(
        &snapshot,
        &ReqwestTransport::with_timeout(Duration::from_secs(10))?,
        &std::collections::BTreeMap::new(),
    )?;
    let plan = Plan::new().withholding(withheld()).dispatch(
        EndpointId::new("node-a-pub")?,
        NodeQuery::new(LEAKING_ESCAPED),
    )?;
    let answer = fan_out(
        &clients,
        &snapshot,
        plan,
        Budget::new(Duration::from_secs(2), Duration::from_secs(3))?,
        None,
    )
    .await;
    assert!(
        matches!(
            answer,
            Err(FanOutError::Dispatch(DispatchError::Withheld { .. }))
        ),
        "the query fails closed on the gateway's side: {answer:?}"
    );
    assert_eq!(0, requests_at(&server).await?, "nothing reached the node");
    Ok(())
}
