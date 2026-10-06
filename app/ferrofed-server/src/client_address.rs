// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The address a request came from, as an access record names it.
//!
//! The connection names its peer. Behind a reverse proxy that peer is the
//! proxy, and the client is named in a header the proxy appends to:
//! `Forwarded` (RFC 7239 §4) or `X-Forwarded-For`. A header is free text any
//! client can send, so it is read only from a peer in `server.trusted_proxies`
//! (RFC 7239 §8.1), and only the hops those proxies appended are believed:
//! read from the right, the first hop that is no trusted proxy is the client.
//! A hop that names no address, such as `unknown` or an obfuscated
//! identifier (RFC 7239 §6), ends the walk at the last address a trusted
//! proxy vouched for. Nothing here keys a rate limit: the caller limit keys on
//! the verified caller. No specification governs the rest: our own design.

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::rejection::ExtensionRejection;
use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::HeaderMap;
use http::header::{FORWARDED, HeaderName};
use ipnet::IpNet;
use serde::Deserialize;

use crate::listener::Peer;

/// The `X-Forwarded-For` header, which no RFC registers.
pub const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// The header a trusted proxy names the client in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForwardedHeader {
    /// `Forwarded`, its `for=` parameters (RFC 7239 §4, §5.2).
    #[default]
    Forwarded,
    /// `X-Forwarded-For`, a comma-separated list of addresses.
    XForwardedFor,
}

/// The reverse proxies a forwarded client address is taken from, and the
/// header they name it in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Forwarding {
    proxies: Vec<IpNet>,
    header: ForwardedHeader,
}

impl Forwarding {
    /// The forwarding of `proxies`, each an address or a CIDR block, naming
    /// the client in `header`.
    #[must_use]
    pub fn new(proxies: Vec<IpNet>, header: ForwardedHeader) -> Self {
        Self { proxies, header }
    }

    /// Whether no proxy is trusted, so every request is named by its peer.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.proxies.is_empty()
    }

    /// Whether `address` is a trusted proxy.
    #[must_use]
    pub fn trusts(&self, address: IpAddr) -> bool {
        let address = address.to_canonical();
        self.proxies.iter().any(|proxy| proxy.contains(&address))
    }

    /// The address a request from `peer` with `headers` came from: `peer`
    /// unless it is a trusted proxy, and otherwise the rightmost hop of the
    /// configured header that is no trusted proxy.
    #[must_use]
    pub fn client(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        let mut client = peer.to_canonical();
        if !self.trusts(client) {
            return client;
        }
        for hop in self.hops(headers).iter().rev() {
            let Some(address) = hop else {
                break;
            };
            client = *address;
            if !self.trusts(client) {
                break;
            }
        }
        client
    }

    /// The hops the configured header names, leftmost first; `None` for a
    /// hop that names no address.
    fn hops(&self, headers: &HeaderMap) -> Vec<Option<IpAddr>> {
        let name = match self.header {
            ForwardedHeader::Forwarded => FORWARDED,
            ForwardedHeader::XForwardedFor => X_FORWARDED_FOR,
        };
        let mut hops = Vec::new();
        for value in headers.get_all(name) {
            // NOTE: RFC 9110 §5.5, a header value that is not visible ASCII
            // names no address a proxy wrote, so the hop is unknown.
            match value.to_str() {
                Ok(text) => hops.extend(self.header.hops(text)),
                Err(_) => hops.push(None),
            }
        }
        hops
    }
}

impl ForwardedHeader {
    /// The hops one value of this header names, leftmost first.
    fn hops(self, text: &str) -> Vec<Option<IpAddr>> {
        match self {
            Self::Forwarded => forwarded_hops(text),
            Self::XForwardedFor => listed_hops(text),
        }
    }
}

/// The address a request came from, set on every request the listener
/// accepted over a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientAddress(pub IpAddr);

/// Names the address `request` came from ([`ClientAddress`]) under
/// `forwarding`, from the connection's peer, before any other layer reads
/// it; a request with no connection, as in a test, gets none.
pub async fn attach(
    State(forwarding): State<Arc<Forwarding>>,
    peer: Result<ConnectInfo<Peer>, ExtensionRejection>,
    mut request: Request,
    next: Next,
) -> Response {
    // NOTE: no specification governs this: our own design; a request served
    // over no connection has no peer, so it is named by no address.
    let peer = peer.ok().map(|ConnectInfo(Peer(peer))| peer.ip());
    if let Some(peer) = peer {
        let client = forwarding.client(peer, request.headers());
        request.extensions_mut().insert(ClientAddress(client));
    }
    next.run(request).await
}

/// The `for=` hops of one `Forwarded` value (RFC 7239 §4): one per element,
/// in order; an element without `for=` names no address.
fn forwarded_hops(text: &str) -> Vec<Option<IpAddr>> {
    text.split(',')
        .map(|element| {
            element
                .split(';')
                .find_map(|pair| {
                    let (name, value) = pair.split_once('=')?;
                    name.trim()
                        .eq_ignore_ascii_case("for")
                        .then(|| node(value.trim()))
                })
                .flatten()
        })
        .collect()
}

/// The hops of one `X-Forwarded-For` value, in order.
fn listed_hops(text: &str) -> Vec<Option<IpAddr>> {
    text.split(',').map(|hop| node(hop.trim())).collect()
}

/// The address an RFC 7239 §6 node names, its port dropped: an IPv4
/// address, a bracketed IPv6 address, either quoted, or a bare IPv6 address
/// as `X-Forwarded-For` writes one; `None` for `unknown`, an obfuscated
/// identifier or anything else.
fn node(text: &str) -> Option<IpAddr> {
    let text = text
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(text);
    if let Ok(address) = text.parse::<IpAddr>() {
        return Some(address.to_canonical());
    }
    if let Some(bracketed) = text.strip_prefix('[') {
        let (address, _port) = bracketed.split_once(']')?;
        return address
            .parse::<IpAddr>()
            .ok()
            .map(|address| address.to_canonical());
    }
    // NOTE: RFC 7239 §6, an IPv4 node may carry a port after one colon.
    let (address, _port) = text.split_once(':')?;
    address
        .parse::<std::net::Ipv4Addr>()
        .ok()
        .map(|address| IpAddr::V4(address).to_canonical())
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::{forwarded_hops, listed_hops, node};

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("an address")
    }

    #[test]
    fn a_node_names_an_address_with_or_without_its_port() {
        assert_eq!(Some(ip("192.0.2.43")), node("192.0.2.43"));
        assert_eq!(Some(ip("192.0.2.43")), node("192.0.2.43:47011"));
        assert_eq!(
            Some(ip("2001:db8:cafe::17")),
            node("\"[2001:db8:cafe::17]:4711\"")
        );
        assert_eq!(Some(ip("2001:db8:cafe::17")), node("2001:db8:cafe::17"));
        assert_eq!(Some(ip("192.0.2.1")), node("::ffff:192.0.2.1"));
        assert_eq!(None, node("unknown"));
        assert_eq!(None, node("_hidden"));
        assert_eq!(None, node(""));
    }

    #[test]
    fn the_for_parameter_of_each_element_is_a_hop() {
        assert_eq!(
            vec![Some(ip("192.0.2.60")), Some(ip("198.51.100.17")), None],
            forwarded_hops(
                "for=192.0.2.60;proto=http;by=203.0.113.43, For=\"198.51.100.17\", by=10.0.0.1"
            )
        );
        assert_eq!(
            vec![Some(ip("192.0.2.60")), None],
            listed_hops("192.0.2.60, unknown")
        );
    }
}
