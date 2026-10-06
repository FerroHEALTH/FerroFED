// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The address an access record names (Annex II 3.2): the connection's peer,
//! or behind a trusted reverse proxy the client the proxy names; a forwarded
//! header from any other peer never reaches the record (RFC 7239 §8.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::net::SocketAddr;

use axum::extract::connect_info::MockConnectInfo;
use ferrofed_server::client_address::{ForwardedHeader, Forwarding};
use ferrofed_server::listener::Peer;
use http::StatusCode;

use super::{LAB_REPORT, TestResult, accesses, composition, gateway_under, node_with_rows};
use crate::facade::{NAMESPACE, PATIENT, body, post, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::call;
use ferrofed_testkit::atna_feed::FeedRepository;

const UID: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The proxy every test connects from.
const PROXY: &str = "127.0.0.1";

/// The client a forwarded header names.
const CLIENT: &str = "192.0.2.60";

/// The network addresses of every agent of the one access record a patient
/// query from `PROXY` with `forwarded` writes, the gateway trusting
/// `trusted`.
async fn recorded(
    trusted: &[&str],
    forwarded: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let (a, b) = (
        node_with_rows(&[composition(LAB_REPORT, UID)]).await,
        node_with_rows(&[]).await,
    );
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let mut server = settings_with_room();
    let proxies = trusted
        .iter()
        .map(|proxy| proxy.parse::<ipnet::IpNet>())
        .collect::<Result<Vec<_>, _>>()?;
    server.forwarding = Forwarding::new(proxies, ForwardedHeader::Forwarded);
    let app = gateway_under(dir.path(), (&a.uri(), &b.uri()), &repository, "", &server)?.layer(
        MockConnectInfo(Peer(SocketAddr::new(PROXY.parse()?, 40000))),
    );
    let aql = format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let mut request = post(body(&aql)?)?;
    request
        .headers_mut()
        .insert(http::header::FORWARDED, forwarded.parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    assert_eq!(1, records.len(), "one record of the access");
    let record = records.first().ok_or("an access record")?;
    Ok(record
        .get("agent")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|agent| agent.pointer("/network/address")?.as_str())
        .map(str::to_owned)
        .collect())
}

#[tokio::test]
async fn a_trusted_proxy_names_the_client_in_the_access_record() -> TestResult {
    let addresses = recorded(&["127.0.0.1/32"], &format!("for={CLIENT}")).await?;
    assert!(
        addresses.iter().any(|address| address == CLIENT),
        "the client the proxy names: {addresses:?}"
    );
    assert!(
        !addresses.iter().any(|address| address == PROXY),
        "not the proxy: {addresses:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_forwarded_header_from_an_untrusted_peer_never_reaches_the_record() -> TestResult {
    let addresses = recorded(&[], &format!("for={CLIENT}")).await?;
    assert!(
        addresses.iter().any(|address| address == PROXY),
        "the peer: {addresses:?}"
    );
    assert!(
        !addresses.iter().any(|address| address == CLIENT),
        "never the forwarded address: {addresses:?}"
    );
    Ok(())
}
