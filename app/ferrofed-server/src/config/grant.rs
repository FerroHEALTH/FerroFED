// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FAPI 2.0 grant, `[credentials."<id>".fapi2]`, and the audience of an
//! `oauth2` grant's client assertion.
//!
//! A `fapi2` section makes the gateway authenticate to the node's
//! authorization server under the FAPI 2.0 Security Profile, as the
//! BgZ/eOverdracht track of Annex B §B.4a does: the server is discovered
//! from its issuer identifier, the client authenticates with an ES256
//! `private_key_jwt` assertion naming the issuer, the token is bound with
//! `DPoP`, and the request carries the configured RFC 9396
//! `authorization_details`.
//!
//! ```toml
//! [credentials."cdr-e".fapi2]
//! issuer = "https://as.cdr-e.example.org"
//! grant = "client_credentials"
//! client_id = "urn:oid:2.999.3.3.12345678"
//! client_key_file = "/run/secrets/fapi2-client.pem"
//! dpop_key_file = "/run/secrets/fapi2-dpop.pem"
//! scope = "system/aql-*.s"
//! authorization_details = '''[{"type": "nl-gis-v1",
//!   "purpose_of_use": "http://terminology.hl7.org/CodeSystem/v3-ActReason|TREAT",
//!   "locations": ["https://cdr-e.example.org/openehr"],
//!   "locations_organization_id": "urn:oid:2.999.3.3.87654321"}]'''
//! ```
//!
//! No specification governs the shape of the table: our own design.

use std::path::PathBuf;
use std::sync::Arc;

use ferrofed_engine::onward::authorization_details::{
    AuthorizationDetails, AuthorizationDetailsError,
};
use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::fapi2::{Fapi2Grant, Fapi2GrantError};
use ferrofed_engine::onward::keys::{KeyError, SigningKey};
use ferrofed_engine::onward::{Grant, Scope};
use ferrofed_registry::secret::{Secret, SecretUrl};
use oauth_server_metadata::{InvalidIssuer, Issuer};
use serde::Deserialize;

use crate::config::OAuth2;
use crate::config::error::Error;
use crate::config::secrets::secret;

/// What an `oauth2` grant's client assertions name as their `aud` (RFC 7523
/// §3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AssertionAudience {
    /// The token endpoint's URL.
    #[default]
    TokenEndpoint,
    /// The authorization server's issuer identifier, which `issuer` names:
    /// the one value the FAPI 2.0 Security Profile admits (§5.3.2.1).
    Issuer,
}

/// The FAPI 2.0 grant at one node's authorization server (Annex B §B.4a;
/// FAPI 2.0 Security Profile).
///
/// No field has a default; `scope` and `authorization_details` are each
/// optional, and at least one is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fapi2 {
    /// The authorization server's issuer identifier (RFC 8414 §2), its
    /// metadata read from the well-known URL under it; `https` outside the
    /// development profile.
    pub issuer: Option<SecretUrl>,
    /// The grant: `client_credentials` (RFC 6749 §4.4) or `token_exchange`
    /// (RFC 8693), a token per verified caller. `authorization_code` is
    /// refused: it needs a user agent.
    pub grant: Option<Fapi2GrantKind>,
    /// The client the authorization server registered the gateway as, the
    /// `iss` and `sub` of every client assertion; on the Annex B §B.4a track,
    /// the organisation's URA-based identifier.
    pub client_id: String,
    /// A file holding the key every client assertion is signed with, a P-256
    /// private key in PKCS#8 PEM, read at boot; its public half is published
    /// in the gateway's JWK Set.
    pub client_key_file: Option<PathBuf>,
    /// A file holding the key the tokens are bound to with `DPoP` (RFC 9449),
    /// a P-256 private key in PKCS#8 PEM, read at boot. Required.
    pub dpop_key_file: Option<PathBuf>,
    /// The scope every token is requested with, each a SMART on openEHR
    /// resource scope of the `system` compartment.
    pub scope: Option<String>,
    /// The `authorization_details` every token request carries, a JSON array
    /// of RFC 9396 §2 objects, sent as written.
    pub authorization_details: Option<String>,
    /// The target service the token is for (RFC 8707 §2); required under
    /// `token_exchange`.
    pub resource: Option<String>,
    /// The audience the token is asked for, when the server takes one.
    pub audience: Option<String>,
}

