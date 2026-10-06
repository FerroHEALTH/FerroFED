// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ehrId` a confined conveyance tells a node, held by the outbound gate
//! to that node's own `ehr_id`: the one the request to it is composed for,
//! never another node's, so the one patient carrier a node is told is the
//! node-local id N33 lets locate it (§5.4.1, N33, §12.5). A request the
//! gate refuses reaches no node. No specification defines a patient-confined
//! grant across nodes, so the confinement is FerroFED's own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::time::{Duration, Instant};

use ferrofed_engine::conveyance::confinement::Confinement;
use ferrofed_engine::conveyance::{ConveyanceError, HEADER};
use ferrofed_engine::dispatch::{DispatchError, DispatchOptions, NodeClient, NodeQuery};
use ferrofed_engine::single_node::forward::{ClientRequest, ForwardError};
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mock::Server;
use http::{HeaderMap, Method};
use openehr_its::rest::client::ReqwestTransport;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed;

type TestResult = Result<(), Box<dyn Error>>;

/// The confined patient's `ehr_id` at the node under test.
const OWN: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The confined patient's `ehr_id` at another node.
const OTHERS: &str = "1111bbbb-1111-4111-8111-111111111111";

/// The endpoint under test.
const ENDPOINT: &str = "node-a-pub";

/// The client of the one endpoint [`ENDPOINT`] at `url`.
fn client(url: &str) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let document = format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"{ENDPOINT}\"\nnode = \"node-a\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
    );
    let snapshot = RegistrySnapshot::from_toml_str(&document)?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(
        endpoint,
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
    )?)
}

/// Options whose conveyance is confined to the patient whose `ehr_id` the
/// confinement holds at [`ENDPOINT`] is `told`.
fn confined_to(told: &str) -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    let confinement = Confinement::new(
        EndpointId::new(ENDPOINT)?,
        BTreeMap::from([(EndpointId::new(ENDPOINT)?, EhrId::new(told)?)]),
    );
    Ok(DispatchOptions::new(
        deadline,
        conveyed::conveyance().with_confinement(confinement),
    ))
}

/// A node answering `verb` at `at` with `200` and `body`.
async fn node(verb: &str, at: &str, body: &str) -> Server {
    let server = Server::start().await;
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// The node answering every query with an empty result set.
async fn query_node() -> Server {
    node(
        "POST",
        "/v1/query/aql",
        r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##,
    )
    .await
}

/// The node query scoped to `ehr_id`.
fn scoped_to(ehr_id: &str) -> Result<NodeQuery, Box<dyn Error>> {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{ehr_id}'"
    );
    Ok(NodeQuery::new(aql)
        .with_scope(EhrId::new(ehr_id)?.hier_object_id())
        .with_width(1))
}

/// The `ehrId` of every conveyance `server` received, read without
/// verifying it.
async fn told(server: &Server) -> Result<Vec<Option<String>>, Box<dyn Error>> {
    #[derive(serde::Deserialize)]
    struct Told {
        #[serde(rename = "ehrId")]
        ehr_id: Option<String>,
    }
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut told = Vec::new();
    for request in requests {
        let token = request
            .headers
            .get(HEADER)
            .ok_or("a conveyance")?
            .to_str()?;
        let claims: Told = jsonwebtoken::dangerous::insecure_decode_claims(token)?;
        told.push(claims.ehr_id);
    }
    Ok(told)
}

/// Whether `error` is the gate's refusal of a conveyance that tells a node
/// another `ehr_id` than the one its request is composed for.
fn not_own(error: &ConveyanceError) -> bool {
    matches!(error, ConveyanceError::NotOwn(endpoint) if endpoint.as_str() == ENDPOINT)
}

// NOTE: §5.4.1, N33: the node is told the ehr_id its own query is scoped to.
#[tokio::test]
async fn a_confined_query_tells_its_node_its_own_ehr_id() -> TestResult {
    let server = query_node().await;
    client(&server.uri())?
        .query(&scoped_to(OWN)?, &confined_to(OWN)?)
        .await?;
    assert_eq!(vec![Some(OWN.to_owned())], told(&server).await?);
    Ok(())
}

// NOTE: §5.4.1, N33, §12.5: a conveyance telling a node another node's ehr_id is refused
// by the outbound gate, so nothing reaches the node.
#[tokio::test]
async fn the_gate_refuses_a_query_conveying_another_node_s_ehr_id() -> TestResult {
    let server = query_node().await;
    let client = client(&server.uri())?;
    let sent = client.query(&scoped_to(OWN)?, &confined_to(OTHERS)?).await;
    assert!(
        matches!(&sent, Err(DispatchError::Conveyance { source, .. }) if not_own(source)),
        "{sent:?}"
    );
    let unscoped = NodeQuery::new("SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c");
    let sent = client.query(&unscoped, &confined_to(OWN)?).await;
    assert!(
        matches!(&sent, Err(DispatchError::Conveyance { source, .. }) if not_own(source)),
        "a query composed for no ehr_id: {sent:?}"
    );
    assert!(told(&server).await?.is_empty(), "nothing reached the node");
    Ok(())
}

/// The routed read of the EHR `ehr_id`.
fn read_of(ehr_id: &str) -> ClientRequest {
    ClientRequest {
        method: Method::GET,
        path: format!("/ehr/{ehr_id}"),
        query: None,
        headers: HeaderMap::new(),
        body: Vec::new(),
    }
}

// NOTE: §5.4.1, N33, §12.5: a routed request is told the ehr_id its path names, and a
// conveyance telling another node's is refused with nothing sent.
#[tokio::test]
async fn the_gate_holds_a_routed_read_to_the_node_s_own_ehr_id() -> TestResult {
    let server = node("GET", &format!("/v1/ehr/{OWN}"), "{}").await;
    let client = client(&server.uri())?;
    let sent = client.forward(read_of(OWN), &confined_to(OTHERS)?).await;
    assert!(
        matches!(&sent, Err(ForwardError::Conveyance { source, .. }) if not_own(source)),
        "{sent:?}"
    );
    assert!(told(&server).await?.is_empty(), "nothing reached the node");
    client.forward(read_of(OWN), &confined_to(OWN)?).await?;
    assert_eq!(vec![Some(OWN.to_owned())], told(&server).await?);
    Ok(())
}
