// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The listeners and the TLS each may serve.
//!
//! `[server]` is the HTTP surface, `[metrics]` the admin listener of the
//! metrics surface, and `[server.tls]` and `[metrics.tls]` their TLS. No
//! specification governs the configuration: our own design.

use std::net::IpAddr;
use std::path::PathBuf;

use ferrofed_registry::secret::{Secret, SecretUrl};
use ipnet::IpNet;
use serde::Deserialize;

use crate::base_path::BasePath;
use crate::client_address::{ForwardedHeader, Forwarding};
use crate::config::error::Error;
use crate::config::limits;
use crate::config::public_url::PublicUrl;
use crate::listener::certificates::{Certificates, TlsFiles};

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
            drain_delay_ms: 0,
            shutdown_timeout_ms: None,
            bindings_drain_timeout_ms: 5_000,
            body_limit_bytes: 1024 * 1024,
            max_concurrent_requests: 512,
            overload_retry_after_s: 1,
            caller_rate: None,
            tls: None,
        }
    }
}

impl Server {
    /// Resolves `server.base_path` and `server.public_url`, whose path must be
    /// that base path.
    ///
    /// # Errors
    ///
    /// [`Error::BasePath`] for a base path no request path can sit under, and
    /// the errors of [`PublicUrl::resolve`].
    pub fn resolve_base(&self) -> Result<(BasePath, Option<PublicUrl>), Error> {
        let base_path = self
            .base_path
            .parse::<BasePath>()
            .map_err(|source| Error::BasePath {
                key: String::from("server.base_path"),
                source,
            })?;
        let public_url = self
            .public_url
            .as_deref()
            .map(|text| PublicUrl::resolve(text, &base_path))
            .transpose()?;
        Ok((base_path, public_url))
    }

    /// Resolves `server.trusted_proxies` and `server.forwarded_header`: each
    /// proxy an IP address, read as a block of one, or a CIDR block.
    ///
    /// # Errors
    ///
    /// [`Error::TrustedProxy`] naming the entry that is neither.
    pub fn resolve_forwarding(&self) -> Result<Forwarding, Error> {
        let proxies = self
            .trusted_proxies
            .iter()
            .enumerate()
            .map(|(index, written)| {
                let text = written.trim();
                text.parse::<IpNet>()
                    .or_else(|_| text.parse::<IpAddr>().map(IpNet::from))
                    .map_err(|source| Error::TrustedProxy {
                        key: format!("server.trusted_proxies[{index}]"),
                        source,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Forwarding::new(proxies, self.forwarded_header))
    }
}

/// The metrics surface: one meter provider read by the Prometheus text
/// exposition on its own listener and, when set, pushed over OTLP.
///
/// Both are off by default. No specification governs metrics: our own design.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Metrics {
    /// The socket address the admin listener binds to serve `GET /metrics`.
    /// Unset, no listener runs; it never shares `server.listen`.
    pub listen: Option<String>,
    /// Whether `listen` may name an address other than a loopback one. A
    /// remote address is refused unless this is set.
    pub allow_remote: bool,
    /// The bearer token a scrape of `GET /metrics` must carry; unset, the
    /// scrape is open. Outside the development profile a `listen` address
    /// that is not a loopback one needs it, or `[metrics.tls]
    /// client_ca_file`.
    pub scrape_token: Option<Secret>,
    /// A file holding the scrape token, read at boot.
    pub scrape_token_file: Option<PathBuf>,
    /// The `http://` URL of an OTLP collector the metrics are pushed to over
    /// gRPC. Unset, nothing is pushed.
    pub otlp_endpoint: Option<SecretUrl>,
    /// The TLS the admin listener serves (`[metrics.tls]`); unset, it serves
    /// plain HTTP.
    pub tls: Option<ListenerTls>,
}

/// The TLS a listener serves: its certificate and key, and the CA a client
/// certificate must chain to when one is required.
///
/// Every file is read at start and again on `SIGHUP`, so a renewed
/// certificate takes effect without a restart; a changed path takes a
/// restart. The listener negotiates TLS 1.3 or TLS 1.2 and nothing older
/// (RFC 8446; BCP 195, RFC 9325 §3.1.1).
#[expect(
    clippy::struct_field_names,
    reason = "each key names a file, as every `_file` key of the configuration does"
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ListenerTls {
    /// The PEM file holding the certificate chain the listener presents,
    /// its own certificate first.
    pub certificate_file: Option<PathBuf>,
    /// The PEM file holding the private key of that certificate.
    pub key_file: Option<PathBuf>,
    /// The PEM file of the CA certificates a client certificate must chain
    /// to. Set, the listener refuses a client that presents none; unset, it
    /// asks for none.
    pub client_ca_file: Option<PathBuf>,
    /// The PEM file holding the client certificate chain and key `ferrofed
    /// healthcheck` presents to a listener that requires one; `[server.tls]`
    /// only.
    pub healthcheck_identity_file: Option<PathBuf>,
}

impl ListenerTls {
    /// Resolves the TLS of the listener whose table is `table`, reading
    /// every file once, as `serve` and a reload read them.
    ///
    /// # Errors
    ///
    /// [`Error::Missing`] for an unset `certificate_file` or `key_file`,
    /// [`Error::HealthcheckIdentityUnused`] for a healthcheck identity outside
    /// `[server.tls]` or beside no `client_ca_file`, and
    /// [`Error::ListenerTls`] for a file that does not read or a key that
    /// does not match its certificate.
    pub(crate) fn resolve(&self, table: &'static str) -> Result<TlsFiles, Error> {
        let required = |field: &str, value: &Option<PathBuf>| {
            value.clone().ok_or_else(|| Error::Missing {
                key: format!("{table}.{field}"),
            })
        };
        let files = TlsFiles {
            table,
            certificate: required("certificate_file", &self.certificate_file)?,
            key: required("key_file", &self.key_file)?,
            client_ca: self.client_ca_file.clone(),
            healthcheck_identity: self.healthcheck_identity_file.clone(),
        };
        // NOTE: no specification governs this: our own design; the healthcheck asks
        // the client listener alone, and presents a certificate only where one is asked.
        if files.healthcheck_identity.is_some()
            && (table != SERVER_TLS || files.client_ca.is_none())
        {
            return Err(Error::HealthcheckIdentityUnused {
                key: format!("{table}.healthcheck_identity_file"),
            });
        }
        Certificates::load(files.clone())?;
        crate::healthcheck::Tls::read(&files)?;
        Ok(files)
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
) -> Result<Option<TlsFiles>, Error> {
    tls.map(|tls| tls.resolve(table)).transpose()
}

/// The table of the client listener's TLS.
pub const SERVER_TLS: &str = "server.tls";

/// The table of the admin listener's TLS.
pub const METRICS_TLS: &str = "metrics.tls";
