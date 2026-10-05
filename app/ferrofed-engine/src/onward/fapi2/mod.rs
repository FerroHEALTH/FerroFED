// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FAPI 2.0 grant: an onward token from an authorization server that
//! follows the FAPI 2.0 Security Profile, as the BgZ/eOverdracht track of
//! Annex B §B.4a does (§13.3, §13.4).
//!
//! For an endpoint configured with a [`Fapi2Grant`], the gateway discovers
//! the authorization server from its issuer identifier ([`metadata`]), then
//! obtains a token with the client-credentials grant or, per verified
//! caller, token exchange, through the same token request as the `oauth2`
//! grant ([`crate::onward::token`]), with these differences, each required
//! by the profile:
//!
//! - the client authenticates with `private_key_jwt` (FAPI 2.0 §5.3.2.1,
//!   §5.3.3.1), an assertion whose `aud` is the issuer identifier as one
//!   string (§5.3.3.1), signed `ES256` (§5.4.1 admits `PS256`, `ES256` and
//!   `EdDSA`) with a P-256 key of the grant's own, which the gateway publishes
//!   in its JWK Set (§5.4.2), with the previous key beside it while the key
//!   is rotated ([`Fapi2Grant::with_previous_client_key`]);
//! - every token is sender-constrained with `DPoP` (§5.3.2.1, §5.3.3.1;
//!   RFC 9449), proven with a P-256 key, so a grant without one cannot be
//!   built; a nonce the server demands is answered (§5.3.3.1, RFC 9449 §8);
//! - the request carries the configured RFC 9396 `authorization_details`,
//!   whose types the metadata must list, and the token response must state
//!   the details it granted (RFC 9396 §7). FAPI 2.0 §5.3.2.2 Note 5
//!   recommends them where `scope` is not expressive enough; the Annex B
//!   §B.4a.3 track carries its healthcare attributes in them.
//!
//! The metadata is read once, at the first token request, and kept for the
//! life of the provider; a discovery that fails is not kept, so the next
//! request tries again. Tokens are cached and dropped on a `401` as the
//! `oauth2` grant's are ([`crate::onward::provider`],
//! [`crate::onward::exchange`]).
//!
//! What the profile asks of an authorization-endpoint flow is out of a
//! server-to-server gateway's reach: the authorization code flow needs a
//! user agent to redirect, with pushed authorization requests and PKCE
//! (FAPI 2.0 §5.3.2.2, §5.3.3.2; RFC 9126). The client-credentials grant
//! the gateway uses is one the profile admits under its general
//! requirements (§5.3.2.1 Note 2), and the authorization code grant is not
//! built.

pub mod metadata;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::SecretUrl;
use jsonwebtoken::Algorithm;
use jsonwebtoken::jwk::JwkSet;
use oauth_server_metadata::Issuer;
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider, Transport};
use url::Url;

use crate::dispatch::SharedCredentials;
use crate::hygiene::Withheld;
use crate::onward::authorization_details::AuthorizationDetails;
use crate::onward::conveyance::Conveyance;
use crate::onward::dpop::Prover;
use crate::onward::exchange::{Exchange, OnBehalf};
use crate::onward::fapi2::metadata::DiscoveryError;
use crate::onward::keys::{KeyError, KeyRing, SigningKey};
use crate::onward::provider::ClientCredentials;
use crate::onward::{Clock, Grant, GrantError, GrantKind, Scope, SystemClock};

/// The JWS algorithm every assertion and proof of the grant is signed with.
///
/// It is one of the three FAPI 2.0 Security Profile §5.4.1 admits, and the
/// one a P-256 key signs with (RFC 7518 §3.4).
pub const ALGORITHM: Algorithm = Algorithm::ES256;

/// [`ALGORITHM`] as RFC 7518 §3.1 names it, as metadata lists it.
pub const ALGORITHM_NAME: &str = "ES256";

