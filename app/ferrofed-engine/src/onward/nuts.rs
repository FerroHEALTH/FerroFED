// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nuts grant: an onward token for a Verifiable Presentation.
//!
//! The token is obtained with a presentation of the gateway's credentials
//! and bound to a key of the gateway's with `DPoP` (Annex B §B.4, §13.3; the
//! IG's GFI-004 and GFI-005; Nuts RFC021).
//!
//! For an endpoint configured with a [`NutsGrant`], [`NutsCredentials`] asks
//! the node's authorization server for a token through
//! `nl-generic-functions`' `NutsClient`, presenting the holder's credentials.
//! The token is bound to the grant's [`Prover`]: the token request carries a
//! proof of it, through [`AuthorizationProver`], and every request to the
//! node carries one through the endpoint's [`NodeProver`](crate::onward::dpop::NodeProver),
//! so one key and one `DPoP` implementation serve both (RFC 9449 §5, §7).
//!
//! A Generic Functions service the gateway is a data user of, such as the
//! NVI Localization Service, is reached the same way: [`NutsAuthorizer`] is
//! the authorizer of its `nl-generic-functions` client, the same provider
//! obtains its token, and every request to the service carries the token
//! with a proof of the grant's key over the request (GFI-005).
//!
//! A token is cached as the client-credentials provider caches one: until
//! [`REFRESH_MARGIN`] before the end of the lifetime its `expires_in`
//! stated, one token request per endpoint or service at a time, and dropped
//! when the node or the service answers `401`. A token that cannot be
//! obtained fails that node or that request; the gateway never sends either
//! unauthenticated. A grant's token reaches its own node or service alone.
//! No error or log of this module carries a credential, the presentation or
//! the token.

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ferrofed_registry::id::EndpointId;
use http::header::WWW_AUTHENTICATE;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use nl_generic_functions::nuts_auth::error::{NutsAuthError, ProofError};
use nl_generic_functions::nuts_auth::holder::Holder;
use nl_generic_functions::nuts_auth::{DpopProver, Grant, NutsClient};
use nl_generic_functions::nvi::authorizer::{Authorized, Authorizer, AuthorizerError, Retry};
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider};
use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::onward::Clock;
use crate::onward::dpop::{self, Prover, Role};
use crate::onward::provider::REFRESH_MARGIN;

/// What a Nuts grant at one node's authorization server asks for, as whom,
/// and the key its tokens are bound to.
///
/// `Debug` shows the grant, the holder's DID and key id, and the `DPoP`
/// key's thumbprint, never a key or a credential.
#[derive(Debug, Clone)]
pub struct NutsGrant {
    grant: Grant,
    holder: Arc<Holder>,
    dpop: Arc<Prover>,
}

impl NutsGrant {
    /// The grant `grant`, presenting `holder`'s credentials, its tokens
    /// bound to `dpop`'s key.
    #[must_use]
    pub fn new(grant: Grant, holder: Arc<Holder>, dpop: Arc<Prover>) -> Self {
        Self {
            grant,
            holder,
            dpop,
        }
    }

    /// The authorization server and the scope.
    #[must_use]
    pub fn grant(&self) -> &Grant {
        &self.grant
    }

    /// The holder whose credentials are presented.
    #[must_use]
    pub fn holder(&self) -> &Holder {
        &self.holder
    }

    /// The key the tokens are bound to, which proves every request to the
    /// node.
    #[must_use]
    pub fn dpop(&self) -> &Arc<Prover> {
        &self.dpop
    }
}

/// The proofs of the token request, from the grant's [`Prover`] in the
/// authorization server's role (RFC 9449 §4.2, §8), as
/// `nl-generic-functions`' [`DpopProver`].
#[derive(Debug)]
pub struct AuthorizationProver<'a>(pub &'a Prover);

/// A proof the grant's key could not sign.
#[derive(Debug, thiserror::Error)]
#[error("the DPoP proof of the Nuts token request could not be signed")]
struct Unsigned(#[source] jsonwebtoken::errors::Error);

impl DpopProver for AuthorizationProver<'_> {
    fn algorithm(&self) -> &'static str {
        if self.0.algorithm() == jsonwebtoken::Algorithm::ES256 {
            "ES256"
        } else {
            "ES384"
        }
    }

    fn proof(&self, method: &Method, url: &Url) -> Result<String, ProofError> {
        self.0
            .prove((method, url), Role::Authorization, None)
            .map_err(|error| ProofError::new(Unsigned(error)))
    }

    fn nonce(&self, url: &Url, nonce: &str) {
        self.0.remember(Role::Authorization, url, nonce);
    }
}

