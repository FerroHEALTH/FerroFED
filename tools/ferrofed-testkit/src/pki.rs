// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic certificates of the harness TLS servers.
//!
//! They serve the gateway's listener ([`crate::listener`]), the mutual-TLS
//! front ([`crate::tls`]), the Audit Record Repository ([`crate::atna`]) and
//! the revocation harness ([`crate::revocation`]).
//!
//! Each is a CA generated at run time and the server or client certificates
//! it signs, valid from yesterday to tomorrow and never committed, and the
//! server configuration `ferrohealth-server` builds over them. No
//! specification governs the harness: our own design.

use std::io;
use std::sync::Arc;

use ferrohealth_server::section::{ClientAuth, TlsSection, TlsVersion};
use ferrohealth_server::tls::{self, TlsError, TlsKeys};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair,
};
use tokio_rustls::rustls::ServerConfig;

/// A CA: its self-signed certificate and the issuer that signs with its key.
pub(crate) struct Authority {
    /// The CA certificate.
    pub(crate) certificate: Certificate,
    pub(crate) issuer: Issuer<'static, KeyPair>,
}

impl Authority {
    /// Generates a CA named `name`.
    pub(crate) fn generate(name: &str) -> Result<Self, rcgen::Error> {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::new(Vec::<String>::new())?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.distinguished_name.push(DnType::CommonName, name);
        validity(&mut params);
        let certificate = params.self_signed(&key)?;
        Ok(Self {
            certificate,
            issuer: Issuer::new(params, key),
        })
    }

    /// Signs a certificate named `name` for `usage` with a fresh key, for
    /// the subject alternative names `names`, and returns it with its key.
    pub(crate) fn issue(
        &self,
        name: &str,
        names: Vec<String>,
        usage: ExtendedKeyUsagePurpose,
    ) -> Result<(Certificate, KeyPair), rcgen::Error> {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::new(names)?;
        params.extended_key_usages = vec![usage];
        params.distinguished_name.push(DnType::CommonName, name);
        validity(&mut params);
        let certificate = params.signed_by(&key, &self.issuer)?;
        Ok((certificate, key))
    }
}

/// Why a harness server's TLS configuration could not be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PkiError {
    /// The certificate files `ferrohealth-server` reads could not be
    /// written.
    #[error("the harness certificate files could not be written")]
    Write(#[source] io::Error),
    /// `ferrohealth-server` refused the certificates.
    #[error("the harness TLS configuration was refused")]
    Tls(#[source] TlsError),
}

/// The configuration of a harness server that presents `certificate` with
/// `key`, built by `ferrohealth-server` as a listener's is, and requires a
/// client certificate `clients` signed when it is given. It speaks TLS 1.2
/// and 1.3 and offers no ALPN protocol, so a client speaks the protocol of
/// the server behind it.
pub(crate) fn server_tls(
    (certificate, key): (&Certificate, &KeyPair),
    clients: Option<&Certificate>,
) -> Result<ServerConfig, PkiError> {
    server_tls_chain((&certificate.pem(), key), clients)
}

/// The configuration [`server_tls`] builds, presenting the PEM chain
/// `chain`, the server certificate first and its intermediates after it.
pub(crate) fn server_tls_chain(
    (chain, key): (&str, &KeyPair),
    clients: Option<&Certificate>,
) -> Result<ServerConfig, PkiError> {
    let dir = tempfile::tempdir().map_err(PkiError::Write)?;
    let write = |name: &str, pem: String| {
        let path = dir.path().join(name);
        std::fs::write(&path, pem)
            .map(|()| path)
            .map_err(PkiError::Write)
    };
    let client_ca_file = clients
        .map(|ca| write("client-ca.pem", ca.pem()))
        .transpose()?;
    let section = TlsSection {
        certificate_file: write("server.pem", chain.to_owned())?,
        key_file: write("server.key", key.serialize_pem())?,
        client_auth: if client_ca_file.is_some() {
            ClientAuth::Required
        } else {
            ClientAuth::None
        },
        client_ca_file,
        reload_interval_s: None,
        healthcheck_identity_file: None,
        min_version: TlsVersion::Tls12,
    };
    let mut config = Arc::unwrap_or_clone(
        tls::server_config_of(&section, TlsKeys::SERVER).map_err(PkiError::Tls)?,
    );
    config.alpn_protocols.clear();
    Ok(config)
}

/// Makes `params` valid from yesterday to tomorrow.
pub(crate) fn validity(params: &mut CertificateParams) {
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
    params.not_before = ymd(yesterday);
    params.not_after = ymd(tomorrow);
}