/// The grant a `fapi2` section names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Fapi2GrantKind {
    /// The client-credentials grant (RFC 6749 §4.4), which the profile admits
    /// under its general requirements (§5.3.2.1 Note 2).
    ClientCredentials,
    /// Token exchange (RFC 8693) per verified caller; the gateway's own
    /// requests use the client-credentials grant.
    TokenExchange,
    /// The authorization code grant, named so its refusal can say why: it
    /// needs a user agent to redirect, with pushed authorization requests
    /// and PKCE (§5.3.2.2, §5.3.3.2), which a server-to-server gateway has
    /// none of.
    AuthorizationCode,
}

/// A grant section the gateway cannot use.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GrantFault {
    /// `oauth2.issuer` is set while the assertions name the token endpoint.
    #[error("{section}.issuer applies only with assertion_audience = \"issuer\"; remove it")]
    IssuerUnused {
        /// The `oauth2` section.
        section: String,
    },
    /// A value is no issuer identifier (RFC 8414 §2).
    #[error("{key} is not an issuer identifier (RFC 8414 §2)")]
    Issuer {
        /// The key that holds it.
        key: String,
        /// Why it is refused.
        #[source]
        source: InvalidIssuer,
    },
    /// The authorization code grant is named, which needs a user agent.
    #[error(
        "{key} = \"authorization_code\" needs a user agent to redirect, with pushed authorization requests and PKCE (FAPI 2.0 Security Profile §5.3.2.2, §5.3.3.2), which a server-to-server gateway has none of; use client_credentials or token_exchange"
    )]
    UserAgentFlow {
        /// The key that names it.
        key: String,
    },
    /// The client key is no P-256 private key in PKCS#8 PEM.
    #[error(
        "{key} is not a P-256 private key in PKCS#8 PEM, the key ES256 signs with (FAPI 2.0 §5.4.1)"
    )]
    ClientKey {
        /// The key the file was named by.
        key: String,
        /// Why it is refused; it quotes no part of the key.
        #[source]
        source: KeyError,
    },
    /// The `authorization_details` are no RFC 9396 §2 array.
    #[error("{key} is not an RFC 9396 §2 authorization_details array")]
    AuthorizationDetails {
        /// The key that holds it.
        key: String,
        /// Why it is refused.
        #[source]
        source: AuthorizationDetailsError,
    },
    /// The FAPI 2.0 grant cannot be built from its section.
    #[error("{section} is not a usable FAPI 2.0 grant")]
    Fapi2 {
        /// The section.
        section: String,
        /// What the grant refused.
        #[source]
        source: Fapi2GrantError,
    },
    /// A `fapi2` section is configured without `[signing]`, whose JWK Set
    /// publishes the grant's client key.
    #[error(
        "{section} needs [signing]: the gateway's JWK Set publishes the grant's client key, and [signing] sets the assertion lifetime"
    )]
    WithoutSigning {
        /// The section.
        section: String,
    },
}

/// Returns `grant` with the audience `oauth2` names for its assertions:
/// the token endpoint, or the issuer `oauth2.issuer` names (RFC 7523 §3;
/// FAPI 2.0 Security Profile §5.3.2.1).
pub(super) fn with_assertion_audience(
    section: &str,
    oauth2: &OAuth2,
    grant: Grant,
) -> Result<Grant, Error> {
    match (oauth2.assertion_audience, &oauth2.issuer) {
        (AssertionAudience::TokenEndpoint, None) => Ok(grant),
        (AssertionAudience::TokenEndpoint, Some(_)) => Err(GrantFault::IssuerUnused {
            section: section.to_owned(),
        }
        .into()),
        (AssertionAudience::Issuer, None) => Err(Error::Missing {
            key: format!("{section}.issuer"),
        }),
        (AssertionAudience::Issuer, Some(issuer)) => {
            let issuer = Issuer::parse(issuer).map_err(|source| GrantFault::Issuer {
                key: format!("{section}.issuer"),
                source,
            })?;
            Ok(grant.with_issuer_audience(issuer))
        }
    }
}