/// A token in the cache, and when it is replaced.
struct Cached {
    token: SecretString,
    refresh_at: Instant,
}

/// Where a grant's tokens are sent: one node's endpoint, or a service of the
/// Generic Functions, by the key of its configuration table.
#[derive(Debug, Clone)]
enum Recipient {
    Endpoint(EndpointId),
    Service(&'static str),
}

/// The Nuts grant of one endpoint, as a [`CredentialsProvider`] for its
/// node client.
///
/// `Debug` names the endpoint or the service and the grant, never a token.
pub struct NutsCredentials {
    recipient: Recipient,
    grant: NutsGrant,
    client: NutsClient,
    timeout: Duration,
    clock: Arc<dyn Clock>,
    cached: Mutex<Option<Cached>>,
    fetch: tokio::sync::Mutex<()>,
}

impl NutsCredentials {
    /// The provider of `endpoint`'s `grant`, sending every request through
    /// `client` and waiting at most `timeout` for the whole token request.
    #[must_use]
    pub fn new(
        endpoint: EndpointId,
        grant: NutsGrant,
        client: NutsClient,
        timeout: Duration,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::to(Recipient::Endpoint(endpoint), grant, client, timeout, clock)
    }

    /// The provider of `grant`'s tokens for `recipient`.
    fn to(
        recipient: Recipient,
        grant: NutsGrant,
        client: NutsClient,
        timeout: Duration,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            recipient,
            grant,
            client,
            timeout,
            clock,
            cached: Mutex::new(None),
            fetch: tokio::sync::Mutex::new(()),
        }
    }

    /// The cached token, when one is still fresh.
    fn fresh(&self) -> Option<SecretString> {
        let now = self.clock.now();
        // NOTE: no specification governs this: our own design; a panic while
        // the lock was held leaves at worst a stale token, which a 401 drops.
        let cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        cached
            .as_ref()
            .filter(|cached| now < cached.refresh_at)
            .map(|cached| cached.token.clone())
    }

    /// The token for the next request: the cached one while it is fresh,
    /// and otherwise a new one, one token request at a time.
    async fn token(&self) -> Result<SecretString, NutsCredentialsError> {
        if let Some(fresh) = self.fresh() {
            return Ok(fresh);
        }
        let fetching = self.fetch.lock().await;
        if let Some(fresh) = self.fresh() {
            return Ok(fresh);
        }
        let obtained = self.obtain().await;
        drop(fetching);
        obtained
    }

    /// Obtains a new token, and caches it when its lifetime outlasts the
    /// margin.
    async fn obtain(&self) -> Result<SecretString, NutsCredentialsError> {
        let started = self.clock.now();
        let token = self
            .client
            .request_access_token(
                &self.grant.grant,
                &self.grant.holder,
                &AuthorizationProver(&self.grant.dpop),
                self.timeout,
            )
            .await
            .map_err(NutsCredentialsError::Token)?;
        let issued = token.token().clone();
        Credentials::dpop(issued.clone())
            .header_value()
            .map_err(NutsCredentialsError::Unsendable)?;
        let refresh_at = token
            .expires_in()
            .and_then(|lifetime| lifetime.checked_sub(REFRESH_MARGIN))
            .filter(|left| !left.is_zero())
            .and_then(|left| started.checked_add(left));
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        *cached = refresh_at.map(|refresh_at| Cached {
            token: issued.clone(),
            refresh_at,
        });
        drop(cached);
        match &self.recipient {
            Recipient::Endpoint(endpoint) => tracing::debug!(
                endpoint = %endpoint,
                cached = refresh_at.is_some(),
                "an onward token was obtained with the Nuts grant"
            ),
            Recipient::Service(service) => tracing::debug!(
                service,
                cached = refresh_at.is_some(),
                "a service token was obtained with the Nuts grant"
            ),
        }
        Ok(issued)
    }
}

/// Why the Nuts grant gave no token, so nothing was sent to the node or the
/// service.
///
/// No variant carries a credential, the presentation or a token.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NutsCredentialsError {
    /// The authorization server gave no `DPoP`-bound token.
    #[error("the Nuts access token request failed")]
    Token(#[source] NutsAuthError),
    /// The issued token cannot be sent as a credential.
    #[error("the issued Nuts access token cannot be sent as a credential")]
    Unsendable(#[source] openehr_its::rest::client::InvalidCredentials),
}

#[async_trait::async_trait]
impl CredentialsProvider for NutsCredentials {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        self.token()
            .await
            .map(Credentials::dpop)
            .map_err(CredentialsError::new)
    }

    fn refused(&self) {
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        *cached = None;
    }
}