/// What a FAPI 2.0 grant at one node's authorization server asks for, as
/// whom, and the keys it signs with.
///
/// `Debug` shows the issuer, the client, the request and the keys' `kid`
/// and thumbprint, never a key.
#[derive(Debug, Clone)]
pub struct Fapi2Grant {
    issuer: Issuer,
    client_id: String,
    kind: GrantKind,
    scope: Option<Scope>,
    authorization_details: Option<AuthorizationDetails>,
    resource: Option<String>,
    audience: Option<String>,
    client_key: Arc<KeyRing>,
    dpop: Arc<Prover>,
}

/// A FAPI 2.0 grant that cannot be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Fapi2GrantError {
    /// The `client_id` is empty.
    #[error("the client_id is empty")]
    ClientId,
    /// The client key does not sign `ES256` (FAPI 2.0 Security Profile
    /// §5.4.1).
    #[error("the client key does not sign ES256, which FAPI 2.0 §5.4.1 requires of it")]
    ClientKey,
    /// The previous client key does not sign `ES256`, so no assertion the
    /// grant signed with it can be one to verify (FAPI 2.0 Security Profile
    /// §5.4.1).
    #[error("the previous client key does not sign ES256, which FAPI 2.0 §5.4.1 requires of it")]
    PreviousClientKey,
    /// The client key cannot be held: the previous key is the current one.
    #[error("the client key cannot be held")]
    Keys(#[source] KeyError),
    /// The `DPoP` key does not sign `ES256` (FAPI 2.0 Security Profile
    /// §5.4.1).
    #[error("the DPoP key does not sign ES256, which FAPI 2.0 §5.4.1 requires of it")]
    DpopKey,
    /// The grant asks for neither a scope nor `authorization_details`, so
    /// the authorization server would choose what the token may do (FAPI
    /// 2.0 Security Profile §5.3.3.1: request the least privilege).
    #[error("the grant asks for neither a scope nor authorization_details")]
    Unrequested,
    /// A token exchange names no resource (RFC 8707 §2).
    #[error("a token exchange grant names no resource")]
    Untargeted,
    /// The grant's `resource` or `audience` is refused as the `oauth2`
    /// grant refuses it.
    #[error("the grant's resource or audience is not usable")]
    Target(#[source] GrantError),
}

impl Fapi2Grant {
    /// A grant at the authorization server `issuer` for the client
    /// `client_id`, authenticating with `client_key`, binding its tokens to
    /// `dpop`'s key, and asking for `scope`, `authorization_details`, or
    /// both.
    ///
    /// # Errors
    ///
    /// Returns [`Fapi2GrantError::ClientId`] for an empty `client_id`,
    /// [`Fapi2GrantError::ClientKey`] and [`Fapi2GrantError::DpopKey`] for a
    /// key that does not sign `ES256`, and [`Fapi2GrantError::Unrequested`]
    /// when it asks for neither a scope nor details.
    pub fn new(
        issuer: Issuer,
        client_id: impl Into<String>,
        (client_key, dpop): (SigningKey, Arc<Prover>),
        (scope, authorization_details): (Option<Scope>, Option<AuthorizationDetails>),
    ) -> Result<Self, Fapi2GrantError> {
        let client_id = client_id.into();
        if client_id.is_empty() {
            return Err(Fapi2GrantError::ClientId);
        }
        if client_key.algorithm() != ALGORITHM {
            return Err(Fapi2GrantError::ClientKey);
        }
        if dpop.algorithm() != ALGORITHM {
            return Err(Fapi2GrantError::DpopKey);
        }
        if scope.is_none() && authorization_details.is_none() {
            return Err(Fapi2GrantError::Unrequested);
        }
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let client_key =
            KeyRing::new(client_key, None, Duration::ZERO, clock).map_err(Fapi2GrantError::Keys)?;
        Ok(Self {
            issuer,
            client_id,
            kind: GrantKind::ClientCredentials,
            scope,
            authorization_details,
            resource: None,
            audience: None,
            client_key: Arc::new(client_key),
            dpop,
        })
    }

    /// This grant, publishing `previous` beside its client key while it is
    /// rotated: until the grant is built again without it.
    ///
    /// Only the current client key signs. An authorization server that
    /// fetches the gateway's JWK Set during the rotation still finds the
    /// key an assertion signed before it named (FAPI 2.0 Security Profile
    /// §5.4.2). No specification governs the rotation: our own design.
    ///
    /// # Errors
    ///
    /// Returns [`Fapi2GrantError::PreviousClientKey`] for a key that does
    /// not sign `ES256`, and [`Fapi2GrantError::Keys`] when `previous` is the
    /// current client key.
    pub fn with_previous_client_key(
        mut self,
        previous: SigningKey,
    ) -> Result<Self, Fapi2GrantError> {
        if previous.algorithm() != ALGORITHM {
            return Err(Fapi2GrantError::PreviousClientKey);
        }
        let ring = KeyRing::until_removed(
            self.client_key.current().clone(),
            previous,
            Arc::new(SystemClock),
        )
        .map_err(Fapi2GrantError::Keys)?;
        self.client_key = Arc::new(ring);
        Ok(self)
    }

    /// This grant, naming `resource` as the target service (RFC 8707 §2).
    ///
    /// # Errors
    ///
    /// Returns [`Fapi2GrantError::Target`] for a value that is no absolute
    /// URI or carries a fragment.
    pub fn with_resource(mut self, resource: &str) -> Result<Self, Fapi2GrantError> {
        let url = Url::parse(resource)
            .map_err(|source| Fapi2GrantError::Target(GrantError::Resource(Some(source))))?;
        if url.fragment().is_some() {
            return Err(Fapi2GrantError::Target(GrantError::Resource(None)));
        }
        self.resource = Some(resource.to_owned());
        Ok(self)
    }

    /// This grant, naming `audience` as the audience it asks the token for.
    ///
    /// # Errors
    ///
    /// Returns [`Fapi2GrantError::Target`] for an empty value.
    pub fn with_audience(mut self, audience: impl Into<String>) -> Result<Self, Fapi2GrantError> {
        let audience = audience.into();
        if audience.is_empty() {
            return Err(Fapi2GrantError::Target(GrantError::Audience));
        }
        self.audience = Some(audience);
        Ok(self)
    }

    /// This grant, obtaining a token per verified caller by token exchange
    /// (RFC 8693 §2); the gateway's own requests use the client-credentials
    /// grant.
    ///
    /// # Errors
    ///
    /// Returns [`Fapi2GrantError::Untargeted`] for a grant that names no
    /// resource yet.
    pub fn with_token_exchange(mut self) -> Result<Self, Fapi2GrantError> {
        if self.resource.is_none() {
            return Err(Fapi2GrantError::Untargeted);
        }
        self.kind = GrantKind::TokenExchange;
        Ok(self)
    }

    /// The authorization server's issuer identifier.
    #[must_use]
    pub fn issuer(&self) -> &Issuer {
        &self.issuer
    }

    /// The `client_id`, the `iss` and `sub` of every assertion.
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// How the grant obtains its tokens.
    #[must_use]
    pub fn kind(&self) -> GrantKind {
        self.kind
    }

    /// The `authorization_details` every token request asks for, when the
    /// grant asks for any.
    #[must_use]
    pub fn authorization_details(&self) -> Option<&AuthorizationDetails> {
        self.authorization_details.as_ref()
    }

    /// The key every client assertion is signed with, whose public half the
    /// gateway publishes in its JWK Set (FAPI 2.0 Security Profile §5.4.2).
    #[must_use]
    pub fn client_key(&self) -> &SigningKey {
        self.client_key.current()
    }

    /// The previous client key while the grant is rotated, published and
    /// never signing.
    #[must_use]
    pub fn previous_client_key(&self) -> Option<&SigningKey> {
        self.client_key.previous()
    }

    /// The public halves the gateway publishes for the grant in its JWK
    /// Set: the client key, then the previous one while the grant is
    /// rotated (RFC 7517 §5; FAPI 2.0 Security Profile §5.4.2).
    #[must_use]
    pub fn published_client_keys(&self) -> JwkSet {
        self.client_key.published()
    }

    /// The key the tokens are bound to, which proves every request to the
    /// node.
    #[must_use]
    pub fn dpop(&self) -> &Arc<Prover> {
        &self.dpop
    }

    /// The `oauth2` grant this grant is at `token_endpoint`, the one its
    /// metadata names.
    fn at(&self, token_endpoint: &Url) -> Result<Grant, Fapi2Error> {
        let mut grant = Grant::at(
            &SecretUrl::new(token_endpoint.as_str().to_owned()),
            self.client_id.clone(),
            self.scope.clone(),
        )
        .map_err(Fapi2Error::TokenEndpoint)?
        .with_issuer_audience(self.issuer.clone())
        .with_dpop(Arc::clone(&self.dpop));
        if let Some(details) = &self.authorization_details {
            grant = grant.with_authorization_details(details.clone());
        }
        if let Some(resource) = &self.resource {
            grant = grant
                .with_resource(resource)
                .map_err(Fapi2Error::TokenEndpoint)?;
        }
        if let Some(audience) = &self.audience {
            grant = grant
                .with_audience(audience.clone())
                .map_err(Fapi2Error::TokenEndpoint)?;
        }
        if self.kind == GrantKind::TokenExchange {
            grant = grant.with_token_exchange();
        }
        Ok(grant)
    }
}

/// Why the FAPI 2.0 grant has no token endpoint to ask, so nothing was sent
/// to it.
///
/// No variant carries a credential or a token.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Fapi2Error {
    /// The authorization server's metadata gave no token endpoint the grant
    /// may use.
    #[error("the FAPI 2.0 authorization server could not be discovered")]
    Discovery(#[source] DiscoveryError),
    /// The token endpoint the metadata names is one the grant refuses.
    #[error("the token endpoint the FAPI 2.0 metadata names is not usable")]
    TokenEndpoint(#[source] GrantError),
}

/// What both providers of one endpoint's grant share: the grant, its
/// timing, and the engine every request goes through.
struct Binding<T> {
    endpoint: EndpointId,
    grant: Fapi2Grant,
    lifetime: Duration,
    timeout: Duration,
    transport: T,
    clock: Arc<dyn Clock>,
}

impl<T: Transport> Binding<T> {
    /// The `oauth2` grant at the token endpoint the issuer's metadata names.
    async fn discovered(&self) -> Result<Grant, Fapi2Error> {
        let token_endpoint = metadata::discover(&self.grant, &self.transport, self.timeout)
            .await
            .map_err(Fapi2Error::Discovery)?;
        let grant = self.grant.at(&token_endpoint)?;
        tracing::debug!(
            endpoint = %self.endpoint,
            "the FAPI 2.0 authorization server was discovered"
        );
        Ok(grant)
    }
}

/// The FAPI 2.0 client-credentials grant of one endpoint, as a
/// [`CredentialsProvider`] for its node client.
///
/// `Debug` names the endpoint and the grant, never a token.
pub struct Fapi2Credentials<T> {
    binding: Binding<T>,
    provider: tokio::sync::OnceCell<ClientCredentials<T>>,
}

impl<T: Transport + Clone + 'static> Fapi2Credentials<T> {
    /// The provider of `endpoint`'s `grant`, signing each assertion valid
    /// for `lifetime` and waiting at most `timeout` for the metadata and for
    /// each token request, over `transport`.
    #[must_use]
    pub fn new(
        endpoint: EndpointId,
        grant: Fapi2Grant,
        (lifetime, timeout): (Duration, Duration),
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            binding: Binding {
                endpoint,
                grant,
                lifetime,
                timeout,
                transport,
                clock,
            },
            provider: tokio::sync::OnceCell::new(),
        }
    }

    /// The client-credentials provider at the discovered token endpoint.
    async fn provider(&self) -> Result<&ClientCredentials<T>, Fapi2Error> {
        self.provider
            .get_or_try_init(|| async {
                let binding = &self.binding;
                let grant = binding.discovered().await?.as_client_credentials();
                Ok(ClientCredentials::new(
                    binding.endpoint.clone(),
                    grant,
                    Arc::clone(&binding.grant.client_key),
                    (binding.lifetime, binding.timeout),
                    binding.transport.clone(),
                    Arc::clone(&binding.clock),
                ))
            })
            .await
    }
}

