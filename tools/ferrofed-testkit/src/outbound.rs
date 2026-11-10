// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferrohealth-smart` outbound client a test reaches a harness server
//! through (#1815).
//!
//! It is built from the endpoint table the gateway reads, with its TLS
//! built by `ferrohealth-server`'s reader, as every identity, localization
//! and consent client of the gateway is built.
//!
//! A table here tries each call once, so a test that counts the requests a
//! harness server received counts the ones the client under test sent, and a
//! `5xx` reaches the client as the server answered it. No specification
//! governs the harness: our own design.

use ferrohealth_config::tls::TlsClientSection;
use ferrohealth_secret::secret::Secret;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::outbound::{OutboundClient, OutboundError};
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::endpoint::{EndpointSection, RetrySection};
use secrecy::SecretString;

/// The key every client here is built at.
const KEY: &str = "harness.endpoint";

/// The endpoint table of `url`: no credential, the platform's trust roots,
/// and one try per call.
#[must_use]
pub fn endpoint(url: &str) -> EndpointSection {
    EndpointSection {
        url: SecretUrl::new(url),
        retry: RetrySection {
            max_attempts: 1,
            ..RetrySection::default()
        },
        ..EndpointSection::default()
    }
}

/// The outbound client `endpoint` describes.
///
/// # Errors
///
/// The [`OutboundError`] of a table `ferrohealth-smart` refuses.
pub fn build(endpoint: &EndpointSection) -> Result<OutboundClient, OutboundError> {
    OutboundClient::build(endpoint, KEY, &ServerTls)
}

/// The outbound client of `url`, as [`endpoint`] describes it.
///
/// # Errors
///
/// The [`OutboundError`] of a URL `ferrohealth-smart` refuses.
pub fn client(url: &str) -> Result<OutboundClient, OutboundError> {
    build(&endpoint(url))
}

/// The outbound client of `url` sending HTTP Basic `user` and `password`.
///
/// # Errors
///
/// The [`OutboundError`] of a URL or a credential `ferrohealth-smart`
/// refuses.
pub fn basic(url: &str, user: &str, password: &str) -> Result<OutboundClient, OutboundError> {
    let mut table = endpoint(url);
    user.clone_into(&mut table.credentials.user);
    table.credentials.password = Some(SecretString::from(password));
    build(&table)
}

/// The outbound client of `url` presenting the TLS material `tls` names.
///
/// # Errors
///
/// The [`OutboundError`] of a URL or TLS material `ferrohealth-smart` or
/// the reader refuses.
pub fn with_tls(url: &str, tls: TlsClientSection) -> Result<OutboundClient, OutboundError> {
    let mut table = endpoint(url);
    table.tls = tls;
    build(&table)
}

/// The outbound client of `url` presenting `identity` and trusting `roots`.
///
/// `identity` is a client certificate chain and key in PEM, and `roots` PEM
/// trust roots beside the platform's, read as a table naming
/// `client_identity` and `trust_roots_file` reads them.
///
/// # Errors
///
/// The [`OutboundError`] of a URL or material `ferrohealth-smart` or the
/// reader refuses; a roots file that cannot be written is refused as
/// material that does not read.
pub fn with_material(
    url: &str,
    identity: &str,
    roots: &str,
) -> Result<OutboundClient, Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let roots_file = dir.path().join("roots.pem");
    std::fs::write(&roots_file, roots)?;
    let tls = TlsClientSection {
        client_identity: Some(Secret::new(identity.to_owned())),
        trust_roots_file: Some(roots_file),
        ..TlsClientSection::default()
    };
    Ok(with_tls(url, tls)?)
}
