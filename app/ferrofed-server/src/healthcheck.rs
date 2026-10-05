// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `healthcheck` job: asks the gateway running beside it whether it is
//! ready, for a container runtime that has no HTTP client of its own.
//!
//! The job connects to the configured listen port on loopback, sends
//! `GET {base}/health/readiness` with a short timeout, and reports the outcome as
//! one line, never the body. It exits `0` only on `200`, which is what a
//! Docker `HEALTHCHECK` reads as healthy
//! (<https://docs.docker.com/reference/dockerfile/#healthcheck>). No
//! specification governs health probes: our own design.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use http::StatusCode;

/// The path the job asks, under the configured base path.
pub const READINESS: &str = "/health/readiness";

/// How long the job waits for the whole exchange.
///
/// Shorter than the five seconds the image's `HEALTHCHECK` allows, so the
/// job reports its own line before the runtime gives up on it.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// What the gateway answered.
#[derive(Debug)]
pub enum Outcome {
    /// Readiness answered `200`.
    Ready,
    /// Readiness answered with this other status.
    NotReady(StatusCode),
    /// No connection could be made: nothing listens on the port.
    Refused,
    /// The gateway did not answer within the timeout.
    TimedOut(Duration),
    /// The request failed in some other way.
    Failed(reqwest::Error),
}

impl Outcome {
    /// Returns whether the gateway is ready.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready => f.write_str("ready"),
            Self::NotReady(status) => write!(f, "not ready: readiness answered {status}"),
            Self::Refused => f.write_str("not ready: no connection could be made"),
            Self::TimedOut(timeout) => {
                write!(f, "not ready: no answer within {} ms", timeout.as_millis())
            }
            Self::Failed(error) => {
                write!(f, "not ready: the request failed: {}", crate::chain(error))
            }
        }
    }
}

/// Returns the address the job connects to for a gateway listening on
/// `listen`.
///
/// A wildcard address is not one a client can connect to, so it becomes the
/// loopback address of its family; any other address is where the gateway
/// accepts, so it stays.
#[must_use]
pub fn target(listen: SocketAddr) -> SocketAddr {
    let ip = match listen.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    SocketAddr::new(ip, listen.port())
}

/// Asks the gateway at `address` for its readiness at `path`, waiting at
/// most `timeout`.
///
/// No proxy is consulted and no redirect is followed: the gateway is on
/// this host, and only its own answer counts.
pub async fn check(address: SocketAddr, path: &str, timeout: Duration) -> Outcome {
    let client = match reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(error) => return Outcome::Failed(error),
    };
    match client.get(format!("http://{address}{path}")).send().await {
        Ok(response) if response.status() == StatusCode::OK => Outcome::Ready,
        Ok(response) => Outcome::NotReady(response.status()),
        Err(error) if error.is_timeout() => Outcome::TimedOut(timeout),
        Err(error) if error.is_connect() => Outcome::Refused,
        Err(error) => Outcome::Failed(error),
    }
}

#[cfg(test)]
mod tests {
    use super::target;
    use std::net::SocketAddr;

    #[test]
    fn a_wildcard_listen_address_is_asked_on_loopback_and_any_other_as_it_is() {
        for (listen, asked) in [
            ("0.0.0.0:8080", "127.0.0.1:8080"),
            ("[::]:8080", "[::1]:8080"),
            ("127.0.0.1:9000", "127.0.0.1:9000"),
            ("[::1]:9000", "[::1]:9000"),
            ("10.0.0.7:8080", "10.0.0.7:8080"),
        ] {
            let listen: SocketAddr = listen.parse().expect("a socket address");
            let asked: SocketAddr = asked.parse().expect("a socket address");
            assert_eq!(asked, target(listen), "{listen}");
        }
    }
}
