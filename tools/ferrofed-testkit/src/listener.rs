// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Synthetic certificates for the gateway's own TLS listener (#632).
//!
//! [`ListenerCertificates::generate`] makes a server CA and a server
//! certificate for `127.0.0.1` and `localhost` it signs, and a client CA and
//! a client certificate it signs, each pair generated at run time over the
//! crypto library rustls builds on and never committed. A test writes them
//! to files ([`ListenerCertificates::write`]) for the `[server.tls]` keys,
//! and reaches the listener as a client trusting [`ListenerCertificates::ca`].
//! Two calls give two unrelated sets, so a test can swap one certificate for
//! another. No specification governs the harness: our own design.

use std::io;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use rcgen::ExtendedKeyUsagePurpose;

use crate::pki::Authority;
use crate::tls::TlsHarnessError;

/// A server certificate with its CA, and a client certificate with its own
/// CA, as PEM.
pub struct ListenerCertificates {
    ca: String,
    chain: String,
    key: String,
    leaf: Vec<u8>,
    client_ca: String,
    client_identity: String,
}

impl std::fmt::Debug for ListenerCertificates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListenerCertificates")
            .finish_non_exhaustive()
    }
}

/// Where [`ListenerCertificates::write`] put each file.
#[derive(Debug, Clone)]
pub struct ListenerFiles {
    /// The server certificate, PEM: `certificate_file`.
    pub certificate: PathBuf,
    /// The server's private key, PEM: `key_file`.
    pub key: PathBuf,
    /// The client CA, PEM: `client_ca_file`.
    pub client_ca: PathBuf,
    /// The client certificate and its key, PEM: `healthcheck_identity_file`.
    pub client_identity: PathBuf,
}

impl ListenerCertificates {
    /// Generates a fresh set, valid from yesterday to tomorrow.
    ///
    /// # Errors
    ///
    /// [`TlsHarnessError::Certificate`] when a key or a certificate cannot be
    /// made.
    pub fn generate() -> Result<Self, TlsHarnessError> {
        let ca = Authority::generate("FerroFED harness listener CA")?;
        let (server, server_key) = ca.issue(
            "harness gateway listener",
            vec![Ipv4Addr::LOCALHOST.to_string(), String::from("localhost")],
            ExtendedKeyUsagePurpose::ServerAuth,
        )?;
        let client_ca = Authority::generate("FerroFED harness proxy CA")?;
        let (client, client_key) = client_ca.issue(
            "harness proxy",
            Vec::new(),
            ExtendedKeyUsagePurpose::ClientAuth,
        )?;
        Ok(Self {
            ca: ca.certificate.pem(),
            chain: server.pem(),
            key: server_key.serialize_pem(),
            leaf: server.der().to_vec(),
            client_ca: client_ca.certificate.pem(),
            client_identity: format!("{}{}", client.pem(), client_key.serialize_pem()),
        })
    }

    /// Returns the CA certificate, PEM, a client trusts the listener by.
    #[must_use]
    pub fn ca(&self) -> &str {
        &self.ca
    }

    /// Returns the server certificate, PEM.
    #[must_use]
    pub fn chain(&self) -> &str {
        &self.chain
    }

    /// Returns the server's private key, PEM.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the DER of the server certificate, which a client reads back
    /// from the handshake to tell one set from another.
    #[must_use]
    pub fn leaf(&self) -> &[u8] {
        &self.leaf
    }

    /// Returns the CA certificate, PEM, the listener admits a client
    /// certificate by.
    #[must_use]
    pub fn client_ca(&self) -> &str {
        &self.client_ca
    }

    /// Returns the client certificate and its private key, PEM, which the
    /// client CA signed.
    #[must_use]
    pub fn client_identity(&self) -> &str {
        &self.client_identity
    }

    /// Writes the server certificate, its key, the client CA and the client
    /// identity into `dir`, replacing files of the same names.
    ///
    /// # Errors
    ///
    /// The I/O error of a write.
    pub fn write(&self, dir: &Path) -> io::Result<ListenerFiles> {
        let files = ListenerFiles {
            certificate: dir.join("listener.crt"),
            key: dir.join("listener.key"),
            client_ca: dir.join("client-ca.pem"),
            client_identity: dir.join("client.pem"),
        };
        std::fs::write(&files.certificate, &self.chain)?;
        std::fs::write(&files.key, &self.key)?;
        std::fs::write(&files.client_ca, &self.client_ca)?;
        std::fs::write(&files.client_identity, &self.client_identity)?;
        Ok(files)
    }
}
