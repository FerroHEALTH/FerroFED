// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How the gateway authenticates to a node as itself: an OAuth 2.0 grant
//! authenticated by a signed JWT client assertion (§13.1, N25, CP-17).
//!
//! For an endpoint configured with a [`Grant`], the gateway obtains an
//! access token at the node's token endpoint, authenticating with an ES384
//! client assertion (RFC 7523 §2.2) signed by the current key of its
//! [`keys::KeyRing`], whose public keys it publishes as a JWK Set (RFC 7517).
//! A [`GrantKind::ClientCredentials`] grant (RFC 6749 §4.4) gives the
//! endpoint one token for every caller, through the
//! [`provider::ClientCredentials`] provider. A [`GrantKind::TokenExchange`]
//! grant (RFC 8693) gives each verified caller a token of its own, through
//! [`exchange::Exchange`]: the caller's verified token is the subject, a
//! second assertion of the gateway the actor, and the scope the caller's
//! scope narrowed to the operation. Either provider caches what it obtained
//! and drops it when the node answers `401`. An onward token that cannot be
//! obtained fails that node; the gateway never dispatches unauthenticated.
//! The caller's own token reaches no node: under token exchange it reaches
//! that node's authorization server alone. What a node is told about the
//! caller is a token of the gateway's own, signed with the same keys
//! ([`conveyance`]; N24).
//!
//! A grant may bind its tokens to a key of the gateway's with `DPoP` (RFC
//! 9449): its [`dpop::Prover`] proves every request to its token endpoint,
//! and every request to its node through the node client's
//! [`dpop::NodeProver`].
//!
//! An endpoint on the Dutch Generic Functions' Nuts track (Annex B §B.4)
//! obtains its token with a Verifiable Presentation of the gateway's
//! credentials instead, bound with `DPoP` to its own [`dpop::Prover`]
//! ([`nuts`]). An endpoint whose authorization server follows the FAPI 2.0
//! Security Profile, as the BgZ/eOverdracht track of Annex B §B.4a does,
//! discovers that server from its issuer and obtains a `DPoP`-bound token
//! with an ES256 assertion naming the issuer, and RFC 9396
//! `authorization_details` where configured ([`fapi2`]).

use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use ferrofed_registry::secret::SecretUrl;
use oauth_server_metadata::Issuer;
use openehr_sdt::smart_scopes::{Compartment, SmartScope};
use url::Url;

use crate::onward::authorization_details::AuthorizationDetails;
use crate::onward::dpop::Prover;

pub mod authorization_details;
pub mod conveyance;
pub mod dpop;
pub mod exchange;
pub mod fapi2;
pub mod keys;
#[cfg(feature = "nl")]
pub mod nuts;
pub mod provider;
pub mod token;

/// The monotonic clock the token cache and the key rotation read.
///
/// The gateway reads [`SystemClock`]; a test supplies one it moves itself.
pub trait Clock: Send + Sync + fmt::Debug {
    /// The current instant.
    fn now(&self) -> Instant;
}

/// The process's monotonic clock, [`Instant::now`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The scope an onward token is requested with, in the SMART on openEHR
/// grammar (ITS-REST SMART App Launch, master08 §Resource Scopes).
///
/// Every scope is a resource scope of the `system` compartment, the one a
/// client-credentials grant to the gateway names, as `openehr-sdt` reads
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    scopes: Vec<SmartScope>,
    text: String,
}

/// A scope that cannot be requested.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ScopeError {
    /// The scope names no scope at all.
    #[error("the scope is empty")]
    Empty,
    /// A scope is not a resource scope of the `system` compartment
    /// (master08 §Resource Scopes).
    #[error(
        "{scope:?} is not a system resource scope (system/<template-|composition-|aql-><id>.<cruds>)"
    )]
    NotSystemResource {
        /// The scope as written.
        scope: String,
    },
}

impl Scope {
    /// Reads `text`, the space-delimited scopes of RFC 6749 §3.3, each with
    /// [`SmartScope::parse`].
    ///
    /// # Errors
    ///
    /// Returns [`ScopeError::Empty`] when `text` names no scope, and
    /// [`ScopeError::NotSystemResource`] for a scope that is not a resource
    /// scope of the `system` compartment.
    pub fn parse(text: &str) -> Result<Self, ScopeError> {
        let mut scopes = Vec::new();
        for raw in text.split_whitespace() {
            let scope = SmartScope::parse(raw);
            match &scope {
                SmartScope::Resource(resource) if resource.compartment == Compartment::System => {}
                _ => {
                    return Err(ScopeError::NotSystemResource {
                        scope: raw.to_owned(),
                    });
                }
            }
            scopes.push(scope);
        }
        if scopes.is_empty() {
            return Err(ScopeError::Empty);
        }
        let text = SmartScope::format_all(&scopes);
        Ok(Self { scopes, text })
    }

    /// The scopes, as `openehr-sdt` read them.
    #[must_use]
    pub fn scopes(&self) -> &[SmartScope] {
        &self.scopes
    }

