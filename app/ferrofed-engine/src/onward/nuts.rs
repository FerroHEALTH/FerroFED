// SPDX-FileCopyrightText: Vernum Projecten B.V.
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
//! A token is cached as the client-credentials provider caches one: until
//! [`REFRESH_MARGIN`] before the end of the lifetime its `expires_in`
//! stated, one token request per endpoint at a time, and dropped when the
//! node answers `401`. A token that cannot be obtained fails that node; the
//! gateway never dispatches unauthenticated. No error or log of this module
//! carries a credential, the presentation or the token.

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ferrofed_registry::id::EndpointId;
use nl_generic_functions::nuts_auth::error::{NutsAuthError, ProofError};
use nl_generic_functions::nuts_auth::holder::Holder;
use nl_generic_functions::nuts_auth::{DpopProver, Grant, NutsClient};
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider};
use url::Url;

use crate::onward::Clock;
use crate::onward::dpop::{Prover, Role};
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

    fn proof(&self, method: &http::Method, url: &Url) -> Result<String, ProofError> {
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
    credentials: Credentials,
    refresh_at: Instant,
}

/// The Nuts grant of one endpoint, as a [`CredentialsProvider`] for its
/// node client.
///
/// `Debug` names the endpoint and the grant, never a token.
pub struct NutsCredentials {
    endpoint: EndpointId,
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
        Self {
            endpoint,
            grant,
            client,
            timeout,
            clock,
            cached: Mutex::new(None),
            fetch: tokio::sync::Mutex::new(()),
        }
    }

    /// The cached token, when one is still fresh.
    fn fresh(&self) -> Option<Credentials> {
        let now = self.clock.now();
        // NOTE: no specification governs this: our own design; a panic while
        // the lock was held leaves at worst a stale token, which a 401 drops.
        let cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        cached
            .as_ref()
            .filter(|cached| now < cached.refresh_at)
            .map(|cached| cached.credentials.clone())
    }

    /// Obtains a new token, and caches it when its lifetime outlasts the
    /// margin.
    async fn obtain(&self) -> Result<Credentials, NutsCredentialsError> {
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
        let credentials = Credentials::dpop(token.token().clone());
        credentials
            .header_value()
            .map_err(NutsCredentialsError::Unsendable)?;
        let refresh_at = token
            .expires_in()
            .and_then(|lifetime| lifetime.checked_sub(REFRESH_MARGIN))
            .filter(|left| !left.is_zero())
            .and_then(|left| started.checked_add(left));
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        *cached = refresh_at.map(|refresh_at| Cached {
            credentials: credentials.clone(),
            refresh_at,
        });
        drop(cached);
        tracing::debug!(
            endpoint = %self.endpoint,
            cached = refresh_at.is_some(),
            "an onward token was obtained with the Nuts grant"
        );
        Ok(credentials)
    }
}

/// Why the Nuts grant gave no token, so nothing was sent to the node.
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
        if let Some(fresh) = self.fresh() {
            return Ok(fresh);
        }
        let fetching = self.fetch.lock().await;
        if let Some(fresh) = self.fresh() {
            return Ok(fresh);
        }
        let obtained = self.obtain().await;
        drop(fetching);
        obtained.map_err(CredentialsError::new)
    }

    fn refused(&self) {
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        *cached = None;
    }
}

impl fmt::Debug for NutsCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NutsCredentials")
            .field("endpoint", &self.endpoint)
            .field("grant", &self.grant)
            .finish_non_exhaustive()
    }
}
