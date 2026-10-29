// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The listeners and the TLS each may serve.
//!
//! `[server]` is the HTTP surface, the section of `ferrohealth-server` with
//! the gateway's [`defaults`], `[metrics]` the admin listener of the metrics
//! surface, and `[server.tls]` and `[metrics.tls]` their TLS. No
//! specification governs the configuration: our own design.

use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;

use ferrohealth_server::section::{ClientAuth, ServerSection, TlsSection, TlsVersion};
use ferrohealth_server::tls::{HealthcheckTls, TlsKeys, server_config_of};
use serde::Deserialize;

use crate::base_path::BasePath;
use crate::config::error::Error;
use crate::config::error::server::ServerFault;

/// The gateway's defaults of the shared `[server]` table, which
/// [`load::loader`](crate::config::load::loader) layers under every source
/// and [`Config::default`](crate::config::Config) holds: at most 512
/// requests served at once, and the rest as `ferrohealth-server` sets it.
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
    /// The `[[auth.basic.users]]` a scrape of `GET /metrics` may
    /// authenticate as, with HTTP Basic (RFC 7617); empty, the scrape is
    /// open. Outside the development profile a `listen` address that is not
    /// a loopback one needs a user here, or `[metrics.tls] client_ca_file`.
    pub scrape_users: Vec<String>,
    /// The TLS the admin listener serves (`[metrics.tls]`); unset, it serves
    /// plain HTTP.
    pub tls: Option<ListenerTls>,
}

/// The TLS a listener serves: its certificate and key, and the client
/// certificates it asks for.
///
/// Every file is read at start and again on `SIGHUP`, every
/// `reload_interval_s` and when a configuration revision is applied, so a
/// renewed certificate takes effect without a restart; a changed path takes
/// a restart. The listener speaks TLS 1.3 alone unless `min_version`
/// lowers it to TLS 1.2, with the suites of RFC 9325 §4.2, and nothing older
/// (RFC 8446; BCP 195, RFC 9325 §3.1.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ListenerTls {
    /// The PEM file holding the certificate chain the listener presents,
    /// its own certificate first.
    pub certificate_file: Option<PathBuf>,
    /// The PEM file holding the private key of that certificate.
    pub key_file: Option<PathBuf>,
    /// Whether the listener asks a client certificate: `none`, the default,
    /// `optional` or `required`.
    pub client_auth: ClientAuth,
    /// The PEM file of the CA certificates a client certificate must chain
    /// to: required unless `client_auth` is `none`, and refused when it is.
    pub client_ca_file: Option<PathBuf>,
    /// How often, in seconds, the files are read again; unset, only
    /// `SIGHUP` and an applied revision read them again.
    pub reload_interval_s: Option<NonZeroU64>,
    /// The oldest TLS version the listener speaks: `"1.3"`, the default, or
    /// `"1.2"` to keep a TLS 1.2 client.
    pub min_version: TlsVersion,
    /// The PEM file holding the client certificate chain and key `ferrofed
    /// healthcheck` presents to a listener that requires one; `[server.tls]`
    /// only.
    pub healthcheck_identity_file: Option<PathBuf>,
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

impl ListenerTls {
    /// Resolves the TLS of the listener whose table is `table`, reading
    /// every file once, as `serve` and a reload read them.
    ///
    /// # Errors
    ///
    /// [`Error::Missing`] for an unset `certificate_file` or `key_file`,
    /// [`ServerFault::HealthcheckIdentityUnused`] for a healthcheck identity outside
    /// `[server.tls]` or beside no client certificate asked for, and
    /// [`Error::ListenerTls`] for a file that does not read, a key that does
    /// not match its certificate, or a `client_auth` its `client_ca_file`
    /// disagrees with.
    pub(crate) fn resolve(&self, table: &'static str) -> Result<ListenerTlsSettings, Error> {
        let required = |field: &str, value: &Option<PathBuf>| {
            value.clone().ok_or_else(|| Error::Missing {
                key: format!("{table}.{field}"),
            })
        };
        let section = TlsSection {
            certificate_file: required("certificate_file", &self.certificate_file)?,
            key_file: required("key_file", &self.key_file)?,
            client_auth: self.client_auth,
            client_ca_file: self.client_ca_file.clone(),
            reload_interval_s: self.reload_interval_s,
            healthcheck_identity_file: self.healthcheck_identity_file.clone(),
            min_version: self.min_version,
        };
        resolve_section(section, table)
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

/// Resolves `tls`, the TLS of the listener whose table is `table`, when it
/// is set.
///
/// # Errors
///
/// The errors of [`ListenerTls::resolve`].
pub(crate) fn resolve_tls(
    tls: Option<&ListenerTls>,
    table: &'static str,
) -> Result<Option<ListenerTlsSettings>, Error> {
    tls.map(|tls| tls.resolve(table)).transpose()
}

/// The table of the client listener's TLS.
pub const SERVER_TLS: &str = "server.tls";

/// The table of the admin listener's TLS.
pub const METRICS_TLS: &str = "metrics.tls";
