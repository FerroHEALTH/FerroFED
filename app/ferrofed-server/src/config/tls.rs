// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The TLS material a table names for the service or the node it reaches.
//!
//! `[[pixm.manager]]`, `[pdqm]`, `[pmir]`, `[registry.mcsd]` and a node's
//! `[credentials."<endpoint id>"]` take the keys `[xcpd]` takes:
//! `client_identity`, inline, or `client_identity_file`, the gateway's client
//! certificate chain and private key in PEM for mutual TLS, held as a
//! secret; and `trust_roots_file`, a PEM bundle of trust roots beside the
//! platform's. Every file is read once, at load. A node's material serves
//! the node and its authorization server alike, so a token bound to the
//! certificate travels only over connections that present it (RFC 8705 §3).
//! No specification governs the keys: our own design, as `[xcpd]` names
//! them.

use std::path::{Path, PathBuf};

use ferrofed_engine::onward::mtls::ThumbprintError;
use ferrofed_registry::secret::Secret;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::Deserialize;

use crate::config::error::Error;
use crate::config::secrets::secret;

/// TLS material, or a use of mutual TLS, the gateway cannot take.
///
/// No variant carries any part of a certificate or a key.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TlsFault {
    /// A service's credentials section names TLS material, which that
    /// service's own table takes.
    #[error(
        "{section} names client_identity or trust_roots_file, which only a node's [credentials] section takes; set them on the service's own table"
    )]
    OnService {
        /// The credentials section.
        section: String,
    },
    /// A grant uses mutual TLS, and its endpoint's section names no client
    /// identity to present.
    #[error(
        "{section} authenticates or binds its tokens by a TLS client certificate (RFC 8705), and its credentials section sets no client_identity"
    )]
    WithoutClientIdentity {
        /// The grant's section.
        section: String,
    },
    /// The client identity holds no certificate whose thumbprint the
    /// gateway can take (RFC 8705 §3.1).
    #[error("{key} holds no certificate a token can be bound to (RFC 8705 §3.1)")]
    Identity {
        /// The key the identity was read from.
        key: String,
        /// Why it does not read; it quotes no part of the PEM.
        #[source]
        source: ThumbprintError,
    },
    /// A URL the gateway presents its client certificate to is not
    /// `https`.
    #[error(
        "{key} is not https: the gateway presents its TLS client certificate there, which needs TLS under every profile"
    )]
    Cleartext {
        /// The key, or the description, of the URL.
        key: String,
    },
}

/// The TLS material of one service, with every file read.
///
/// `Debug` shows the client identity as `***`, because [`Secret`] does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsSettings {
    /// The gateway's client certificate chain and private key, PEM.
    pub client_identity: Option<Secret>,
    /// The PEM trust roots beside the platform's.
    pub trust_roots: Option<String>,
}

/// Reads the TLS material of the table at `key`: the client identity, inline
/// or from `identity_file`, and the trust roots from `roots_file`.
///
/// # Errors
///
/// [`Error::Conflict`] for an identity given inline and as a file,
/// [`Error::Secret`] and [`Error::EmptySecret`] for a file that cannot be
/// read or is empty.
pub(crate) fn resolve(
    key: &str,
    identity: Option<&Secret>,
    identity_file: Option<&Path>,
    roots_file: Option<&PathBuf>,
) -> Result<TlsSettings, Error> {
    let client_identity = secret(&format!("{key}.client_identity"), identity, identity_file)?;
    let trust_roots = roots_file
        .map(|path| {
            std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: format!("{key}.trust_roots_file"),
                path: path.clone(),
                source,
            })
        })
        .transpose()?;
    Ok(TlsSettings {
        client_identity,
        trust_roots,
    })
}

/// The TLS an OTLP collector is reached with (`[telemetry.otlp_tls]`,
/// `[metrics.otlp_tls]`): the client identity the gateway presents, inline
/// or from a file, and trust roots beside the platform's.
///
/// It takes effect only with an `https://` `otlp_endpoint`. No specification
/// governs the keys: our own design, as the other tables name them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CollectorTls {
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS with the collector.
    pub client_identity: Option<Secret>,
    /// A file holding `client_identity`, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A PEM bundle of trust roots, beside the platform's, the collector is
    /// trusted by.
    pub trust_roots_file: Option<PathBuf>,
}

impl CollectorTls {
    /// Whether the table names any TLS material.
    #[must_use]
    pub fn names_tls(&self) -> bool {
        self.client_identity.is_some()
            || self.client_identity_file.is_some()
            || self.trust_roots_file.is_some()
    }
}

/// Reads the collector TLS `table` at `key`, every file once, and checks
/// that the client identity holds a certificate and a private key and that
/// the trust roots hold a certificate.
///
/// # Errors
///
/// The errors of [`resolve`], and [`Error::Pem`] for material that is no
/// PEM of the kind its key takes.
pub(crate) fn resolve_collector(key: &str, table: &CollectorTls) -> Result<TlsSettings, Error> {
    let tls = resolve(
        key,
        table.client_identity.as_ref(),
        table.client_identity_file.as_deref(),
        table.trust_roots_file.as_ref(),
    )?;
    if let Some(identity) = &tls.client_identity {
        let pem = identity.expose().as_bytes();
        if !holds_certificates(pem) || PrivateKeyDer::from_pem_slice(pem).is_err() {
            return Err(Error::Pem {
                key: format!("{key}.client_identity"),
                holds: "a certificate chain and its private key",
            });
        }
    }
    if let Some(roots) = &tls.trust_roots
        && !holds_certificates(roots.as_bytes())
    {
        return Err(Error::Pem {
            key: format!("{key}.trust_roots_file"),
            holds: "at least one certificate",
        });
    }
    Ok(tls)
}

/// Whether `pem` holds at least one certificate and every certificate in it
/// reads.
fn holds_certificates(pem: &[u8]) -> bool {
    let mut count = 0_usize;
    for certificate in CertificateDer::pem_slice_iter(pem) {
        if certificate.is_err() {
            return false;
        }
        count = count.saturating_add(1);
    }
    count > 0
}
