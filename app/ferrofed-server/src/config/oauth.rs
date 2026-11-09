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

use ferrofed_engine::onward::grant::lane::Organisations;
use ferrohealth_smart::client::{ClientCredentials, Endpoint};
use ferrohealth_smart::config::credentials::OauthSection;
use ferrohealth_smart::config::onward::{OnwardLane, OnwardSection};
use ferrohealth_smart::kind::IssuerKind;
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
        // TODO(#1805): compare by the section's own PartialEq once smart 0.16.1 derives it.
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
    /// assertion key read, reaching its token endpoint over `presented`,
    /// the TLS material of a node the gateway already read, when one is
    /// given: the table then names no client certificate, key or trust roots
    /// of its own (RFC 8705 §2, §3).
    ///
    /// # Errors
    /// Returns [`Error::Smart`] naming `key` for a table `ferrohealth-smart`
    /// refuses.
    pub fn resolve(
        &self,
        key: &str,
        presented: Option<ferrohealth_smart::client::PresentedTls>,
    ) -> Result<ClientCredentials, Error> {
        match presented {
            Some(presented) => self.0.resolve_with_tls(presented),
            None => self.0.resolve(),
        }
        .map_err(|source| Error::Smart {
            section: key.to_owned(),
            source: Box::new(source),
        })
    }
}

/// A node's `[credentials."<endpoint id>".onward]` table, resolved: the
/// client of its issuer's onward grants and the node as their target.
#[derive(Debug, Clone)]
pub struct Onward {
    /// The client and the target.
    pub lane: OnwardLane,
    /// Whose tokens the issuer signs.
    pub kind: IssuerKind,
    /// Whether the lane carries another party's organisation.
    pub organisations: Organisations,
    /// The table as written, which a reload compares.
    pub written: OnwardSection,
}

impl Onward {
    /// Resolves `written`, the table at `key`, authenticating as `grant`,
    /// the lane's `oauth` client; the lane refuses another party's
    /// organisation.
    ///
    /// # Errors
    /// Returns [`Error::Smart`] naming `key` for a table `ferrohealth-smart`
    /// refuses: no issuer, kind or audience, a profile without
    /// `max_onward_lifetime`, or a target its grant cannot ask for.
    pub fn resolve(
        key: &str,
        written: &OnwardSection,
        grant: ClientCredentials,
    ) -> Result<Self, Error> {
        let refused = |source| Error::Smart {
            section: key.to_owned(),
            source: Box::new(source),
        };
        let lane = written.resolve(grant).map_err(refused)?;
        let kind = written
            .kind
            .ok_or(ferrohealth_smart::config::ConfigError::Missing {
                key: "credentials.onward.kind",
            })
            .map_err(refused)?;
        Ok(Self {
            lane,
            kind,
            organisations: Organisations::Refused,
            written: written.clone(),
        })
    }
}

/// The token endpoint `grant` names outright, which the gateway's
/// cleartext policy holds; `None` for one discovered from the issuer.
#[must_use]
pub fn token_endpoint(grant: &ClientCredentials) -> Option<&url::Url> {
    match &grant.endpoint {
        Endpoint::Token(url) => Some(url),
        Endpoint::Issuer(_) => None,
    }
}

/// The issuer `grant` discovers its token endpoint from, which the
/// gateway's cleartext policy holds as it holds a token endpoint:
/// `ferrohealth-smart` admits a loopback `http` issuer under any profile.
#[must_use]
pub fn issuer(grant: &ClientCredentials) -> Option<&str> {
    match &grant.endpoint {
        Endpoint::Issuer(issuer) => Some(issuer),
        Endpoint::Token(_) => None,
    }
}