    /// The `scope` parameter the token request carries: each scope in the
    /// canonical form of the SMART on openEHR grammar, as `openehr-sdt`
    /// prints it, one space apart (RFC 6749 §3.3).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// What a grant at one node's token endpoint asks for, and as whom.
///
/// `Debug` shows the token endpoint with its credentials redacted, and the
/// types of its `authorization_details` alone.
#[derive(Clone)]
pub struct Grant {
    kind: GrantKind,
    token_endpoint: Url,
    client_id: String,
    scope: Option<Scope>,
    resource: Option<Url>,
    audience: Option<String>,
    assertion_audience: AssertionAudience,
    authorization_details: Option<AuthorizationDetails>,
    dpop: Option<Arc<Prover>>,
}

/// The `aud` of every client assertion a [`Grant`] signs (RFC 7523 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AssertionAudience {
    /// The token endpoint's URL, which RFC 7523 §3 admits as the
    /// authorization server's identity.
    TokenEndpoint,
    /// The authorization server's issuer identifier (RFC 8414 §2), as a
    /// string: the one value an authorization server under the FAPI 2.0
    /// Security Profile accepts (§5.3.2.1, §5.3.3.1).
    Issuer(Issuer),
}

/// How a [`Grant`] obtains its tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GrantKind {
    /// The client-credentials grant (RFC 6749 §4.4): one token for every
    /// request to the endpoint, with the grant's own [`Scope`].
    ClientCredentials,
    /// Token exchange (RFC 8693 §2): a token per verified caller, with the
    /// caller's token as the subject and the gateway as the actor. The
    /// gateway's own requests, which have no caller, use the
    /// client-credentials grant at the same token endpoint.
    TokenExchange,
}

