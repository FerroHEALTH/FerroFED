// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials provider of one endpoint's onward grant: it obtains a
//! token at the node's token endpoint, caches it, and hands it to the node
//! client before each attempt (§13.1, N25).
//!
//! A token is cached until [`REFRESH_MARGIN`] before the end of the lifetime
//! its `expires_in` stated, counted from when the request left. A token with
//! no stated lifetime, or one shorter than the margin, serves the one
//! request it was obtained for. One token request per endpoint runs at a
//! time: a call that finds the cache empty while another is fetching waits
//! for that fetch and takes its token. A `401` from the node drops the
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

/// A token in the cache, and when it is replaced.
struct Cached {
    credentials: Credentials,
    refresh_at: Instant,
}

/// The onward grant of one endpoint, as a [`CredentialsProvider`] for its
/// node client.
///
/// `Debug` names the endpoint and the client, never a token.
pub struct ClientCredentials<T> {
    endpoint: EndpointId,
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
    /// The provider of `endpoint`'s `grant`, signing each assertion with
    /// the current key of `keys`, valid for `lifetime`, and sending each
    /// token request over `transport`, waiting at most `timeout` for it.
    ///
    /// `lifetime` is held to [`MAX_ASSERTION_LIFETIME`].
    #[must_use]
    pub fn new(
        endpoint: EndpointId,
        grant: Grant,
        keys: Arc<KeyRing>,
        (lifetime, timeout): (Duration, Duration),
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            endpoint,
            grant,
            keys: Some(keys),
            lifetime: lifetime.min(MAX_ASSERTION_LIFETIME),
            transport,
            timeout,
            clock,
            cached: Mutex::new(None),
            fetch: tokio::sync::Mutex::new(()),
        }
    }

    /// The provider of `endpoint`'s `grant` with no key of the gateway's,
    /// for a grant authenticated by the TLS client certificate `transport`
    /// presents (RFC 8705 §2), waiting at most `timeout` for each token
    /// request.
    ///
    /// A grant that authenticates with a client assertion gets no token
    /// from it ([`TokenError::Unsigned`]).
    #[must_use]
    pub fn by_certificate(
        endpoint: EndpointId,
        grant: Grant,
        timeout: Duration,
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            endpoint,
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
            endpoint = %self.endpoint,
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
            .field("endpoint", &self.endpoint)
            .field("grant", &self.grant)
            .field("lifetime", &self.lifetime)
            .finish_non_exhaustive()
    }
}
