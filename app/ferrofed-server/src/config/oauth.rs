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
//! [credentials."cdr-a".credentials.oauth]
//! issuer = "https://as.cdr-a.example.org"
//! client_id = "ferrofed"
//! client_auth = "private_key_jwt"
//! private_key_file = "/run/secrets/ferrofed-client.pem"
//! assertion_alg = "ES256"
//! scope = "system/aql-*.s"
//! ```

use ferrofed_engine::onward::grant::lane::Organisations;
use ferrohealth_smart::client::{ClientCredentials, Endpoint, PresentedTls};
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::credentials::OauthSection;
use ferrohealth_smart::config::onward::{OnwardLane, OnwardSection};
use ferrohealth_smart::kind::IssuerKind;

use crate::config::error::Error;

/// Resolves the grant `table`, the table at `key`, every secret read.
///
/// Its own `tls` is read by `ferrohealth-smart`'s reader. Given
/// `presented`, the TLS of the node the grant serves, a table whose
/// `tls` names no material presents that connection's trust roots and
/// client certificate instead (RFC 8705 §2, §3).
///
/// # Errors
/// Returns [`Error::Smart`] naming `key` for a table `ferrohealth-smart`
/// refuses, and for one that names TLS material of its own beside the
/// node's.
pub fn resolve(
    key: &str,
    table: &OauthSection,
    presented: Option<&PresentedTls>,
) -> Result<ClientCredentials, Error> {
    match presented {
        // NOTE: RFC 8705 §2: the grant presents the node section's identity, so a TLS
        // table of its own beside it would leave two identities, and is refused.
        Some(_) if table.tls.names_material() => {
            Err(ferrohealth_smart::config::ConfigError::Exclusive {
                left: "credentials.credentials.oauth.tls",
                right: "credentials.tls",
            })
        }
        Some(connection) => table.resolve_sharing_tls(&ServerTls, connection),
        None => table.resolve_with_tls(&ServerTls),
    }
    .map_err(|source| Error::Smart {
        section: key.to_owned(),
        source: Box::new(source),
    })
}

/// A node's `[credentials."<endpoint id>".credentials.onward]` table,
/// resolved: the
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
