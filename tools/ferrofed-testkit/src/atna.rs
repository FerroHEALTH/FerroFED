// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A harness ATNA Audit Record Repository: it accepts ITI-20 syslog messages
//! over TLS (RFC 5425) and keeps every message it reads, for a test to
//! inspect (#418).
//!
//! Its CA and server certificate are generated at start, for `127.0.0.1`,
//! and never written anywhere: [`AuditRepository::trust_roots`] is the CA a
//! client trusts it by. While it is [down](AuditRepository::set_up), it
//! accepts each connection and closes it before the TLS handshake, so a
//! sender cannot deliver and keeps its messages; while it is
//! [stalled](AuditRepository::set_stalled), it accepts each connection and
//! never answers, so only a sender's own timeouts free it. No specification
//! governs the harness: our own design.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
};
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// Why the harness repository could not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AtnaHarnessError {
    /// The certificates could not be generated.
    #[error("the harness certificates could not be generated")]
    Certificate(#[from] rcgen::Error),
    /// The TLS configuration was refused.
    #[error("the harness TLS configuration was refused")]
    Tls(#[from] tokio_rustls::rustls::Error),
    /// The listener could not be bound.
    #[error("the harness repository could not listen")]
    Listen(#[from] std::io::Error),
}

/// A running harness Audit Record Repository.
#[derive(Debug)]
pub struct AuditRepository {
    address: SocketAddr,
    trust_roots: String,
    messages: Arc<Mutex<Vec<String>>>,
    up: Arc<AtomicBool>,
    stalled: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

impl AuditRepository {
    /// Starts a repository on a free loopback port, up.
    ///
    /// # Errors
    ///
    /// An [`AtnaHarnessError`] when the certificates, the TLS configuration
    /// or the listener cannot be made.
    pub async fn start() -> Result<Self, AtnaHarnessError> {
        let (trust_roots, config) = certificates()?;
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let messages = Arc::new(Mutex::new(Vec::new()));
        let up = Arc::new(AtomicBool::new(true));
        let stalled = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(serve(
            listener,
            acceptor,
            Arc::clone(&messages),
            Arc::clone(&up),
            Arc::clone(&stalled),
        ));
        Ok(Self {
            address,
            trust_roots,
            messages,
            up,
            stalled,
            task,
        })
    }

    /// Stalls the repository, or lets it go on: while stalled, it accepts
    /// each TCP connection and then never answers the TLS handshake, as a
    /// repository that hangs does.
    pub fn set_stalled(&self, stalled: bool) {
        self.stalled.store(stalled, Ordering::SeqCst);
    }

    /// The repository's address, `tls://127.0.0.1:<port>`.
    #[must_use]
    pub fn url(&self) -> String {
        format!("tls://{}", self.address)
    }

    /// The PEM CA certificate the repository's server certificate chains to.
    #[must_use]
    pub fn trust_roots(&self) -> &str {
        &self.trust_roots
    }

    /// Brings the repository up, or takes it down.
    pub fn set_up(&self, up: bool) {
        self.up.store(up, Ordering::SeqCst);
    }

    /// Every syslog message read so far, in arrival order, without its
    /// octet count.
    #[must_use]
    pub fn messages(&self) -> Vec<String> {
        self.messages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Waits up to `within` until `count` messages have arrived, and returns
    /// every message read by then.
    pub async fn wait_for(&self, count: usize, within: Duration) -> Vec<String> {
        let until = Instant::now() + within;
        loop {
            let messages = self.messages();
            if messages.len() >= count || Instant::now() >= until {
                return messages;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

impl Drop for AuditRepository {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The CA certificate as PEM, and a server configuration for `127.0.0.1`
/// whose certificate it signs.
fn certificates() -> Result<(String, ServerConfig), AtnaHarnessError> {
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
        .push(DnType::CommonName, "FerroFED harness audit repository CA");
    ca.not_before = ymd(yesterday);
    ca.not_after = ymd(tomorrow);
    let ca_certificate = ca.self_signed(&ca_key)?;
    let issuer = Issuer::new(ca, ca_key);

    let server_key = KeyPair::generate()?;
    let mut server = CertificateParams::new(vec![Ipv4Addr::LOCALHOST.to_string()])?;
    server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server
        .distinguished_name
        .push(DnType::CommonName, "harness audit repository");
    server.not_before = ymd(yesterday);
    server.not_after = ymd(tomorrow);
    let server_certificate = server.signed_by(&server_key, &issuer)?;

    let chain: Vec<CertificateDer<'static>> = vec![server_certificate.der().clone()];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key.serialize_der()));
    let provider = Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(chain, key)?;
    Ok((ca_certificate.pem(), config))
}

/// Accepts connections until the task is aborted.
async fn serve(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    messages: Arc<Mutex<Vec<String>>>,
    up: Arc<AtomicBool>,
    stalled: Arc<AtomicBool>,
) {
    let mut held = Vec::new();
    while let Ok((stream, _peer)) = listener.accept().await {
        if stalled.load(Ordering::SeqCst) {
            held.push(stream);
            continue;
        }
        if !up.load(Ordering::SeqCst) {
            drop(stream);
            continue;
        }
        let acceptor = acceptor.clone();
        let messages = Arc::clone(&messages);
        tokio::spawn(async move {
            if let Ok(tls) = acceptor.accept(stream).await {
                read_frames(tls, &messages).await;
            }
        });
    }
}

/// Reads RFC 5425 §4.3 frames, `MSG-LEN SP SYSLOG-MSG`, until the
/// connection ends or a frame is malformed.
async fn read_frames(mut stream: impl AsyncRead + Unpin, messages: &Mutex<Vec<String>>) {
    loop {
        let mut length = 0_usize;
        loop {
            let Ok(byte) = stream.read_u8().await else {
                return;
            };
            match byte {
                b' ' => break,
                b'0'..=b'9' => {
                    length = length
                        .saturating_mul(10)
                        .saturating_add(usize::from(byte - b'0'));
                }
                _ => return,
            }
        }
        let mut message = vec![0_u8; length];
        if stream.read_exact(&mut message).await.is_err() {
            return;
        }
        messages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(String::from_utf8_lossy(&message).into_owned());
    }
}
