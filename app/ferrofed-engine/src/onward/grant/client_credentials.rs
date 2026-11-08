// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials provider of one client-credentials grant.
//!
//! It obtains a token at the token endpoint, caches it, and hands it to the
//! client before each attempt. A node's grant serves its node client (§13.1,
//! N25); an identity service's serves the IHE FHIR client of that service
//! (IHE IUA ITI-71 §3.71.4.1.2.1, ITI-72 §3.72.4.2).
//!
//! A token is cached until [`REFRESH_MARGIN`](crate::onward::token::REFRESH_MARGIN) before the end of the lifetime
//! its `expires_in` stated, counted from when the request left. A token with
//! no stated lifetime, or one shorter than the margin, serves the one
//! request it was obtained for. One token request per provider runs at a
//! time: a call that finds the cache empty while another is fetching waits
//! for that fetch and takes its token. A `401` from the service drops the
//! cached token, so the next attempt obtains a new one. No specification
//! governs the caching: our own design.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider, Transport};

use crate::onward::grant::cache::TokenCache;
use crate::onward::token::{self, MAX_ASSERTION_LIFETIME, TokenError};
use crate::onward::{Clock, Grant};
use ferrohealth_smart::client::keys::KeyRing;

/// Whom a [`ClientCredentials`] provider obtains its tokens for, named in
/// its `Debug` and its log events.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Recipient {
    /// A node's endpoint, reached by its node client.
    Endpoint(EndpointId),
    /// An identity, localization or directory service, by the configuration
    /// key of its table, such as `pixm.manager[0]`.
    Service(String),
}

impl From<EndpointId> for Recipient {
    fn from(endpoint: EndpointId) -> Self {
        Self::Endpoint(endpoint)
    }
}

impl fmt::Display for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Endpoint(endpoint) => endpoint.fmt(f),
            Self::Service(key) => f.write_str(key),
        }
    }
}

/// One client-credentials grant, as a [`CredentialsProvider`] for the
/// client of the service it is for.
///
/// `Debug` names the recipient and the client, never a token or a secret.
pub struct ClientCredentials<T> {
    recipient: Recipient,
    grant: Grant,
    keys: Option<Arc<KeyRing>>,
    lifetime: Duration,
    transport: T,
    timeout: Duration,
    cache: TokenCache<Credentials>,
}

impl<T: Transport> ClientCredentials<T> {
    /// The provider of `recipient`'s `grant`, signing each assertion with
    /// the current key of `keys`, valid for `lifetime`, and sending each
    /// token request over `transport`, waiting at most `timeout` for it.
    ///
    /// `lifetime` is held to [`MAX_ASSERTION_LIFETIME`].
    #[must_use]
    pub fn new(
        recipient: impl Into<Recipient>,
        grant: Grant,
        keys: Arc<KeyRing>,
        (lifetime, timeout): (Duration, Duration),
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            keys: Some(keys),
            lifetime: lifetime.min(MAX_ASSERTION_LIFETIME),
            ..Self::unsigned(recipient.into(), grant, timeout, transport, clock)
        }
    }

    /// The provider of `recipient`'s `grant` with no key of the gateway's,
    /// for a grant authenticated by the TLS client certificate `transport`
    /// presents (RFC 8705 §2), waiting at most `timeout` for each token
    /// request.
    ///
    /// A grant that authenticates with a client assertion gets no token
    /// from it ([`TokenError::Unsigned`]).
    #[must_use]
    pub fn by_certificate(
        recipient: impl Into<Recipient>,
        grant: Grant,
        timeout: Duration,
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::unsigned(recipient.into(), grant, timeout, transport, clock)
    }

    /// The provider of `recipient`'s `grant` with no key of the gateway's,
    /// for a grant authenticated by its client secret (RFC 6749 §2.3.1),
    /// sending each token request over `transport` and waiting at most
    /// `timeout` for it.
    ///
    /// A grant that authenticates with a client assertion gets no token
    /// from it ([`TokenError::Unsigned`]).
    #[must_use]
    pub fn by_secret(
        recipient: impl Into<Recipient>,
        grant: Grant,
        timeout: Duration,
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::unsigned(recipient.into(), grant, timeout, transport, clock)
    }

    /// The provider of `recipient`'s `grant` with no key to sign an
    /// assertion with.
    fn unsigned(
        recipient: Recipient,
        grant: Grant,
        timeout: Duration,
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            recipient,
            grant,
            keys: None,
            lifetime: MAX_ASSERTION_LIFETIME,
            transport,
            timeout,
            cache: TokenCache::new(clock),
        }
    }

    /// The grant this provider obtains tokens with.
    #[must_use]
    pub fn grant(&self) -> &Grant {
        &self.grant
    }

    /// Obtains a new token, requested at `started`, and caches it when its
    /// lifetime outlasts the margin.
    async fn obtain(&self, started: Instant) -> Result<Credentials, TokenError> {
        let issued = token::request(
            &self.grant,
            self.keys
                .as_deref()
                .map(|keys| (keys.current(), self.lifetime)),
            &self.transport,
            self.timeout,
        )
        .await?;
        let cached = self
            .cache
            .keep(&issued.credentials, started, issued.expires_in);
        tracing::debug!(
            recipient = %self.recipient,
            cached,
            "an onward token was obtained"
        );
        Ok(issued.credentials)
    }
}

#[async_trait::async_trait]
impl<T: Transport + 'static> CredentialsProvider for ClientCredentials<T> {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        self.cache
            .token(async |started| self.obtain(started).await)
            .await
            .map_err(CredentialsError::new)
    }

    fn refused(&self) {
        self.cache.clear();
    }
}

impl<T> fmt::Debug for ClientCredentials<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCredentials")
            .field("recipient", &self.recipient)
            .field("grant", &self.grant)
            .field("lifetime", &self.lifetime)
            .finish_non_exhaustive()
    }
}
