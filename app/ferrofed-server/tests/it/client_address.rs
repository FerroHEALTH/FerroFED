// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The address a request came from: the connection's peer, unless the peer
//! is a trusted proxy, and then the rightmost hop of the forwarded header
//! that is no trusted proxy. A header from any other peer changes nothing
//! (RFC 7239 §8.1). No specification governs the rest: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::routing::get;
use ferrofed_server::client_address::{self, ClientAddress, ForwardedHeader, Forwarding};
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::listener::Peer;
use http::{Request, StatusCode};

use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// Answers the address the request was named by, or `none`.
async fn named(request: axum::extract::Request) -> String {
    request.extensions().get::<ClientAddress>().map_or_else(
        || String::from("none"),
        |ClientAddress(address)| address.to_string(),
    )
}

/// The address a request from `peer` with `headers` is named by under
/// `forwarding`.
async fn client(
    forwarding: Forwarding,
    peer: &str,
    headers: &[(&str, &str)],
) -> Result<String, Box<dyn Error>> {
    let peer: SocketAddr = format!("{peer}:40000").parse()?;
    let app = Router::new()
        .route("/", get(named))
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(forwarding),
            client_address::attach,
        ))
        .layer(MockConnectInfo(Peer(peer)));
    let mut request = Request::get("/");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let (status, text) = call(app, request.body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "the probe route answers");
    Ok(text)
}

/// The forwarding of the proxies `proxies` naming the client in `header`.
fn trusting(proxies: &[&str], header: ForwardedHeader) -> Result<Forwarding, Box<dyn Error>> {
    let proxies = proxies
        .iter()
        .map(|proxy| proxy.parse::<ipnet::IpNet>())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Forwarding::new(proxies, header))
}

#[tokio::test]
async fn with_no_trusted_proxy_the_peer_names_every_request() -> TestResult {
    let named = client(
        Forwarding::default(),
        "198.51.100.7",
        &[
            ("forwarded", "for=192.0.2.60"),
            ("x-forwarded-for", "192.0.2.61"),
        ],
    )
    .await?;
    assert_eq!("198.51.100.7", named);
    Ok(())
}

#[tokio::test]
async fn a_trusted_proxy_names_the_client_in_forwarded() -> TestResult {
    let forwarding = trusting(&["127.0.0.1/32"], ForwardedHeader::Forwarded)?;
    let named = client(
        forwarding,
        "127.0.0.1",
        &[("forwarded", "for=192.0.2.60;proto=https;by=127.0.0.1")],
    )
    .await?;
    assert_eq!("192.0.2.60", named);
    Ok(())
}

#[tokio::test]
async fn a_header_from_an_untrusted_peer_changes_nothing() -> TestResult {
    let forwarding = trusting(&["10.0.0.0/8"], ForwardedHeader::Forwarded)?;
    let named = client(
        forwarding,
        "203.0.113.50",
        &[
            ("forwarded", "for=10.0.0.1"),
            ("forwarded", "for=192.0.2.60"),
        ],
    )
    .await?;
    assert_eq!("203.0.113.50", named);
    Ok(())
}

#[tokio::test]
async fn the_rightmost_untrusted_hop_names_the_client_past_a_spoofed_one() -> TestResult {
    let forwarding = trusting(&["10.0.0.0/24"], ForwardedHeader::XForwardedFor)?;
    // The client wrote 192.0.2.66 itself; the edge proxy appended the address
    // it saw, 203.0.113.9, and the inner proxy appended the edge, 10.0.0.2.
    let named = client(
        forwarding,
        "10.0.0.1",
        &[("x-forwarded-for", "192.0.2.66, 203.0.113.9, 10.0.0.2")],
    )
    .await?;
    assert_eq!("203.0.113.9", named);
    Ok(())
}

#[tokio::test]
async fn hops_across_several_header_lines_are_read_in_order() -> TestResult {
    let forwarding = trusting(&["10.0.0.0/24"], ForwardedHeader::Forwarded)?;
    let named = client(
        forwarding,
        "10.0.0.1",
        &[
            ("forwarded", "for=192.0.2.66"),
            ("forwarded", "for=\"[2001:db8::17]:4711\", for=10.0.0.2"),
        ],
    )
    .await?;
    assert_eq!("2001:db8::17", named);
    Ok(())
}

#[tokio::test]
async fn an_unknown_hop_stops_at_the_last_address_a_proxy_gave() -> TestResult {
    let forwarding = trusting(&["10.0.0.0/24"], ForwardedHeader::Forwarded)?;
    let named = client(
        forwarding,
        "10.0.0.1",
        &[("forwarded", "for=192.0.2.66, for=unknown, for=10.0.0.2")],
    )
    .await?;
    assert_eq!("10.0.0.2", named);
    Ok(())
}

#[tokio::test]
async fn only_the_configured_header_is_read() -> TestResult {
    let forwarding = trusting(&["127.0.0.1/32"], ForwardedHeader::XForwardedFor)?;
    let named = client(forwarding, "127.0.0.1", &[("forwarded", "for=192.0.2.60")]).await?;
    assert_eq!("127.0.0.1", named);
    Ok(())
}

#[tokio::test]
async fn a_request_with_no_connection_is_named_by_nothing() -> TestResult {
    let app = Router::new()
        .route("/", get(named))
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(Forwarding::default()),
            client_address::attach,
        ));
    let (_, text) = call(app, Request::get("/").body(Body::empty())?).await?;
    assert_eq!("none", text);
    Ok(())
}

/// The configuration `text` resolves to.
fn resolve(text: &str) -> Result<Result<Forwarding, ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .map(|settings| settings.server.forwarding))
}

#[test]
fn the_default_trusts_no_proxy() -> TestResult {
    let forwarding = resolve("")?.map_err(|error| error.to_string())?;
    assert!(forwarding.is_empty());
    assert_eq!(Forwarding::default(), forwarding);
    Ok(())
}

#[test]
fn a_trusted_proxy_is_an_address_or_a_cidr_block() -> TestResult {
    let forwarding = resolve(
        "[server]\ntrusted_proxies = [\"127.0.0.1\", \"10.20.0.0/16\", \"::1\"]\nforwarded_header = \"x-forwarded-for\"\n",
    )?
    .map_err(|error| error.to_string())?;
    for trusted in ["127.0.0.1", "10.20.3.4", "::1", "::ffff:10.20.0.9"] {
        assert!(forwarding.trusts(trusted.parse::<IpAddr>()?), "{trusted}");
    }
    for untrusted in ["127.0.0.2", "10.21.0.1", "::2"] {
        assert!(
            !forwarding.trusts(untrusted.parse::<IpAddr>()?),
            "{untrusted}"
        );
    }
    Ok(())
}

#[test]
fn an_entry_that_is_no_address_is_refused_by_its_key() -> TestResult {
    let refused = resolve("[server]\ntrusted_proxies = [\"127.0.0.1\", \"proxy.example.org\"]\n")?
        .err()
        .ok_or("a host name is refused")?;
    assert!(
        refused.to_string().contains("server.trusted_proxies[1]"),
        "{refused}"
    );
    let unknown = Config::from_sources(
        Some("[server]\nforwarded_header = \"x-real-ip\"\n"),
        &BTreeMap::new(),
    );
    assert!(unknown.is_err(), "a header other than the two is refused");
    Ok(())
}