impl fmt::Debug for NutsCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("NutsCredentials");
        match &self.recipient {
            Recipient::Endpoint(endpoint) => debug.field("endpoint", endpoint),
            Recipient::Service(service) => debug.field("service", service),
        };
        debug.field("grant", &self.grant).finish_non_exhaustive()
    }
}

/// The header a `DPoP` proof travels in (RFC 9449 §4.1), as a header name.
const PROOF_HEADER: HeaderName = HeaderName::from_static("dpop");

/// The Nuts grant of a Generic Functions service the gateway is a data user
/// of, as the [`Authorizer`] of that service's client (the IG's GFI-004 and
/// GFI-005).
///
/// Every request carries the grant's token under the `DPoP` scheme and a
/// proof of the grant's key over the request's method, its URL without query
/// and fragment, and the token (RFC 9449 §4.2, §7.1). The token is cached
/// and refreshed as a node's is, and dropped when the service answers `401`.
/// A `401` whose `DPoP` challenge names `use_dpop_nonce` keeps the nonce and
/// asks for the request once more (§9). The token goes to that service alone.
///
/// `Debug` names the service and the grant, never a token.
#[derive(Debug)]
pub struct NutsAuthorizer {
    credentials: NutsCredentials,
}

impl NutsAuthorizer {
    /// The authorizer of the requests to `service`, the key of its
    /// configuration table, with `grant`'s tokens, obtained through `client`
    /// within `timeout`.
    #[must_use]
    pub fn new(
        service: &'static str,
        grant: NutsGrant,
        client: NutsClient,
        timeout: Duration,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            credentials: NutsCredentials::to(
                Recipient::Service(service),
                grant,
                client,
                timeout,
                clock,
            ),
        }
    }

    /// The `Authorization` and `DPoP` headers of a request with `method` to
    /// `url`, each marked sensitive.
    async fn headers(&self, method: &Method, url: &Url) -> Result<HeaderMap, AuthorizerError> {
        let token = self
            .credentials
            .token()
            .await
            .map_err(AuthorizerError::new)?;
        let authorization = Credentials::dpop(token.clone())
            .header_value()
            .map_err(|source| AuthorizerError::new(NutsCredentialsError::Unsendable(source)))?;
        let proof = self
            .credentials
            .grant
            .dpop
            .prove((method, url), Role::Resource, Some(token.expose_secret()))
            .map_err(|source| AuthorizerError::new(Unproven::Signed(source)))?;
        let mut proof = HeaderValue::from_str(&proof)
            .map_err(|source| AuthorizerError::new(Unproven::Header(source)))?;
        proof.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(http::header::AUTHORIZATION, authorization);
        headers.insert(PROOF_HEADER, proof);
        Ok(headers)
    }
}

/// Why no `DPoP` proof of a service request could be made.
#[derive(Debug, thiserror::Error)]
enum Unproven {
    /// The grant's key could not sign the proof.
    #[error("the DPoP proof of the request could not be signed")]
    Signed(#[source] jsonwebtoken::errors::Error),
    /// The signed proof is not a header value.
    #[error("the DPoP proof of the request is not a header value")]
    Header(#[source] http::header::InvalidHeaderValue),
}

impl Authorizer for NutsAuthorizer {
    fn authorize<'a>(&'a self, method: &'a Method, url: &'a Url) -> Authorized<'a> {
        Box::pin(self.headers(method, url))
    }

    fn answered(&self, url: &Url, status: StatusCode, headers: &HeaderMap) -> Retry {
        // NOTE: RFC 9449 §8.1, a nonce is NQCHAR text, so a header value that
        // is not visible ASCII is legitimately no nonce.
        let nonce = headers
            .get(dpop::NONCE_HEADER)
            .and_then(|value| value.to_str().ok());
        if let Some(nonce) = nonce {
            self.credentials
                .grant
                .dpop
                .remember(Role::Resource, url, nonce);
        }
        if status != StatusCode::UNAUTHORIZED {
            return Retry::Done;
        }
        if nonce.is_some() && demands_nonce(headers) {
            return Retry::Resend;
        }
        self.credentials.refused();
        Retry::Done
    }
}

/// Whether `headers` carry a `DPoP` challenge naming `use_dpop_nonce`
/// (RFC 9449 §9).
fn demands_nonce(headers: &HeaderMap) -> bool {
    headers.get_all(WWW_AUTHENTICATE).iter().any(|value| {
        // NOTE: RFC 9110 §5.5 admits opaque octets; a challenge that is not
        // text is legitimately not a DPoP challenge.
        value.to_str().is_ok_and(|text| {
            text.split_once(' ').is_some_and(|(scheme, params)| {
                scheme.eq_ignore_ascii_case("DPoP") && params.contains(dpop::USE_NONCE)
            })
        })
    })
}