#[async_trait::async_trait]
impl<T: Transport + Clone + 'static> CredentialsProvider for Fapi2Credentials<T> {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        self.provider()
            .await
            .map_err(CredentialsError::new)?
            .credentials()
            .await
    }

    fn refused(&self) {
        if let Some(provider) = self.provider.get() {
            provider.refused();
        }
    }
}

impl<T> fmt::Debug for Fapi2Credentials<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fapi2Credentials")
            .field("endpoint", &self.binding.endpoint)
            .field("grant", &self.binding.grant)
            .finish_non_exhaustive()
    }
}

/// The FAPI 2.0 token-exchange grant of one endpoint: a token per verified
/// caller, and the client-credentials grant for the gateway's own requests,
/// both at the discovered token endpoint.
///
/// `Debug` names the endpoint and the grant, never a token.
pub struct Fapi2Exchange<T>(Arc<ExchangeInner<T>>);

/// What every call of one endpoint's FAPI 2.0 exchange shares.
struct ExchangeInner<T> {
    binding: Binding<T>,
    exchange: tokio::sync::OnceCell<Exchange<T>>,
}

impl<T: Transport + Clone + 'static> Fapi2Exchange<T> {
    /// The exchange of `endpoint`'s `grant`, as [`Fapi2Credentials::new`]
    /// takes its arguments.
    #[must_use]
    pub fn new(
        endpoint: EndpointId,
        grant: Fapi2Grant,
        (lifetime, timeout): (Duration, Duration),
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self(Arc::new(ExchangeInner {
            binding: Binding {
                endpoint,
                grant,
                lifetime,
                timeout,
                transport,
                clock,
            },
            exchange: tokio::sync::OnceCell::new(),
        }))
    }
}

