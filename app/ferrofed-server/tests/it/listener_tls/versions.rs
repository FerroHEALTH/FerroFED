// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The protocol versions and cipher suites the listener negotiates: TLS 1.3
//! alone by default (RFC 8446; RFC 9325 §3.1.1), and with `min_version =
//! "1.2"` TLS 1.3 when the client offers it, TLS 1.2 with a suite RFC 9325
//! §4.2 recommends, and no TLS 1.2 suite outside that list.

use std::net::SocketAddr;
use std::sync::Arc;

use ferrofed_testkit::listener::ListenerCertificates;
use ferrohealth_server::tls::TlsKeys;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{
    CipherSuite, ClientConfig, ProtocolVersion, RootCertStore, SupportedCipherSuite,
    SupportedProtocolVersion,
};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use super::{TestResult, kept, ready_over, tls_section};

/// A listener over a fresh certificate, speaking TLS 1.2 too when `tls12`
/// is set, and the CA a client trusts it by.
async fn listening(
    tls12: bool,
) -> Result<(SocketAddr, String, tempfile::TempDir), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let set = ListenerCertificates::generate()?;
    let files = set.write(dir.path())?;
    let tls = kept(&tls_section(&files, false, tls12), TlsKeys::SERVER)?;
    Ok((ready_over(tls).await?, set.ca().to_owned(), dir))
}

/// Opens a TLS connection to `address` trusting `ca`, offering `versions`
/// and the suites `keep` admits, and returns the negotiated version and
/// suite.
async fn handshake(
    address: SocketAddr,
    ca: &str,
    versions: &[&'static SupportedProtocolVersion],
    keep: impl Fn(&SupportedCipherSuite) -> bool,
) -> Result<(ProtocolVersion, CipherSuite), Box<dyn std::error::Error>> {
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from_pem_slice(ca.as_bytes())?)?;
    let mut provider = rustls::crypto::aws_lc_rs::default_provider();
    provider.cipher_suites.retain(|suite| keep(suite));
    let config = ClientConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(versions)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let stream = TcpStream::connect(address).await?;
    let tls = TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("127.0.0.1")?, stream)
        .await?;
    let (_, connection) = tls.get_ref();
    let version = connection
        .protocol_version()
        .ok_or("a version was negotiated")?;
    let suite = connection
        .negotiated_cipher_suite()
        .ok_or("a suite was negotiated")?
        .suite();
    Ok((version, suite))
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_13_is_negotiated_when_the_client_offers_it() -> TestResult {
    let (address, ca, _dir) = listening(true).await?;
    let both = [&rustls::version::TLS13, &rustls::version::TLS12];
    let (version, _) = handshake(address, &ca, &both, |_| true).await?;
    assert_eq!(
        ProtocolVersion::TLSv1_3,
        version,
        "RFC 9325 §3.1.1: TLS 1.3 is preferred"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_12_is_served_with_a_recommended_suite_and_refused_with_another() -> TestResult {
    let (address, ca, _dir) = listening(true).await?;
    let tls12 = [&rustls::version::TLS12];
    let (version, suite) = handshake(address, &ca, &tls12, |_| true).await?;
    assert_eq!(ProtocolVersion::TLSv1_2, version, "RFC 9325 §3.1.1");
    assert!(
        [
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
        ]
        .contains(&suite),
        "RFC 9325 §4.2, for the ECDSA certificate: {suite:?}"
    );
    let chacha = handshake(address, &ca, &tls12, |suite| {
        suite.suite() == CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
    })
    .await;
    assert!(
        chacha.is_err(),
        "RFC 9325 §4.2: a TLS 1.2 suite outside the four is refused"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn by_default_tls_13_alone_is_served_and_a_tls_12_client_is_refused() -> TestResult {
    let (address, ca, _dir) = listening(false).await?;
    let both = [&rustls::version::TLS13, &rustls::version::TLS12];
    let (version, _) = handshake(address, &ca, &both, |_| true).await?;
    assert_eq!(ProtocolVersion::TLSv1_3, version, "RFC 8446");
    let tls12 = handshake(address, &ca, &[&rustls::version::TLS12], |_| true).await;
    assert!(
        tls12.is_err(),
        "a client that offers TLS 1.2 alone is refused unless min_version is \"1.2\""
    );
    Ok(())
}
