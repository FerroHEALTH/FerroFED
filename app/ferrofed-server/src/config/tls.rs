// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The TLS material a table names for the service or the node it reaches.
//!
//! Every outbound table takes a `tls` sub-table, the family's
//! `ferrohealth_server::tls_client::TlsClientSection`: `client_identity`,
//! inline, or `client_identity_file`, the gateway's client certificate chain
//! and private key in PEM for mutual TLS, held as a secret;
//! `trust_roots_file`, a PEM bundle of trust roots beside the platform's;
//! and `min_version`, `"1.3"` unless lowered to `"1.2"`. `read` reads
//! every file once, at load. A node's material serves the node and its
//! authorization server alike, so a token bound to the certificate travels
//! only over connections that present it (RFC 8705 §3). No specification
//! governs the keys: our own design.

use std::sync::Arc;

use ferrohealth_config::tls::TlsClientSection;
use ferrohealth_server::tls_client::TlsClient;

use crate::config::error::Error;

/// TLS material, or a use of mutual TLS, the gateway cannot take.
///
/// No variant carries any part of a certificate or a key.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TlsFault {
    /// A grant uses mutual TLS, and its endpoint's section names no client
    /// identity to present.
    #[error(
        "{section} authenticates or binds its tokens by a TLS client certificate (RFC 8705), and its credentials section sets no tls.client_identity"
    )]
    WithoutClientIdentity {
        /// The grant's section.
        section: String,
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

/// Reads the TLS material `section` names for the table at `key`, such as
/// `xcpd.tls`, so a refusal names `xcpd.tls.client_identity`.
///
/// # Errors
///
/// [`Error::TlsClient`] for material that cannot be read or holds no PEM of
/// the kind its key takes.
pub(crate) fn read(key: &str, section: &TlsClientSection) -> Result<Arc<TlsClient>, Error> {
    Ok(Arc::new(TlsClient::read(key, section)?))
}