impl<T: Transport + Clone + 'static> ExchangeInner<T> {
    /// The token exchange at the discovered token endpoint.
    async fn exchange(&self) -> Result<&Exchange<T>, Fapi2Error> {
        self.exchange
            .get_or_try_init(|| async {
                let binding = &self.binding;
                let grant = binding.discovered().await?;
                Ok(Exchange::new(
                    binding.endpoint.clone(),
                    grant,
                    Arc::clone(&binding.grant.client_key),
                    (binding.lifetime, binding.timeout),
                    binding.transport.clone(),
                    Arc::clone(&binding.clock),
                ))
            })
            .await
    }
}

impl<T: Transport + Clone + 'static> OnBehalf for Fapi2Exchange<T> {
    fn provider(&self, conveyance: &Conveyance, withheld: &Arc<Withheld>) -> SharedCredentials {
        Arc::new(OnBehalfOf {
            inner: Arc::clone(&self.0),
            conveyance: conveyance.clone(),
            withheld: Arc::clone(withheld),
        })
    }
}

impl<T> fmt::Debug for Fapi2Exchange<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fapi2Exchange")
            .field("endpoint", &self.0.binding.endpoint)
            .field("grant", &self.0.binding.grant)
            .finish_non_exhaustive()
    }
}

/// The credentials of one call through a FAPI 2.0 exchange: the exchange's
/// own provider for the call, once the token endpoint is discovered.
struct OnBehalfOf<T> {
    inner: Arc<ExchangeInner<T>>,
    conveyance: Conveyance,
    withheld: Arc<Withheld>,
}

#[async_trait::async_trait]
impl<T: Transport + Clone + 'static> CredentialsProvider for OnBehalfOf<T> {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        let exchange = self.inner.exchange().await.map_err(CredentialsError::new)?;
        exchange
            .provider(&self.conveyance, &self.withheld)
            .credentials()
            .await
    }

    fn refused(&self) {
        if let Some(exchange) = self.inner.exchange.get() {
            exchange
                .provider(&self.conveyance, &self.withheld)
                .refused();
        }
    }
}

impl<T> fmt::Debug for OnBehalfOf<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OnBehalfOf")
            .field("endpoint", &self.inner.binding.endpoint)
            .finish_non_exhaustive()
    }
}
