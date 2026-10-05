// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials provider of one client-credentials grant.
//!
//! It obtains a token at the token endpoint, caches it, and hands it to the
//! client before each attempt. A node's grant serves its node client (§13.1,
//! N25); an identity service's serves the IHE FHIR client of that service
//! (IHE IUA ITI-71 §3.71.4.1.2.1, ITI-72 §3.72.4.2).
//!
//! A token is cached until [`REFRESH_MARGIN`] before the end of the lifetime
//! its `expires_in` stated, counted from when the request left. A token with
//! no stated lifetime, or one shorter than the margin, serves the one
//! request it was obtained for. One token request per provider runs at a
//! time: a call that finds the cache empty while another is fetching waits
//! for that fetch and takes its token. A `401` from the service drops the
//! cached token, so the next attempt obtains a new one. No specification
//! governs the caching: our own design.

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider, Transport};

use crate::onward::keys::KeyRing;
use crate::onward::token::{self, MAX_ASSERTION_LIFETIME, REFRESH_MARGIN, TokenError};
use crate::onward::{Clock, Grant};

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

/// A token in the cache, and when it is replaced.
struct Cached {
    credentials: Credentials,
    refresh_at: Instant,
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
    clock: Arc<dyn Clock>,
    cached: Mutex<Option<Cached>>,
    fetch: tokio::sync::Mutex<()>,
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
            clock,
            cached: Mutex::new(None),
            fetch: tokio::sync::Mutex::new(()),
        }
    }

    /// The grant this provider obtains tokens with.
    #[must_use]
    pub fn grant(&self) -> &Grant {
        &self.grant
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
    async fn obtain(&self) -> Result<Credentials, TokenError> {
        let started = self.clock.now();
        let issued = token::request(
            &self.grant,
            self.keys
                .as_deref()
                .map(|keys| (keys.current(), self.lifetime)),
            &self.transport,
            self.timeout,
        )
        .await?;
        let refresh_at = issued
            .expires_in
            .and_then(|lifetime| lifetime.checked_sub(REFRESH_MARGIN))
            .filter(|left| !left.is_zero())
            .and_then(|left| started.checked_add(left));
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        *cached = refresh_at.map(|refresh_at| Cached {
            credentials: issued.credentials.clone(),
            refresh_at,
        });
        drop(cached);
        tracing::debug!(
            recipient = %self.recipient,
            cached = refresh_at.is_some(),
            "an onward token was obtained"
        );
        Ok(issued.credentials)
    }
}

#[async_trait::async_trait]
impl<T: Transport + 'static> CredentialsProvider for ClientCredentials<T> {
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

impl<T> fmt::Debug for ClientCredentials<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCredentials")
            .field("recipient", &self.recipient)
            .field("grant", &self.grant)
            .field("lifetime", &self.lifetime)
            .finish_non_exhaustive()
    }
}
