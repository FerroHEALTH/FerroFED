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
//!
//! Where `[server.tls]` is set, the job speaks TLS ([`Tls`]). It asks over
//! loopback, where the listener's certificate names no address the job
//! connects to, so it accepts exactly the certificate
//! `server.tls.certificate_file` holds and no other, and it presents
//! `server.tls.healthcheck_identity_file` to a listener that requires a
//! client certificate.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use http::StatusCode;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};

use crate::listener::certificates::{self, CertificateError, TlsFiles};

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
    /// The listener presented a certificate other than the one
    /// `server.tls.certificate_file` holds: the file was replaced and the
    /// gateway has not read it again.
    OtherCertificate,
    /// The TLS handshake failed.
    Handshake(reqwest::Error),
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
            Self::OtherCertificate => f.write_str(
                "not ready: the listener presents a certificate other than server.tls.certificate_file holds; send SIGHUP so the gateway reads it",
            ),
            Self::Handshake(error) => {
                write!(
                    f,
                    "not ready: the TLS handshake failed: {}",
                    crate::chain(error)
                )
            }
            Self::Failed(error) => {
                write!(f, "not ready: the request failed: {}", crate::chain(error))
            }
        }
    }
}

/// The TLS the job speaks to a listener that serves it.
#[derive(Debug, Clone)]
pub struct Tls {
    config: ClientConfig,
    other: Arc<AtomicBool>,
}

impl Tls {
    /// Reads the certificate the listener serves and the identity the job
    /// presents from `files`.
    ///
    /// # Errors
    ///
    /// A [`CertificateError`] naming the key of a file that does not read,
    /// and [`CertificateError::Config`] for an identity whose key does not
    /// match its certificate.
    pub fn read(files: &TlsFiles) -> Result<Self, CertificateError> {
        let certificate = format!("{}.certificate_file", files.table);
        let leaf = certificates::certificates(&certificate, &files.certificate)?
            .into_iter()
            .next()
            .ok_or(CertificateError::NoCertificate { key: certificate })?;
        let provider = certificates::provider();
        let other = Arc::new(AtomicBool::new(false));
        let pinned = Pinned {
            leaf,
            algorithms: provider.signature_verification_algorithms,
            other: Arc::clone(&other),
        };
        let config = |source| CertificateError::Config {
            table: files.table,
            source,
        };
        let builder = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
            .map_err(config)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(pinned));
        let config = match &files.healthcheck_identity {
            None => builder.with_no_client_auth(),
            Some(path) => {
                let key = format!("{}.healthcheck_identity_file", files.table);
                let chain = certificates::certificates(&key, path)?;
                let private = certificates::private_key(&key, path)?;
                builder
                    .with_client_auth_cert(chain, private)
                    .map_err(config)?
            }
        };
        Ok(Self { config, other })
    }
}

/// The verifier that accepts one certificate: the one the listener's own
/// certificate file holds.
#[derive(Debug)]
struct Pinned {
    leaf: CertificateDer<'static>,
    algorithms: WebPkiSupportedAlgorithms,
    other: Arc<AtomicBool>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // NOTE: no specification governs this: our own design; over loopback the
        // job trusts the gateway's own certificate byte for byte, and no name.
        if end_entity.as_ref() == self.leaf.as_ref() {
            return Ok(ServerCertVerified::assertion());
        }
        self.other.store(true, Ordering::SeqCst);
        Err(rustls::Error::InvalidCertificate(
            rustls::CertificateError::ApplicationVerificationFailure,
        ))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
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
/// most `timeout`, over `tls` when the listener serves it.
///
/// No proxy is consulted and no redirect is followed: the gateway is on
/// this host, and only its own answer counts.
pub async fn check(
    address: SocketAddr,
    path: &str,
    timeout: Duration,
    tls: Option<&Tls>,
) -> Outcome {
    let (client, scheme) = match client(timeout, tls) {
        Ok(built) => built,
        Err(error) => return Outcome::Failed(error),
    };
    match client
        .get(format!("{scheme}://{address}{path}"))
        .send()
        .await
    {
        Ok(response) if response.status() == StatusCode::OK => Outcome::Ready,
        Ok(response) => Outcome::NotReady(response.status()),
        Err(_) if tls.is_some_and(|tls| tls.other.load(Ordering::SeqCst)) => {
            Outcome::OtherCertificate
        }
        Err(error) if error.is_timeout() => Outcome::TimedOut(timeout),
        Err(error) if tls.is_some() && handshake(&error) => Outcome::Handshake(error),
        Err(error) if error.is_connect() => Outcome::Refused,
        Err(error) => Outcome::Failed(error),
    }
}

/// Returns the client that asks a listener on this host, waiting at most
/// `timeout`, over `tls` when the listener serves it, with the scheme its
/// URLs take.
///
/// No proxy is consulted and no redirect is followed: the listener is on
/// this host, and only its own answer counts.
pub(crate) fn client(
    timeout: Duration,
    tls: Option<&Tls>,
) -> Result<(reqwest::Client, &'static str), reqwest::Error> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none());
    let scheme = match tls {
        None => "http",
        Some(tls) => {
            builder = builder.tls_backend_preconfigured(tls.config.clone());
            "https"
        }
    };
    builder.build().map(|client| (client, scheme))
}

/// Whether a TLS failure is among the causes of `error`.
fn handshake(error: &reqwest::Error) -> bool {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = cause {
        let inner = current
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::get_ref)
            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
            .is_some();
        if inner || current.is::<rustls::Error>() {
            return true;
        }
        cause = current.source();
    }
    false
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
