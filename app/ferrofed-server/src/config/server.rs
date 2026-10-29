// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The listeners and the TLS each may serve.
//!
//! `[server]` is the HTTP surface, `[metrics]` the admin listener of the
//! metrics surface, and `[server.tls]` and `[metrics.tls]` their TLS. No
//! specification governs the configuration: our own design.

use std::net::IpAddr;
use std::num::NonZeroU64;
use std::path::PathBuf;

use ferrofed_registry::replica::Replica;
use ferrohealth_server::section::{ClientAuth, TlsSection, TlsVersion};
use ferrohealth_server::tls::{HealthcheckTls, TlsKeys, server_config_of};
use ipnet::IpNet;
use serde::Deserialize;

use crate::base_path::BasePath;
use crate::client_address::{ForwardedHeader, Forwarding};
use crate::config::error::Error;
use crate::config::error::server::ServerFault;
use crate::config::limits;
use crate::config::public_url::PublicUrl;

/// The HTTP surface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address to bind.
    pub listen: String,
    /// The path of the deployment's base URL, `{base}`, which every route
    /// sits under: `/`, the default, or a path such as `/fed/openehr` with no
    /// trailing `/`, query or fragment (§4.1, N28). The specification
    /// reserves no prefix.
    pub base_path: String,
    /// The absolute URL clients reach `{base}` at, such as
    /// `https://gateway.example.org/fed`, whose path is `base_path`. Set,
    /// `auth.audience`, `signing.jwks_uri` and `pmir.callback_url` default
    /// from it, and a `signing.jwks_uri` or `pmir.callback_url` that names
    /// another route than the gateway serves under it is refused
    /// ([`public_url`](crate::config::public_url)). Unset, each is written
    /// out.
    pub public_url: Option<String>,
    /// The reverse proxies whose forwarded client address the gateway
    /// takes, each an IP address or a CIDR block. Empty, the default, the
    /// address a request came from is its peer's, whatever it forwards.
    pub trusted_proxies: Vec<String>,
    /// The header a trusted proxy names the client in: `forwarded`, the
    /// default (RFC 7239), or `x-forwarded-for`.
    pub forwarded_header: ForwardedHeader,
    /// How long one request may take before the server answers `408`. With a
    /// registry configured, it must exceed `federation.overall_timeout_ms`
    /// by more than [`COMBINING_MARGIN_MS`](crate::config::COMBINING_MARGIN_MS).
    pub request_timeout_ms: u64,
    /// How long a client may take to send a request head once it connected
    /// or once its last response was written; a connection still sending one
    /// then is closed. Zero is refused.
    pub header_read_timeout_ms: u64,
    /// How long the server keeps accepting connections after the stop signal,
    /// with readiness already `503`, so a load balancer stops routing to the
    /// process before its listener closes. `0`, the default, closes it at once.
    pub drain_delay_ms: u64,
    /// How long the drain may take once the listener has closed. Unset, it is
    /// `request_timeout_ms`; set, it must be at least that, so the drain
    /// outlasts every request accepted before the listener closed.
    pub shutdown_timeout_ms: Option<u64>,
    /// How long the bindings may take to stop their processes once the drain
    /// has ended, such as deleting the PMIR subscription; a process still
    /// running when it elapses is abandoned with a warning. Zero is refused.
    pub bindings_drain_timeout_ms: u64,
    /// The largest request body the server reads before answering `413`.
    pub body_limit_bytes: usize,
    /// The most requests the server serves at once. One more is answered `503`
    /// with `Retry-After` and reaches nothing behind the listener; the health
    /// family is never refused. Zero is refused.
    pub max_concurrent_requests: u32,
    /// The seconds a `503` past `max_concurrent_requests` asks the client to
    /// wait in `Retry-After`; zero is refused.
    pub overload_retry_after_s: u32,
    /// The per-caller rate limit (`[server.caller_rate]`), keyed on the caller
    /// client authentication verified; unset, no caller is rate limited.
    pub caller_rate: Option<limits::CallerRate>,
    /// The TLS the listener serves (`[server.tls]`); unset, it serves plain
    /// HTTP, for a proxy that terminates TLS in front of it.
    pub tls: Option<ListenerTls>,
    /// The name of this process, which every report of the operator surface
    /// and the export of the identity state carry: 1 to 64 ASCII letters,
    /// digits, `.`, `_` and `-`. Unset, it is the `HOSTNAME` the process
    /// runs with, which Kubernetes sets to the pod's name, and `unnamed`
    /// without one ([`Server::resolve_replica`]).
    pub replica: Option<String>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:8080"),
            base_path: String::from("/"),
            public_url: None,
            trusted_proxies: Vec::new(),
            forwarded_header: ForwardedHeader::default(),
            request_timeout_ms: 30_000,
            header_read_timeout_ms: 30_000,
            drain_delay_ms: 0,
            shutdown_timeout_ms: None,
            bindings_drain_timeout_ms: 5_000,
            body_limit_bytes: 1024 * 1024,
            max_concurrent_requests: 512,
            overload_retry_after_s: 1,
            caller_rate: None,
            tls: None,
            replica: None,
        }
    }
}

