// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The certificate a listener presents and the CA it admits clients by,
//! read from files at start and again on `SIGHUP`.
//!
//! [`Certificates`] holds the TLS server configuration the files describe and
//! hands each new connection the one in place when it arrives, so a reload
//! that reads valid files takes effect for the next handshake and leaves
//! every open connection on the certificate it began with. A reload whose
//! files do not read, or whose key does not match its certificate, is
//! refused and the running configuration stays.
//!
//! The listener negotiates TLS 1.3 (RFC 8446) or TLS 1.2 and nothing older,
//! and prefers TLS 1.3, as BCP 195 requires (RFC 9325 §3.1.1). Its TLS 1.2
//! cipher suites are the four RFC 9325 §4.2 recommends, every one of them
//! forward secret (§7.3); its TLS 1.3 suites are the crypto provider's,
//! aws-lc-rs, which every outbound connection of the gateway already uses.

use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{CipherSuite, RootCertStore, ServerConfig, SupportedCipherSuite};
use tokio_rustls::TlsAcceptor;

/// The TLS 1.2 cipher suites BCP 195 recommends (RFC 9325 §4.2).
const TLS12_SUITES: [CipherSuite; 4] = [
    CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
    CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
    CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
    CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
];

/// The files one listener's TLS is read from, every path already known to
/// be set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsFiles {
    /// The table the files were named in, `server.tls` or `metrics.tls`,
    /// which every refusal names.
    pub table: &'static str,
    /// The certificate chain the listener presents, PEM.
    pub certificate: PathBuf,
    /// The private key of that certificate, PEM.
    pub key: PathBuf,
    /// The CA certificates a client certificate must chain to, PEM; `None`
    /// asks for no client certificate.
    pub client_ca: Option<PathBuf>,
    /// The client certificate chain and key `ferrofed healthcheck`
    /// presents, PEM.
    pub healthcheck_identity: Option<PathBuf>,
}

/// TLS material a listener cannot serve with.
///
/// Every variant names the key the file was named under and never quotes
/// the file, so no part of a private key reaches a log line.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CertificateError {
    /// A file could not be read.
    #[error("{key} names {}, which could not be read", path.display())]
    Read {
        /// The key that named the file.
        key: String,
        /// The path it named.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A file holds no PEM certificate.
    #[error("{key} holds no PEM certificate")]
    NoCertificate {
        /// The key that named the file.
        key: String,
    },
    /// A file holds no PEM private key, or one that does not read.
    #[error("{key} holds no PEM private key")]
    NoKey {
        /// The key that named the file.
        key: String,
    },
    /// The client CA file holds a certificate no client can be verified by.
    #[error("{key} holds a certificate that is no usable trust anchor")]
    ClientCa {
        /// The key that named the file.
        key: String,
        /// What the verifier reported.
        #[source]
        source: rustls::Error,
    },
    /// No client verifier could be built over the client CA file.
    #[error("{key} gives no client certificate verifier")]
    Verifier {
        /// The key that named the file.
        key: String,
        /// What the verifier builder reported.
        #[source]
        source: rustls::server::VerifierBuilderError,
    },
    /// The certificate and the key do not form a server configuration: the
    /// key does not match the certificate, or is of a kind the provider
    /// does not sign with.
    #[error("{table}: the certificate and the key do not form a TLS server configuration")]
    Config {
        /// The table the files were named in.
        table: &'static str,
        /// What rustls reported.
        #[source]
        source: rustls::Error,
    },
}

impl TlsFiles {
    /// The key `field` is named under in this listener's table.
    fn key(&self, field: &str) -> String {
        format!("{}.{field}", self.table)
    }
}

/// One listener's TLS server configuration, replaced as a whole on a reload.
#[derive(Debug)]
pub struct Certificates {
    files: TlsFiles,
    current: RwLock<Arc<ServerConfig>>,
}

impl Certificates {
    /// Reads `files` into a server configuration.
    ///
    /// # Errors
    ///
    /// A [`CertificateError`] naming the key of the file that does not read,
    /// or [`CertificateError::Config`] for a key that does not match its
    /// certificate.
    pub fn load(files: TlsFiles) -> Result<Self, CertificateError> {
        let config = server_config(&files)?;
        Ok(Self {
            files,
            current: RwLock::new(Arc::new(config)),
        })
    }

    /// Reads the files again and puts the configuration they describe in
    /// place for every handshake that starts afterwards.
    ///
    /// # Errors
    ///
    /// A [`CertificateError`] as [`Certificates::load`] gives one; the
    /// running configuration then stays.
    pub fn reload(&self) -> Result<(), CertificateError> {
        let config = Arc::new(server_config(&self.files)?);
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = config;
        Ok(())
    }

