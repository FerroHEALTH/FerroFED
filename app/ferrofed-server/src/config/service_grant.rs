// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OAuth 2.0 client-credentials grant of an identity service: a PIX
//! Manager, the PDQm Supplier, the PMIR Registry and the mCSD directory.
//!
//! `[<service>.credentials.oauth2]` names the token endpoint, the client id,
//! the scope, and how the gateway authenticates: a client secret in the
//! Basic scheme, the method IHE IUA ITI-71 prescribes (§3.71.4.1.2.1), or in
//! the request body (RFC 6749 §2.3.1), each read inline or from its `_file`
//! sibling, or a JWT client assertion signed by the `[signing]` key (RFC
//! 7523 §2.2). The token is obtained with the client-credentials grant (RFC
//! 6749 §4.4), refreshed before it expires, and incorporated in each request
//! as a bearer token (IUA ITI-72 §3.72.4.2).
//!
//! ```toml
//! [pixm.manager.credentials.oauth2]
//! grant = "client_credentials"
//! token_endpoint = "https://mpi.example.org/auth/oauth2_token"
//! client_id = "ferrofed-pix-consumer"
//! client_auth = "client_secret_basic"
//! client_secret_file = "/run/secrets/pix-consumer"
//! scope = "*"
//! ```
//!
//! No specification governs the shape of the table: our own design.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::grant::client_credentials::{ClientCredentials, Recipient};
use ferrofed_engine::onward::keys::KeyRing;
use ferrofed_engine::onward::{Grant, Scope, SecretMethod, SystemClock};
use ferrofed_identity::dev::Profile;
use ferrofed_registry::secret::Secret;
use openehr_its::rest::client::{CredentialsProvider, ReqwestTransport};

use crate::config::error::Error;
use crate::config::grant::{self, GrantFault};
use crate::config::secrets::{resolve_credentials, secret};
use crate::config::settings::{Scheme, Settings, SigningSettings};
use crate::config::tls::TlsFault;
use crate::config::transport::{self, Encryption, ProtectedSite};
use crate::config::{ClientAuth, Credentials, GrantKind, OAuth2};

/// What an identity service's grant is resolved against: the deployment
/// profile, the `[signing]` key an assertion is signed with, and how long a
/// token request may take.
#[derive(Debug, Clone, Copy)]
pub struct ServiceContext<'a> {
    /// The deployment profile, which decides whether a token endpoint may be
    /// plain `http`.
    pub profile: Profile,
    /// The gateway's signing keys, when `[signing]` is set.
    pub signing: Option<&'a SigningSettings>,
    /// How long one token request may take: the per-node timeout.
    pub timeout: Duration,
}

impl<'a> ServiceContext<'a> {
    /// The context `settings` give, as far as they are resolved.
    #[must_use]
    pub fn of(settings: &'a Settings) -> Self {
        Self {
            profile: settings.profile,
            signing: settings.signing.as_ref(),
            timeout: settings.federation.budget.per_node(),
        }
    }
}

/// An identity service's client-credentials grant, resolved: every value
/// held to its rule and every file read.
///
/// `Debug` shows the grant, which names no secret.
pub struct ServiceGrant {
    grant: Grant,
    signer: Option<(Arc<KeyRing>, Duration)>,
    timeout: Duration,
    table: OAuth2,
    secret: Option<Secret>,
}

impl fmt::Debug for ServiceGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceGrant")
            .field("grant", &self.grant)
            .field("signed", &self.signer.is_some())
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl ServiceGrant {
    /// The grant the tokens are obtained with.
    #[must_use]
    pub fn grant(&self) -> &Grant {
        &self.grant
    }

    /// Whether `other` names the same table and the same secret, so a
    /// reload that reads it again changes nothing.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.table == other.table && self.secret == other.secret
    }

    /// The provider that obtains this grant's tokens for the service at the
    /// configuration key `service`, sending each token request over
    /// `transport`.
    #[must_use]
    pub fn provider(
        &self,
        service: &str,
        transport: ReqwestTransport,
    ) -> Arc<dyn CredentialsProvider> {
        let recipient = Recipient::Service(service.to_owned());
        let clock = Arc::new(SystemClock);
        match &self.signer {
            Some((keys, lifetime)) => Arc::new(ClientCredentials::new(
                recipient,
                self.grant.clone(),
                Arc::clone(keys),
                (*lifetime, self.timeout),
                transport,
                clock,
            )),
            None => Arc::new(ClientCredentials::by_secret(
                recipient,
                self.grant.clone(),
                self.timeout,
                transport,
                clock,
            )),
        }
    }

    /// How long one token request may take.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// The site of the token endpoint of the grant at `key`, sent the gateway's
/// client credentials.
#[must_use]
pub fn site(key: &str) -> ProtectedSite {
    ProtectedSite {
        url_key: format!("{key}.token_endpoint"),
        payload: format!("{key}, the gateway's client credentials"),
        requires: Encryption::Https,
    }
}

