// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A mutual-TLS front for any harness server (#507).
//!
//! It terminates TLS on a loopback port, admits only a client that presents
//! a certificate its CA signed, and passes every byte through to the plain
//! `http` origin behind it.
//!
//! A harness server, a PIX Manager, a PDQm Supplier, a Patient Identity
//! Registry or a care services directory, serves `http`; this front puts
//! that same server behind `https` with client authentication, so a test
//! exercises the gateway's mutual TLS against the real harness. Its CA, its
//! server certificate for `127.0.0.1` and the client certificate are
//! generated at start and never written anywhere:
//! [`MutualTls::trust_roots`] is the CA the gateway trusts the front by, and
//! [`MutualTls::client_identity`] is the certificate chain and key the
//! gateway presents. A connection without a client certificate the CA
//! signed fails its handshake and never reaches the origin. The front
//! records the RFC 8705 §3.1 thumbprint of the certificate each connection
//! presented ([`MutualTls::presented`]), so a test can show that a
//! certificate-bound token travelled over a connection presenting the
//! certificate it is bound to. No specification governs the harness: our
//! own design.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::{RootCertStore, ServerConfig};

/// Why the front could not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TlsHarnessError {
    /// The certificates could not be generated.
    #[error("the harness certificates could not be generated")]
    Certificate(#[from] rcgen::Error),
    /// The TLS configuration was refused.
    #[error("the harness TLS configuration was refused")]
    Tls(#[from] tokio_rustls::rustls::Error),
    /// The client verifier could not be built.
    #[error("the harness client verifier could not be built")]
    Verifier(#[from] tokio_rustls::rustls::server::VerifierBuilderError),
    /// The listener could not be bound.
    #[error("the harness front could not listen")]
    Listen(#[from] std::io::Error),
    /// The origin is no `http://<host>:<port>` address.
    #[error("the origin is no http://<host>:<port> address")]
    Origin,
}

/// A running mutual-TLS front, stopped when it is dropped.
pub struct MutualTls {
    address: SocketAddr,
    trust_roots: String,
    client_identity: String,
    client_thumbprint: String,
    handshakes: Arc<AtomicUsize>,
    refused: Arc<AtomicUsize>,
    presented: Arc<Mutex<Vec<String>>>,
    stop: Option<oneshot::Sender<()>>,
}

impl std::fmt::Debug for MutualTls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutualTls")
            .field("address", &self.address)
            .field("handshakes", &self.handshakes())
            .field("refused", &self.refused())
            .finish_non_exhaustive()
    }
}

impl MutualTls {
    /// Starts a front on a free loopback port that passes every connection
    /// a client certificate opened through to `origin`, an
    /// `http://<host>:<port>` address with no path.
    ///
    /// # Errors
    ///
    /// A [`TlsHarnessError`] when the origin does not read, or the
    /// certificates, the TLS configuration or the listener cannot be made.
    pub fn front(origin: &str) -> Result<Self, TlsHarnessError> {
        Self::over(origin, &material()?)
    }

    /// Starts two fronts on two free loopback ports, two origins, that pass
    /// every connection to `origin` and share one CA, one server
    /// certificate and one client certificate, so a client presenting one
    /// identity reaches both.
    ///
    /// # Errors
    ///
    /// A [`TlsHarnessError`] as [`MutualTls::front`] gives one.
    pub fn pair(origin: &str) -> Result<(Self, Self), TlsHarnessError> {
        let material = material()?;
        Ok((
            Self::over(origin, &material)?,
            Self::over(origin, &material)?,
        ))
    }

    /// Starts a front over `material` that passes every connection to
    /// `origin`.
    fn over(origin: &str, material: &Material) -> Result<Self, TlsHarnessError> {
        let target = origin
            .strip_prefix("http://")
            .map(|rest| rest.trim_end_matches('/'))
            .filter(|rest| !rest.is_empty() && !rest.contains('/'))
            .ok_or(TlsHarnessError::Origin)?
            .to_owned();
        let acceptor = TlsAcceptor::from(Arc::new(material.config.clone()));
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let handshakes = Arc::new(AtomicUsize::new(0));
        let refused = Arc::new(AtomicUsize::new(0));
        let presented = Arc::new(Mutex::new(Vec::new()));
        let counters = Counters {
            handshakes: Arc::clone(&handshakes),
            refused: Arc::clone(&refused),
            presented: Arc::clone(&presented),
        };
        // The front runs on a runtime of its own, as wiremock's servers do, so a
        // caller that blocks its own runtime still reaches the origin.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (stop, stopped) = oneshot::channel();
        std::thread::Builder::new()
            .name(String::from("mutual-tls-front"))
            .spawn(move || {
                runtime.block_on(async move {
                    let Ok(listener) = TcpListener::from_std(listener) else {
                        return;
                    };
                    tokio::select! {
                        () = serve(listener, acceptor, target, counters) => {}
                        _ = stopped => {}
                    }
                });
            })?;
        Ok(Self {
            address,
            trust_roots: material.trust_roots.clone(),
            client_identity: material.client_identity.clone(),
            client_thumbprint: material.client_thumbprint.clone(),
            handshakes,
            refused,
            presented,
            stop: Some(stop),
        })
    }

    /// Returns the `https://127.0.0.1:<port>` origin of the front, with no
    /// path.
    #[must_use]
    pub fn origin(&self) -> String {
        format!("https://{}", self.address)
    }

    /// Returns the CA certificate, PEM, the gateway trusts the front by.
    #[must_use]
    pub fn trust_roots(&self) -> &str {
        &self.trust_roots
    }

    /// Returns the client certificate and its private key, PEM, which the
    /// front admits.
    #[must_use]
    pub fn client_identity(&self) -> &str {
        &self.client_identity
    }

    /// Returns how many connections completed the handshake with a client
    /// certificate the CA signed.
    #[must_use]
    pub fn handshakes(&self) -> usize {
        self.handshakes.load(Ordering::SeqCst)
    }

    /// Returns how many connections failed the handshake, a client without a
    /// certificate the CA signed among them.
    #[must_use]
    pub fn refused(&self) -> usize {
        self.refused.load(Ordering::SeqCst)
    }

    /// Returns the RFC 8705 §3.1 thumbprint of the client certificate the
    /// front admits: the unpadded base64url SHA-256 of its DER encoding.
    #[must_use]
    pub fn client_thumbprint(&self) -> &str {
        &self.client_thumbprint
    }

    /// Returns the thumbprint of the certificate each connection that
    /// completed its handshake presented, in arrival order.
    #[must_use]
    pub fn presented(&self) -> Vec<String> {
        self.presented
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The counters a front keeps of its connections.
#[derive(Clone)]
struct Counters {
    handshakes: Arc<AtomicUsize>,
    refused: Arc<AtomicUsize>,
    presented: Arc<Mutex<Vec<String>>>,
}

/// The RFC 8705 §3.1 thumbprint of the certificate whose DER is `der`.
fn thumbprint(der: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, der))
}

impl Drop for MutualTls {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            // NOTE: no specification governs this: our own design; a front whose
            // thread already ended has nothing left to stop.
            let _stopped: Result<(), ()> = stop.send(());
        }
    }
}

