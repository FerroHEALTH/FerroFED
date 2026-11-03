// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic certificates of the harness TLS servers: the gateway's
//! listener ([`crate::listener`]), the mutual-TLS front ([`crate::tls`]) and
//! the Audit Record Repository ([`crate::atna`]).
//!
//! Each is a CA generated at run time and the server or client certificates
//! it signs, valid from yesterday to tomorrow and never committed, and a
//! rustls server configuration over the crypto library rustls builds on. No
//! specification governs the harness: our own design.

use std::sync::Arc;

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::danger::ClientCertVerifier;
use tokio_rustls::rustls::{ServerConfig, crypto::CryptoProvider};

/// A CA: its self-signed certificate and the issuer that signs with its key.
pub(crate) struct Authority {
    /// The CA certificate.
    pub(crate) certificate: Certificate,
    issuer: Issuer<'static, KeyPair>,
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

/// The rustls provider every harness server runs on.
pub(crate) fn provider() -> Arc<CryptoProvider> {
    Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider())
}

/// A server configuration over `provider` that presents `certificate` with
/// `key`, and requires a client certificate `clients` verifies when it is
/// given.
pub(crate) fn server_config(
    provider: Arc<CryptoProvider>,
    (certificate, key): (&Certificate, &KeyPair),
    clients: Option<Arc<dyn ClientCertVerifier>>,
) -> Result<ServerConfig, tokio_rustls::rustls::Error> {
    let chain: Vec<CertificateDer<'static>> = vec![certificate.der().clone()];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
    let builder =
        ServerConfig::builder_with_provider(provider).with_safe_default_protocol_versions()?;
    match clients {
        Some(verifier) => builder
            .with_client_cert_verifier(verifier)
            .with_single_cert(chain, key),
        None => builder.with_no_client_auth().with_single_cert(chain, key),
    }
}

/// Makes `params` valid from yesterday to tomorrow.
fn validity(params: &mut CertificateParams) {
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