/// Returns the scheme an identity service's `credentials` at `section`
/// describe: a bearer token, basic credentials, or the client-credentials
/// grant of its `oauth2` table.
///
/// # Errors
///
/// [`TlsFault::OnService`] for TLS material in the section,
/// [`Error::Scheme`] for a grant beside another scheme,
/// [`Error::ServiceGrantNotHere`] for a Nuts or FAPI 2.0 grant, and every
/// refusal of [`resolve`] and of a bearer token or basic credentials.
pub(crate) fn resolve_service(
    section: &str,
    credentials: &Credentials,
    context: &ServiceContext<'_>,
) -> Result<Scheme, Error> {
    let Some(oauth2) = &credentials.oauth2 else {
        let scheme = resolve_credentials(section, credentials)?;
        if scheme.is_grant() {
            return Err(Error::ServiceGrantNotHere {
                section: section.to_owned(),
            });
        }
        return Ok(scheme);
    };
    if credentials.names_tls() {
        return Err(TlsFault::OnService {
            section: section.to_owned(),
        }
        .into());
    }
    if credentials.bearer_token.is_some()
        || credentials.bearer_token_file.is_some()
        || credentials.user.is_some()
        || credentials.password.is_some()
        || credentials.password_file.is_some()
        || credentials.fapi2.is_some()
    {
        return Err(Error::Scheme {
            section: section.to_owned(),
        });
    }
    resolve(&format!("{section}.oauth2"), oauth2, context)
        .map(|grant| Scheme::ServiceGrant(Box::new(grant)))
}

/// Returns the grant the `oauth2` table at `key` describes for an identity
/// service, every key set and each value held to its rule.
///
/// # Errors
///
/// [`Error::Missing`] for a key that must be set, [`GrantFault`] for a key a
/// service's grant does not take, [`Error::GrantWithoutSigning`] for an
/// assertion with no `[signing]` key, [`Error::ServiceScope`] for a scope
/// outside RFC 6749 §3.3, [`Error::Grant`] for an unusable token endpoint or
/// resource, and [`Error::Cleartext`] for a token endpoint that is not
/// `https` outside the development profile.
pub(crate) fn resolve(
    key: &str,
    oauth2: &OAuth2,
    context: &ServiceContext<'_>,
) -> Result<ServiceGrant, Error> {
    let missing = |name: &str| Error::Missing {
        key: format!("{key}.{name}"),
    };
    let node_only = |name: &str| -> Error {
        GrantFault::NodeOnly {
            key: format!("{key}.{name}"),
        }
        .into()
    };
    match oauth2.grant {
        Some(GrantKind::ClientCredentials) => {}
        // NOTE: IUA ITI-71 §3.71.4.1.2.1: an identity service is asked as the
        // gateway itself, so its grant is the client-credentials grant alone.
        Some(GrantKind::TokenExchange) => return Err(node_only("grant")),
        None => return Err(missing("grant")),
    }
    if oauth2.dpop_key_file.is_some() {
        return Err(node_only("dpop_key_file"));
    }
    if oauth2.tls_client_certificate_bound_access_tokens {
        return Err(node_only("tls_client_certificate_bound_access_tokens"));
    }
    let token_endpoint = oauth2
        .token_endpoint
        .as_ref()
        .ok_or_else(|| missing("token_endpoint"))?;
    // NOTE: RFC 6749 §2.3.1 requires TLS for a client password, and an assertion
    // or a token is no less a credential, so cleartext is for development alone.
    transport::protected_payload(context.profile, token_endpoint.expose(), site(key))?;
    if oauth2.client_id.is_empty() {
        return Err(missing("client_id"));
    }
    if oauth2.scope.trim().is_empty() {
        return Err(missing("scope"));
    }
    let scope = Scope::tokens(&oauth2.scope).map_err(|source| Error::ServiceScope {
        key: format!("{key}.scope"),
        source,
    })?;
    let refused = |source| Error::Grant {
        section: key.to_owned(),
        source,
    };
    let secret = secret::<Secret>(
        &format!("{key}.client_secret"),
        oauth2.client_secret.as_ref(),
        oauth2.client_secret_file.as_deref(),
    )?;
    let mut grant = Grant::new(token_endpoint, oauth2.client_id.clone(), scope).map_err(refused)?;
    let method = match oauth2.client_auth.ok_or_else(|| missing("client_auth"))? {
        ClientAuth::ClientSecretBasic => Some(SecretMethod::Basic),
        ClientAuth::ClientSecretPost => Some(SecretMethod::Post),
        ClientAuth::PrivateKeyJwt => None,
        ClientAuth::Tls | ClientAuth::SelfSignedTls => return Err(node_only("client_auth")),
    };
    let signer = match (method, secret.clone()) {
        (Some(method), Some(secret)) => {
            grant = grant.with_client_secret(method, secret.to_secret_string());
            None
        }
        (Some(_), None) => return Err(missing("client_secret")),
        (None, Some(_)) => {
            return Err(GrantFault::SecretUnused {
                key: format!("{key}.client_secret"),
            }
            .into());
        }
        (None, None) => {
            let signing = context.signing.ok_or_else(|| Error::GrantWithoutSigning {
                section: key.to_owned(),
            })?;
            Some((Arc::clone(&signing.keys), signing.assertion_lifetime))
        }
    };
    grant = grant::with_assertion_audience(key, oauth2, grant)?;
    if let Some(resource) = &oauth2.resource {
        grant = grant.with_resource(resource).map_err(refused)?;
    }
    if let Some(audience) = &oauth2.audience {
        grant = grant.with_audience(audience.clone()).map_err(refused)?;
    }
    Ok(ServiceGrant {
        grant,
        signer,
        timeout: context.timeout,
        table: oauth2.clone(),
        secret,
    })
}