/// The certificates of one front and the server configuration over them.
struct Material {
    trust_roots: String,
    client_identity: String,
    client_thumbprint: String,
    config: ServerConfig,
}

/// A CA, a server certificate for `127.0.0.1` and a client certificate it
/// signs, and a server configuration that requires a client certificate the
/// CA signed.
fn material() -> Result<Material, TlsHarnessError> {
    let today = jiff::Zoned::now().date();
    let (yesterday, tomorrow) = (
        today.yesterday().unwrap_or(today),
        today.tomorrow().unwrap_or(today),
    );
    let ymd = |date: jiff::civil::Date| {
        rcgen::date_time_ymd(
            i32::from(date.year()),
            u8::try_from(date.month()).unwrap_or(1),
            u8::try_from(date.day()).unwrap_or(1),
        )
    };
    let ca_key = KeyPair::generate()?;
    let mut ca = CertificateParams::new(Vec::<String>::new())?;
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca.distinguished_name
        .push(DnType::CommonName, "FerroFED harness mutual TLS CA");
    ca.not_before = ymd(yesterday);
    ca.not_after = ymd(tomorrow);
    let ca_certificate = ca.self_signed(&ca_key)?;
    let issuer = Issuer::new(ca, ca_key);

    let server_key = KeyPair::generate()?;
    let mut server = CertificateParams::new(vec![Ipv4Addr::LOCALHOST.to_string()])?;
    server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server
        .distinguished_name
        .push(DnType::CommonName, "harness mutual TLS front");
    server.not_before = ymd(yesterday);
    server.not_after = ymd(tomorrow);
    let server_certificate = server.signed_by(&server_key, &issuer)?;

    let client_key = KeyPair::generate()?;
    let mut client = CertificateParams::new(Vec::<String>::new())?;
    client.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    client
        .distinguished_name
        .push(DnType::CommonName, "harness gateway client");
    client.not_before = ymd(yesterday);
    client.not_after = ymd(tomorrow);
    let client_certificate = client.signed_by(&client_key, &issuer)?;

    let mut roots = RootCertStore::empty();
    roots.add(ca_certificate.der().clone())?;
    let provider = Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider());
    let verifier =
        WebPkiClientVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
            .build()?;
    let chain: Vec<CertificateDer<'static>> = vec![server_certificate.der().clone()];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key.serialize_der()));
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(verifier)
        .with_single_cert(chain, key)?;
    Ok(Material {
        trust_roots: ca_certificate.pem(),
        client_identity: format!("{}{}", client_certificate.pem(), client_key.serialize_pem()),
        client_thumbprint: thumbprint(client_certificate.der()),
        config,
    })
}

/// Accepts connections until the front is stopped, passing each one a
/// client certificate opened through to `target`.
async fn serve(listener: TcpListener, acceptor: TlsAcceptor, target: String, counters: Counters) {
    while let Ok((stream, _peer)) = listener.accept().await {
        let acceptor = acceptor.clone();
        let target = target.clone();
        let counters = counters.clone();
        tokio::spawn(async move {
            let Ok(mut tls) = acceptor.accept(stream).await else {
                counters.refused.fetch_add(1, Ordering::SeqCst);
                return;
            };
            counters.handshakes.fetch_add(1, Ordering::SeqCst);
            if let Some(certificate) = tls
                .get_ref()
                .1
                .peer_certificates()
                .and_then(<[CertificateDer<'_>]>::first)
            {
                counters
                    .presented
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(thumbprint(certificate));
            }
            let Ok(mut origin) = TcpStream::connect(target.as_str()).await else {
                return;
            };
            // A connection ends in an error whenever a side hangs up, which a
            // test does on purpose; the front has nothing to report about it.
            match tokio::io::copy_bidirectional(&mut tls, &mut origin).await {
                Ok(_) | Err(_) => {}
            }
        });
    }
}
