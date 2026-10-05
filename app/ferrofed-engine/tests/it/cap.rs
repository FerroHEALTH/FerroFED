// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-endpoint in-flight cap: a request past it waits for a slot until
//! its own deadline, and one still waiting then is `time-out` with nothing
//! sent (§11.5, N38), reported as capped so the gateway counts it as a
//! refusal of the cap; another endpoint is never held up by it.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::cap::CAPPED;
use ferrofed_engine::dispatch::{Contact, DispatchOptions, NodeClients, NodeQuery};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mock::Server;
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::ReqwestTransport;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic node query, scoped to an `ehr_id` under no real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// An empty ITS-REST `RESULT_SET`.
const EMPTY_RESULT_SET: &str = r##"{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// How long the slow node keeps the first request.
const HOLD: Duration = Duration::from_millis(1_500);

/// A node answering the query with an empty result set after `delay`.
async fn node_after(delay: Duration) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json")
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

/// The clients of one node with `endpoints` as its `(endpoint_id, url)`
/// pairs, each capped at `cap` requests in flight.
fn capped(
    endpoints: &[(&str, &str)],
    cap: u32,
) -> Result<NodeClients<ReqwestTransport>, Box<dyn Error>> {
    let mut document = String::from(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n",
    );
    for (id, url) in endpoints {
        write!(
            document,
            "\n[[endpoint]]\nid = \"{id}\"\nnode = \"node-a\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
        )?;
    }
    let snapshot = RegistrySnapshot::from_toml_str(&document)?;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(10))?;
    let cap = NonZeroU32::new(cap).ok_or("a cap is positive")?;
    Ok(
        NodeClients::from_snapshot(&snapshot, &transport, &BTreeMap::new())?
            .with_in_flight_cap(cap),
    )
}

/// Options with a deadline `budget` from now.
fn within(budget: Duration) -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(budget)
        .ok_or("the deadline is past the platform clock")?;
    Ok(DispatchOptions::new(
        deadline,
        crate::conveyed::conveyance(),
    ))
}

/// Waits until `server` received `count` requests, or fails after a bound.
async fn until_received(server: &Server, count: usize) -> TestResult {
    for _ in 0..500 {
        if server.received_requests().await.unwrap_or_default().len() >= count {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("the node never received {count} requests").into())
}

// NOTE: §11.5, N38: a request abandoned at its per-node deadline is time-out,
// here with nothing sent, since it never had a slot.
#[tokio::test]
async fn a_request_past_the_cap_ends_time_out_at_its_deadline_with_nothing_sent() -> TestResult {
    let slow = node_after(HOLD).await;
    let clients = capped(&[("node-a-pub", &slow.uri())], 1)?;
    let endpoint = EndpointId::new("node-a-pub")?;
    let client = clients
        .get(&endpoint)
        .ok_or("the endpoint has a client")?
        .clone();
    let options = within(Duration::from_secs(5))?;
    let first = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .query(&NodeQuery::new(NODE_AQL), &options)
                .await
                .map_err(|error| error.to_string())
        }
    });
    until_received(&slow, 1).await?;

    let started = Instant::now();
    let second = client
        .query(
            &NodeQuery::new(NODE_AQL),
            &within(Duration::from_millis(300))?,
        )
        .await?;
    assert_eq!(EndpointStatus::TimeOut, second.status());
    assert_eq!(Contact::Capped, second.contact());
    assert!(!second.contact().sent(), "nothing was sent");
    assert_eq!(
        Some(&ErrorDetail::Text(CAPPED.to_owned())),
        second.outcome().error()
    );
    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "the request waited for a slot until its deadline"
    );
    assert_eq!(
        1,
        slow.received_requests().await.unwrap_or_default().len(),
        "the capped request never reached the node"
    );

    let first = first.await??;
    assert_eq!(
        EndpointStatus::Active,
        first.status(),
        "the slot holder is served"
    );
    let third = client
        .query(&NodeQuery::new(NODE_AQL), &within(Duration::from_secs(5))?)
        .await?;
    assert_eq!(
        EndpointStatus::Active,
        third.status(),
        "a freed slot is taken again"
    );
    Ok(())
}

#[tokio::test]
async fn a_full_cap_holds_up_no_other_endpoint() -> TestResult {
    let slow = node_after(HOLD).await;
    let quick = node_after(Duration::ZERO).await;
    let clients = capped(
        &[("node-a-pub", &slow.uri()), ("node-a-alt", &quick.uri())],
        1,
    )?;
    let held = clients
        .get(&EndpointId::new("node-a-pub")?)
        .ok_or("the slow endpoint has a client")?
        .clone();
    let options = within(Duration::from_secs(5))?;
    let first = tokio::spawn(async move {
        held.query(&NodeQuery::new(NODE_AQL), &options)
            .await
            .map_err(|error| error.to_string())
    });
    until_received(&slow, 1).await?;

    let other = clients
        .get(&EndpointId::new("node-a-alt")?)
        .ok_or("the quick endpoint has a client")?
        .query(
            &NodeQuery::new(NODE_AQL),
            &within(Duration::from_millis(800))?,
        )
        .await?;
    assert_eq!(
        EndpointStatus::Active,
        other.status(),
        "the other endpoint is served at once"
    );
    first.await??;
    Ok(())
}

#[tokio::test]
async fn under_the_cap_every_request_is_sent_at_once() -> TestResult {
    let slow = node_after(Duration::from_millis(300)).await;
    let clients = capped(&[("node-a-pub", &slow.uri())], 2)?;
    let client = clients
        .get(&EndpointId::new("node-a-pub")?)
        .ok_or("the endpoint has a client")?;
    let (query, options) = (NodeQuery::new(NODE_AQL), within(Duration::from_secs(5))?);
    let (one, two) = tokio::join!(
        client.query(&query, &options),
        client.query(&query, &options)
    );
    assert_eq!(EndpointStatus::Active, one?.status());
    assert_eq!(EndpointStatus::Active, two?.status());
    assert_eq!(2, slow.received_requests().await.unwrap_or_default().len());
    Ok(())
}
