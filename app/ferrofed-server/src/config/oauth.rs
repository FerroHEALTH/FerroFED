// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[<section>.oauth]`: the OAuth 2.0 client credentials grant (RFC 6749
//! §4.4) of `ferrohealth-smart`, the table every FerroHEALTH product reads
//! for an upstream it calls as itself.
//!
//! The gateway obtains its own token at the token endpoint, configured or
//! discovered from the issuer's metadata (RFC 8414 §3), authenticating with
//! a client secret in the Basic scheme (`client_secret_basic`, RFC 6749
//! §2.3.1), in the body (`client_secret_post`) or with a `private_key_jwt`
//! assertion (RFC 7523 §2.2) signed by the key in `private_key_file`, whose
//! public half the gateway publishes in its JWK Set. The token is cached
//! until shortly before it expires, and dropped and asked for once more when
//! the upstream answers `401`. The token endpoint is reached with the
//! platform's trust roots and those of `ca_file`, and with a client
//! certificate (`client_certificate_file`, `client_key_file`) under the RFC
//! 8705 §2 methods `tls_client_auth` and `self_signed_tls_client_auth`.
//!
//! ```toml
//! [credentials."cdr-a".oauth]
//! issuer = "https://as.cdr-a.example.org"
//! client_id = "ferrofed"
//! client_auth = "private_key_jwt"
//! private_key_file = "/run/secrets/ferrofed-client.pem"
//! assertion_alg = "ES256"
//! scope = "system/aql-*.s"
//! ```

use ferrohealth_smart::client::{ClientCredentials, Endpoint};
use ferrohealth_smart::config::OauthSection;
use secrecy::ExposeSecret as _;
use serde::{Deserialize, Serialize};

use crate::config::error::Error;

/// `[<section>.oauth]` as written: the table `ferrohealth-smart` reads.
///
/// Two tables compare equal when they render alike and hold the same inline
/// client secret, so a reload sees a changed secret.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Oauth(pub OauthSection);

impl PartialEq for Oauth {
    fn eq(&self, other: &Self) -> bool {
        // NOTE: no specification governs this: our own design; the table renders its
        // secret redacted, so the inline secrets are compared besides.
        let secret = |table: &Self| {
            table
                .0
                .client_secret
                .as_ref()
                .map(|secret| secret.expose_secret().to_owned())
        };
        serde_json::to_value(&self.0).ok() == serde_json::to_value(&other.0).ok()
            && secret(self) == secret(other)
    }
}

impl Eq for Oauth {}

impl Oauth {
    /// Resolves the grant the table at `key` describes, every secret and the
    /// assertion key read.
    ///
    /// # Errors
    /// Returns [`Error::Smart`] naming `key` for a table `ferrohealth-smart`
    /// refuses.
    pub fn resolve(&self, key: &str) -> Result<ClientCredentials, Error> {
        self.0.resolve().map_err(|source| Error::Smart {
            section: key.to_owned(),
            source: Box::new(source),
        })
    }
}

/// The token endpoint `grant` names outright, which the gateway's
/// cleartext policy holds; `None` for one discovered from the issuer, which
/// `ferrohealth-smart` holds to `https`.
#[must_use]
pub fn token_endpoint(grant: &ClientCredentials) -> Option<&url::Url> {
    match &grant.endpoint {
        Endpoint::Token(url) => Some(url),
        Endpoint::Issuer(_) => None,
    }
}