impl Server {
    /// Resolves `server.base_path` and `server.public_url`, whose path must be
    /// that base path.
    ///
    /// # Errors
    ///
    /// [`ServerFault::BasePath`] for a base path no request path can sit under, and
    /// the errors of [`PublicUrl::resolve`].
    pub fn resolve_base(&self) -> Result<(BasePath, Option<PublicUrl>), Error> {
        let base_path = self.base_path.parse::<BasePath>().map_err(|source| {
            Error::Server(ServerFault::BasePath {
                key: String::from("server.base_path"),
                source,
            })
        })?;
        let public_url = self
            .public_url
            .as_deref()
            .map(|text| PublicUrl::resolve(text, &base_path))
            .transpose()?;
        Ok((base_path, public_url))
    }

    /// Resolves `server.replica`, the name of this process, with the
    /// [`HOST_NAME_ENV`] the process runs with as the name when none is
    /// configured ([`Server::replica_or_host`]).
    ///
    /// # Errors
    ///
    /// [`ServerFault::Replica`] for a configured name that is no replica name.
    pub fn resolve_replica(&self) -> Result<Replica, Error> {
        self.replica_or_host(std::env::var(HOST_NAME_ENV).ok().as_deref())
    }

    /// Resolves `server.replica`, with `host_name` as the name when none is
    /// configured.
    ///
    /// A host name that is no replica name, or none, names the process
    /// [`UNNAMED`](ferrofed_registry::replica::UNNAMED): the platform's
    /// host name is never a reason to refuse a start.
    ///
    /// # Errors
    ///
    /// [`ServerFault::Replica`] for a configured name that is no replica name.
    pub fn replica_or_host(&self, host_name: Option<&str>) -> Result<Replica, Error> {
        match &self.replica {
            Some(configured) => configured
                .parse()
                .map_err(|source| Error::Server(ServerFault::Replica(source))),
            // NOTE: no specification governs this: our own design; a host name outside the
            // form is legitimately absent as a name, so the process is unnamed.
            None => Ok(host_name
                .and_then(|host| host.parse().ok())
                .unwrap_or_default()),
        }
    }

    /// Resolves `server.header_read_timeout_ms`.
    ///
    /// # Errors
    ///
    /// [`Error::Zero`] for a bound of zero.
    pub fn resolve_header_read_timeout(&self) -> Result<NonZeroU64, Error> {
        NonZeroU64::new(self.header_read_timeout_ms).ok_or_else(|| Error::Zero {
            key: String::from("server.header_read_timeout_ms"),
        })
    }

    /// Resolves `server.trusted_proxies` and `server.forwarded_header`: each
    /// proxy an IP address, read as a block of one, or a CIDR block.
    ///
    /// # Errors
    ///
    /// [`ServerFault::TrustedProxy`] naming the entry that is neither.
    pub fn resolve_forwarding(&self) -> Result<Forwarding, Error> {
        let proxies = self
            .trusted_proxies
            .iter()
            .enumerate()
            .map(|(index, written)| {
                let text = written.trim();
                text.parse::<IpNet>()
                    .or_else(|_| text.parse::<IpAddr>().map(IpNet::from))
                    .map_err(|source| {
                        Error::Server(ServerFault::TrustedProxy {
                            key: format!("server.trusted_proxies[{index}]"),
                            source,
                        })
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Forwarding::new(proxies, self.forwarded_header))
    }
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

/// The environment variable a process's host name is read from when
/// `server.replica` names none; Kubernetes sets it to the pod's name.
pub const HOST_NAME_ENV: &str = "HOSTNAME";

/// The table of the admin listener's TLS.
pub const METRICS_TLS: &str = "metrics.tls";
