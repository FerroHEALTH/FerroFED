// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one token a grant holds for every request, which the
//! client-credentials grant and the Nuts grant share.
//!
//! A token is kept until [`REFRESH_MARGIN`] before the end of the lifetime
//! its token response states (RFC 6749 §5.1 `expires_in`); one with no
//! stated lifetime, or one shorter than the margin, is not kept. A new token
//! is requested one at a time, so callers that find the cache stale together
//! wait for one token request. A node's `401` drops the token. No
//! specification governs the cache: our own design.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::onward::Clock;
use crate::onward::token::REFRESH_MARGIN;

/// A token in the cache, and when it is replaced.
struct Cached<V> {
    token: V,
    refresh_at: Instant,
}

/// The token of one grant, kept while it is fresh.
pub(crate) struct TokenCache<V> {
    clock: Arc<dyn Clock>,
    cached: Mutex<Option<Cached<V>>>,
    fetch: tokio::sync::Mutex<()>,
}

impl<V: Clone> TokenCache<V> {
    /// An empty cache that reads `clock`.
    pub(crate) fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            cached: Mutex::new(None),
            fetch: tokio::sync::Mutex::new(()),
        }
    }

    /// The cached token, when one is still fresh.
    fn fresh(&self) -> Option<V> {
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
    /// and otherwise the one `obtain` requests, given the instant it starts,
    /// one token request at a time.
    pub(crate) async fn token<E>(
        &self,
        obtain: impl AsyncFnOnce(Instant) -> Result<V, E>,
    ) -> Result<V, E> {
        if let Some(fresh) = self.fresh() {
            return Ok(fresh);
        }
        let fetching = self.fetch.lock().await;
        if let Some(fresh) = self.fresh() {
            return Ok(fresh);
        }
        let obtained = obtain(self.clock.now()).await;
        drop(fetching);
        obtained
    }

    /// Keeps `token`, requested at `started` and valid for `expires_in`,
    /// when its lifetime outlasts the margin, and says whether it was kept.
    pub(crate) fn keep(&self, token: &V, started: Instant, expires_in: Option<Duration>) -> bool {
        let refresh_at = expires_in
            .and_then(|lifetime| lifetime.checked_sub(REFRESH_MARGIN))
            .filter(|left| !left.is_zero())
            .and_then(|left| started.checked_add(left));
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        *cached = refresh_at.map(|refresh_at| Cached {
            token: token.clone(),
            refresh_at,
        });
        refresh_at.is_some()
    }

    /// Drops the cached token, which the node refused.
    pub(crate) fn clear(&self) {
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        *cached = None;
    }
}