    /// Returns the files the configuration is read from.
    #[must_use]
    pub fn files(&self) -> &TlsFiles {
        &self.files
    }

    /// Returns an acceptor over the configuration in place now.
    #[must_use]
    pub fn acceptor(&self) -> TlsAcceptor {
        let current = self.current.read().unwrap_or_else(PoisonError::into_inner);
        TlsAcceptor::from(Arc::clone(&current))
    }
}

/// Returns the crypto provider every listener uses: aws-lc-rs, its TLS 1.2
/// suites narrowed to the four of RFC 9325 §4.2.
#[must_use]
pub fn provider() -> Arc<CryptoProvider> {
    let mut provider = rustls::crypto::aws_lc_rs::default_provider();
    provider.cipher_suites.retain(|suite| match suite {
        SupportedCipherSuite::Tls13(_) => true,
        SupportedCipherSuite::Tls12(_) => TLS12_SUITES.contains(&suite.suite()),
    });
    Arc::new(provider)
}

/// Builds the server configuration `files` describe.
fn server_config(files: &TlsFiles) -> Result<ServerConfig, CertificateError> {
    let provider = provider();
    let chain = certificates(&files.key("certificate_file"), &files.certificate)?;
    let key = private_key(&files.key("key_file"), &files.key)?;
    let versions = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .map_err(|source| CertificateError::Config {
            table: files.table,
            source,
        })?;
    let builder = match &files.client_ca {
        None => versions.with_no_client_auth(),
        Some(path) => {
            let key = files.key("client_ca_file");
            let mut roots = RootCertStore::empty();
            for certificate in certificates(&key, path)? {
                roots
                    .add(certificate)
                    .map_err(|source| CertificateError::ClientCa {
                        key: key.clone(),
                        source,
                    })?;
            }
            let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
                .build()
                .map_err(|source| CertificateError::Verifier { key, source })?;
            versions.with_client_cert_verifier(verifier)
        }
    };
    let mut config =
        builder
            .with_single_cert(chain, key)
            .map_err(|source| CertificateError::Config {
                table: files.table,
                source,
            })?;
    // NOTE: no specification governs this: our own design; the server speaks
    // HTTP/1.1 alone, so ALPN offers that and nothing it cannot serve.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

/// Reads the PEM certificates in `path`, named under `key`.
///
/// # Errors
///
/// [`CertificateError::Read`] for a file that cannot be read, and
/// [`CertificateError::NoCertificate`] for one that holds no certificate or
/// one that does not read.
pub fn certificates(
    key: &str,
    path: &Path,
) -> Result<Vec<CertificateDer<'static>>, CertificateError> {
    let text = read(key, path)?;
    let chain = CertificateDer::pem_slice_iter(&text)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_unreadable| CertificateError::NoCertificate {
            key: key.to_owned(),
        })?;
    if chain.is_empty() {
        return Err(CertificateError::NoCertificate {
            key: key.to_owned(),
        });
    }
    Ok(chain)
}

/// Reads the PEM private key in `path`, named under `key`.
///
/// The parser's own message is not kept: it may quote a line of the file.
///
/// # Errors
///
/// [`CertificateError::Read`] for a file that cannot be read, and
/// [`CertificateError::NoKey`] for one that holds no private key.
pub fn private_key(key: &str, path: &Path) -> Result<PrivateKeyDer<'static>, CertificateError> {
    let text = read(key, path)?;
    PrivateKeyDer::from_pem_slice(&text).map_err(|_unreadable| CertificateError::NoKey {
        key: key.to_owned(),
    })
}

/// Reads `path`, named under `key`.
fn read(key: &str, path: &Path) -> Result<Vec<u8>, CertificateError> {
    std::fs::read(path).map_err(|source| CertificateError::Read {
        key: key.to_owned(),
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::{TLS12_SUITES, provider};
    use rustls::SupportedCipherSuite;

    #[test]
    fn the_tls12_suites_are_the_four_bcp_195_recommends_and_tls13_stays() {
        let provider = provider();
        let tls12: Vec<_> = provider
            .cipher_suites
            .iter()
            .filter(|suite| matches!(suite, SupportedCipherSuite::Tls12(_)))
            .map(SupportedCipherSuite::suite)
            .collect();
        assert_eq!(TLS12_SUITES.len(), tls12.len(), "RFC 9325 §4.2: {tls12:?}");
        assert!(tls12.iter().all(|suite| TLS12_SUITES.contains(suite)));
        assert!(
            provider
                .cipher_suites
                .iter()
                .any(|suite| matches!(suite, SupportedCipherSuite::Tls13(_))),
            "RFC 9325 §3.1.1: TLS 1.3 is offered"
        );
    }
}