/// A grant that cannot be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GrantError {
    /// The token endpoint is not an `http` or `https` URL.
    #[error("the token endpoint is not an http or https URL")]
    TokenEndpoint(#[source] Option<url::ParseError>),
    /// The token endpoint carries a user name or a password, which belong in
    /// no URL.
    #[error("the token endpoint carries credentials in its URL")]
    TokenEndpointCredentials,
    /// The token endpoint carries a query or a fragment (RFC 6749 §3.2).
    #[error("the token endpoint carries a query or a fragment")]
    TokenEndpointQuery,
    /// The `client_id` is empty.
    #[error("the client_id is empty")]
    ClientId,
    /// The `resource` is not an absolute URI without a fragment (RFC 8707
    /// §2).
    #[error("the resource is not an absolute URI without a fragment (RFC 8707 §2)")]
    Resource(#[source] Option<url::ParseError>),
    /// The `audience` is empty.
    #[error("the audience is empty")]
    Audience,
}

impl Grant {
    /// A grant at `token_endpoint` for the client `client_id`, asking for
    /// `scope`.
    ///
    /// # Errors
    ///
    /// Returns [`GrantError::TokenEndpoint`] for an endpoint that is no
    /// `http` or `https` URL, [`GrantError::TokenEndpointCredentials`] for
    /// one that carries a user name or password,
    /// [`GrantError::TokenEndpointQuery`] for one with a query or a fragment,
    /// and [`GrantError::ClientId`] for an empty `client_id`.
    pub fn new(
        token_endpoint: &SecretUrl,
        client_id: impl Into<String>,
        scope: Scope,
    ) -> Result<Self, GrantError> {
        Self::at(token_endpoint, client_id, Some(scope))
    }

    /// A grant at `token_endpoint` for the client `client_id`, asking for
    /// `scope` when one is given, as [`Grant::new`] checks it.
    pub(crate) fn at(
        token_endpoint: &SecretUrl,
        client_id: impl Into<String>,
        scope: Option<Scope>,
    ) -> Result<Self, GrantError> {
        let url = Url::parse(token_endpoint.expose())
            .map_err(|source| GrantError::TokenEndpoint(Some(source)))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(GrantError::TokenEndpoint(None));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(GrantError::TokenEndpointCredentials);
        }
        // NOTE: RFC 6749 §3.2 forbids a fragment; a query is our own refusal, so
        // the URL can carry no secret into the gateway's logs.
        if url.fragment().is_some() || url.query().is_some() {
            return Err(GrantError::TokenEndpointQuery);
        }
        let client_id = client_id.into();
        if client_id.is_empty() {
            return Err(GrantError::ClientId);
        }
        Ok(Self {
            kind: GrantKind::ClientCredentials,
            token_endpoint: url,
            client_id,
            scope,
            resource: None,
            audience: None,
            assertion_audience: AssertionAudience::TokenEndpoint,
            authorization_details: None,
            dpop: None,
        })
    }

    /// This grant, naming `issuer`, the authorization server's issuer
    /// identifier, as the `aud` of every client assertion in place of the
    /// token endpoint (RFC 7523 §3; FAPI 2.0 Security Profile §5.3.2.1).
    #[must_use]
    pub fn with_issuer_audience(mut self, issuer: Issuer) -> Self {
        self.assertion_audience = AssertionAudience::Issuer(issuer);
        self
    }

    /// This grant, asking for `details` with `authorization_details` in
    /// every token request (RFC 9396 §6), and taking only a token response
    /// that states the details it granted (§7).
    #[must_use]
    pub fn with_authorization_details(mut self, details: AuthorizationDetails) -> Self {
        self.authorization_details = Some(details);
        self
    }

    /// This grant, obtaining a token per verified caller by token exchange
    /// (RFC 8693 §2).
    #[must_use]
    pub fn with_token_exchange(mut self) -> Self {
        self.kind = GrantKind::TokenExchange;
        self
    }

    /// This grant, binding its tokens to `prover`'s key with `DPoP` (RFC
    /// 9449): the token endpoint must answer `token_type` `DPoP` (§5), and
    /// the requests to the token endpoint and to the node carry proofs of
    /// that key ([`dpop::NodeProver`] for the node's).
    #[must_use]
    pub fn with_dpop(mut self, prover: Arc<Prover>) -> Self {
        self.dpop = Some(prover);
        self
    }

    /// This grant as the client-credentials grant at the same token
    /// endpoint, for the gateway's own requests.
    #[must_use]
    pub fn as_client_credentials(&self) -> Self {
        Self {
            kind: GrantKind::ClientCredentials,
            ..self.clone()
        }
    }

    /// How the grant obtains its tokens.
    #[must_use]
    pub fn kind(&self) -> GrantKind {
        self.kind
    }

    /// The key the grant's tokens are bound to with `DPoP` (RFC 9449), when
    /// they are.
    #[must_use]
    pub fn dpop(&self) -> Option<&Arc<Prover>> {
        self.dpop.as_ref()
    }

    /// This grant, naming `resource` as the target service (RFC 8707 §2).
    ///
    /// # Errors
    ///
    /// Returns [`GrantError::Resource`] for a value that is no absolute URI
    /// or carries a fragment.
    pub fn with_resource(mut self, resource: &str) -> Result<Self, GrantError> {
        let url = Url::parse(resource).map_err(|source| GrantError::Resource(Some(source)))?;
        if url.fragment().is_some() {
            return Err(GrantError::Resource(None));
        }
        self.resource = Some(url);
        Ok(self)
    }

    /// This grant, naming `audience` as the audience it asks the token for.
    ///
    /// # Errors
    ///
    /// Returns [`GrantError::Audience`] for an empty value.
    pub fn with_audience(mut self, audience: impl Into<String>) -> Result<Self, GrantError> {
        let audience = audience.into();
        if audience.is_empty() {
            return Err(GrantError::Audience);
        }
        self.audience = Some(audience);
        Ok(self)
    }

    /// The token endpoint.
    #[must_use]
    pub fn token_endpoint(&self) -> &Url {
        &self.token_endpoint
    }

    /// What every assertion names as its `aud`.
    #[must_use]
    pub fn assertion_audience(&self) -> &AssertionAudience {
        &self.assertion_audience
    }

    /// The `aud` of every assertion, as it is written: the token endpoint's
    /// URL or the issuer identifier (RFC 7523 §3).
    #[must_use]
    pub fn assertion_aud(&self) -> &str {
        match &self.assertion_audience {
            AssertionAudience::TokenEndpoint => self.token_endpoint.as_str(),
            AssertionAudience::Issuer(issuer) => issuer.as_str(),
        }
    }

    /// The `authorization_details` every token request asks for, when the
    /// grant asks for any (RFC 9396 §6).
    #[must_use]
    pub fn authorization_details(&self) -> Option<&AuthorizationDetails> {
        self.authorization_details.as_ref()
    }

    /// The `client_id`, the `iss` and `sub` of every assertion (RFC 7523
    /// §3).
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// The scope the token is requested with, when it is requested with one.
    #[must_use]
    pub fn scope(&self) -> Option<&Scope> {
        self.scope.as_ref()
    }

    /// The target service named with `resource`, when one is.
    #[must_use]
    pub fn resource(&self) -> Option<&Url> {
        self.resource.as_ref()
    }

    /// The audience asked for, when one is.
    #[must_use]
    pub fn audience(&self) -> Option<&str> {
        self.audience.as_deref()
    }
}

impl fmt::Debug for Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Grant")
            .field("kind", &self.kind)
            .field(
                "token_endpoint",
                &SecretUrl::new(String::from(self.token_endpoint.clone())),
            )
            .field("client_id", &self.client_id)
            .field("scope", &self.scope.as_ref().map(Scope::as_str))
            .field("resource", &self.resource.as_ref().map(Url::as_str))
            .field("audience", &self.audience)
            .field("assertion_audience", &self.assertion_audience)
            .field("authorization_details", &self.authorization_details)
            .field("dpop", &self.dpop)
            .finish()
    }
}
