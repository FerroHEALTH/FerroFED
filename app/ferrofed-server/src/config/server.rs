// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The listeners and the TLS each may serve.
//!
//! `[server]` is the HTTP surface, the section of `ferrohealth-server` with
//! the gateway's [`defaults`], `[metrics]` the admin listener of the metrics
//! surface, and `[server.tls]` and `[metrics.tls]` their TLS. No
//! specification governs the configuration: our own design.

use std::num::NonZeroUsize;

use ferrohealth_server::section::{ClientAuth, ServerSection, TlsSection};
use ferrohealth_server::tls::{HealthcheckTls, TlsKeys, server_config_of};
use serde::Deserialize;

use crate::config::base_path::BasePath;
use crate::config::error::Error;
use crate::config::error::server::ServerFault;

/// The gateway's defaults of the shared `[server]` table.
///
/// [`load::loader`](crate::config::load::loader) layers them under every
/// source and [`Config::default`](crate::config::Config) holds them: at most
/// 512 requests served at once, and the rest as `ferrohealth-server` sets it.
#[must_use]
pub fn defaults() -> ServerSection {
    ServerSection {
        max_concurrent_requests: NonZeroUsize::new(512),
        ..ServerSection::default()
    }
}

/// Resolves `server.base_path` as the gateway's [`BasePath`]: empty, the
/// default, is `/`.
///
/// # Errors
///
/// [`ServerFault::BasePath`] for a base path no request path can sit under.
pub fn resolve_base_path(section: &ServerSection) -> Result<BasePath, Error> {
    let written = section.base_path.as_str();
    let path = if written.is_empty() { "/" } else { written };
    path.parse::<BasePath>().map_err(|source| {
        Error::Server(ServerFault::BasePath {
            key: String::from("server.base_path"),
            source,
        })
    })
}

/// The admin listener that serves the Prometheus text exposition of the
/// metrics; `[telemetry.otlp]` pushes the same metrics to a collector.
///
/// It is off by default. No specification governs metrics: our own design.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Metrics {
    /// The socket address the admin listener binds to serve `GET /metrics`.
    /// Unset, no listener runs; it never shares `server.listen`.
    pub listen: Option<String>,
    /// Whether `listen` may name an address other than a loopback one. A
    /// remote address is refused unless this is set.
    pub allow_remote: bool,
    /// The clients a scrape of `GET /metrics` may authenticate as, each with
    /// a client-credentials token of an `[[auth.issuer]]` (RFC 6749 §4.4);
    /// empty, the scrape is open. Outside the development profile a `listen`
    /// address that is not a loopback one needs a client here, or
    /// `[metrics.tls] client_ca_file`.
    pub scrape_clients: Vec<String>,
    /// The TLS the admin listener serves (`[metrics.tls]`), the table
    /// `ferrohealth-server` reads; unset, it serves plain HTTP.
    pub tls: Option<TlsSection>,
}

/// The TLS of one listener, resolved: the table `ferrohealth-server` serves
/// and the keys its faults are named by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerTlsSettings {
    /// The table.
    pub section: TlsSection,
    /// The keys of the table it came from, `server.tls` or `metrics.tls`.
    pub keys: TlsKeys,
}

impl ListenerTlsSettings {
    /// Whether the listener asks every client for a certificate.
    #[must_use]
    pub fn requires_client_certificate(&self) -> bool {
        self.section.client_auth == ClientAuth::Required
    }
}

/// Resolves `section`, the TLS of the listener whose table is `table`,
/// reading every file once, as `serve` and a reload read them.
///
/// # Errors
///
/// [`ServerFault::HealthcheckIdentityUnused`] for a healthcheck identity
/// outside `[server.tls]` or beside no client certificate asked for, and
/// [`Error::ListenerTls`] for a file that does not read, a key that does not
/// match its certificate, or a `client_auth` its `client_ca_file` disagrees
/// with.
pub(crate) fn resolve_section(
    section: TlsSection,
    table: &'static str,
) -> Result<ListenerTlsSettings, Error> {
    // NOTE: no specification governs this: our own design; the healthcheck asks
    // the client listener alone, and presents a certificate only where one is asked.
    if section.healthcheck_identity_file.is_some()
        && (table != SERVER_TLS || section.client_auth == ClientAuth::None)
    {
        return Err(Error::Server(ServerFault::HealthcheckIdentityUnused {
            key: format!("{table}.healthcheck_identity_file"),
        }));
    }
    let keys = if table == SERVER_TLS {
        TlsKeys::SERVER
    } else {
        ferrohealth_server::tls_keys!("metrics.tls")
    };
    let served = |source| Error::ListenerTls { table, source };
    server_config_of(&section, keys).map_err(served)?;
    HealthcheckTls::read_of(&section, keys).map_err(served)?;
    Ok(ListenerTlsSettings { section, keys })
}

/// Resolves `[server.tls]`, when it is set.
///
/// # Errors
///
/// The errors of [`resolve_section`].
pub(crate) fn resolve_server_tls(
    section: &ServerSection,
) -> Result<Option<ListenerTlsSettings>, Error> {
    section
        .tls
        .clone()
        .map(|tls| resolve_section(tls, SERVER_TLS))
        .transpose()
}

/// The smallest response body the client listener compresses, when
/// `server.compression` is on.
#[must_use]
pub fn compression(section: &ServerSection) -> Option<u64> {
    section
        .compression
        .then_some(section.compression_min_size.bytes())
}

/// Resolves `tls`, the TLS of the listener whose table is `table`, when it
/// is set.
///
/// # Errors
///
/// The errors of [`resolve_section`].
pub(crate) fn resolve_tls(
    tls: Option<&TlsSection>,
    table: &'static str,
) -> Result<Option<ListenerTlsSettings>, Error> {
    tls.map(|tls| resolve_section(tls.clone(), table))
        .transpose()
}

/// The table of the client listener's TLS.
pub const SERVER_TLS: &str = "server.tls";

/// The table of the admin listener's TLS.
pub const METRICS_TLS: &str = "metrics.tls";
