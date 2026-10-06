// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A node answer the HTTP engine stopped reading at the gateway's bound: the
//! node answered, so its endpoint is `node-error` with its status, the query
//! fails `424` under all-or-nothing and `complete` is false (§11.1, §11.4,
//! N37), and a forwarded request has no answer to pass on.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::oversized::Oversized;
use ferrofed_engine::dispatch::{Contact, DispatchOptions, NodeClients, NodeQuery};
use ferrofed_engine::fanout::{Budget, Plan, Verdict, fan_out};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::single_node::forward::{ClientRequest, ForwardError, HeldRequest};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::{HeaderMap, Method, StatusCode};
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{Transport, TransportError};

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic node query, scoped to an `ehr_id` under no real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// The bound the engine below stops at.
const LIMIT: usize = 1024;

/// An engine that reports every answer as one past [`LIMIT`], as a bounded
/// engine does.
#[derive(Debug, Clone)]
struct Bounded;

#[async_trait::async_trait]
impl Transport for Bounded {
    async fn send(
        &self,
        _request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, TransportError> {
        Err(TransportError::Send {
            source: Box::new(Oversized::new(StatusCode::OK, LIMIT)),
        })
    }
}

/// The clients of one endpoint, `node-a-pub`, over [`Bounded`].
fn bounded() -> Result<(RegistrySnapshot, NodeClients<Bounded>), Box<dyn Error>> {
    let snapshot = RegistrySnapshot::from_toml_str(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"https://cdr-a.example.org/openehr\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
    )?;
    let clients = NodeClients::from_snapshot(&snapshot, &Bounded, &BTreeMap::new())?;
    Ok((snapshot, clients))
}

// conformance: CP-30
#[tokio::test]
async fn an_answer_past_the_bound_is_node_error_and_fails_the_query_424() -> TestResult {
    let (snapshot, clients) = bounded()?;
    let endpoint = EndpointId::new("node-a-pub")?;
    let plan = Plan::new().dispatch(endpoint.clone(), NodeQuery::new(NODE_AQL))?;
    let budget = Budget::new(Duration::from_secs(2), Duration::from_secs(5))?;
    let answer = fan_out(
        &clients,
        &snapshot,
        plan,
        budget,
        (&crate::conveyed::conveyance(), Some(OutboundId::mint())),
    )
    .await?;
    assert_eq!(Verdict::NodeFailed, answer.verdict());
    assert_eq!(StatusCode::FAILED_DEPENDENCY, answer.status());
    assert!(!answer.federation().complete(), "§11.4: complete is false");
    let record = answer
        .federation()
        .endpoints()
        .first()
        .ok_or("the endpoint is reported")?;
    assert_eq!(EndpointStatus::NodeError, record.status());
    let Some(ErrorDetail::Text(error)) = record.outcome().error() else {
        return Err(format!("a node-error carries its error: {record:?}").into());
    };
    assert!(error.contains(&LIMIT.to_string()), "{error}");
    let contacts: Vec<Contact> = answer.contacts().map(|(_, contact)| contact).collect();
    assert_eq!(vec![Contact::Answered(StatusCode::OK)], contacts);
    Ok(())
}

#[tokio::test]
async fn a_forwarded_answer_past_the_bound_has_nothing_to_pass_on() -> TestResult {
    let (_, clients) = bounded()?;
    let client = clients
        .get(&EndpointId::new("node-a-pub")?)
        .ok_or("the endpoint has a client")?;
    let request = HeldRequest::hold(ClientRequest {
        method: Method::GET,
        path: "/ehr/7d44b88c-4199-4bad-97dc-d78268e01398".to_owned(),
        query: None,
        headers: HeaderMap::new(),
        body: Vec::new(),
    })?;
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    let options = DispatchOptions::new(deadline, crate::conveyed::conveyance());
    let outcome = client.forward_held(request, &options).await;
    let Err(ForwardError::Oversized { status, limit, .. }) = &outcome else {
        return Err(format!("an oversized answer is no answer: {outcome:?}").into());
    };
    assert_eq!((StatusCode::OK, LIMIT), (*status, *limit));
    assert_eq!(
        Contact::Answered(StatusCode::OK),
        Contact::of_forwarded(&outcome)
    );
    Ok(())
}