/// Returns the FAPI 2.0 grant the `fapi2` table at `section` describes,
/// every key set and each value held to its rule, every key file read.
pub(super) fn resolve_fapi2(section: &str, fapi2: &Fapi2) -> Result<Fapi2Grant, Error> {
    let missing = |name: &str| Error::Missing {
        key: format!("{section}.{name}"),
    };
    let exchange = match fapi2.grant {
        Some(Fapi2GrantKind::ClientCredentials) => false,
        Some(Fapi2GrantKind::TokenExchange) => true,
        Some(Fapi2GrantKind::AuthorizationCode) => {
            return Err(GrantFault::UserAgentFlow {
                key: format!("{section}.grant"),
            }
            .into());
        }
        None => return Err(missing("grant")),
    };
    let issuer = fapi2.issuer.as_ref().ok_or_else(|| missing("issuer"))?;
    let issuer = Issuer::parse(issuer.expose()).map_err(|source| GrantFault::Issuer {
        key: format!("{section}.issuer"),
        source,
    })?;
    if fapi2.client_id.is_empty() {
        return Err(missing("client_id"));
    }
    let client_key = client_key(section, fapi2)?;
    let dpop = dpop_key(section, fapi2)?;
    let scope = fapi2
        .scope
        .as_deref()
        .map(|scope| {
            Scope::parse(scope).map_err(|source| Error::Scope {
                key: format!("{section}.scope"),
                source,
            })
        })
        .transpose()?;
    let details = fapi2
        .authorization_details
        .as_deref()
        .map(|text| {
            AuthorizationDetails::parse(text).map_err(|source| GrantFault::AuthorizationDetails {
                key: format!("{section}.authorization_details"),
                source,
            })
        })
        .transpose()?;
    let refused = |source| GrantFault::Fapi2 {
        section: section.to_owned(),
        source,
    };
    let mut grant = Fapi2Grant::new(
        issuer,
        fapi2.client_id.clone(),
        (client_key, Arc::new(dpop)),
        (scope, details),
    )
    .map_err(refused)?;
    if let Some(resource) = &fapi2.resource {
        grant = grant.with_resource(resource).map_err(refused)?;
    }
    if let Some(audience) = &fapi2.audience {
        grant = grant.with_audience(audience.clone()).map_err(refused)?;
    }
    if exchange {
        // NOTE: RFC 8707 §2, RFC 8693 §2.1: an exchanged token names the node it
        // is for, so a token-exchange grant without a resource is refused.
        if fapi2.resource.is_none() {
            return Err(missing("resource"));
        }
        grant = grant.with_token_exchange().map_err(refused)?;
    }
    Ok(grant)
}

/// Reads the client key `client_key_file` names, a P-256 key.
fn client_key(section: &str, fapi2: &Fapi2) -> Result<SigningKey, Error> {
    let key = format!("{section}.client_key_file");
    let path = fapi2
        .client_key_file
        .as_deref()
        .ok_or_else(|| Error::Missing { key: key.clone() })?;
    let pem = secret::<Secret>(&format!("{section}.client_key"), None, Some(path))?
        .ok_or_else(|| Error::Missing { key: key.clone() })?;
    SigningKey::from_p256_pem(&pem.to_secret_string())
        .map_err(|source| GrantFault::ClientKey { key, source }.into())
}

/// Reads the `DPoP` key `dpop_key_file` names.
fn dpop_key(section: &str, fapi2: &Fapi2) -> Result<Prover, Error> {
    let key = format!("{section}.dpop_key_file");
    let path = fapi2
        .dpop_key_file
        .as_deref()
        .ok_or_else(|| Error::Missing { key: key.clone() })?;
    let pem = secret::<Secret>(&format!("{section}.dpop_key"), None, Some(path))?
        .ok_or_else(|| Error::Missing { key: key.clone() })?;
    Prover::from_pem(&pem.to_secret_string()).map_err(|source| Error::DpopKey { key, source })
}
