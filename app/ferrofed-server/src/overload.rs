// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Overload protection at the listener: a bound on the requests served at
//! once, and an optional rate per verified caller.
//!
//! One federated query becomes a request to every member, so the gateway
//! bounds how much it takes on. A request past
//! `server.max_concurrent_requests` is answered `503 overloaded` at once,
//! with `Retry-After` (RFC 9110 §15.6.4, §10.2.3), and reaches nothing
//! behind the listener: no client authentication, no resolver and no node.
//! The health family is never refused, so a busy gateway is not restarted
//! by its own liveness probe. A caller that sends faster than
//! `[server.caller_rate]` admits is answered `429 rate-limited` with
//! `Retry-After` (RFC 6585 §4). The caller is the one client authentication
//! verified, by its issuer and `client_id`; a forwarded address is never
//! read, so a client cannot spread its load over addresses it names.
//!
//! The per-member bound lives in the node dispatch
//! ([`ferrofed_engine::dispatch::cap`]). Every refusal is counted by its
//! limit ([`crate::metrics::nodes::Limit`]). No specification governs the
//! limits: our own design.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use http::{HeaderValue, header};
use tokio::sync::Semaphore;

use crate::auth::caller::Caller;
use crate::base_path::BasePath;
use crate::config::limits::{CallerRateSettings, Overload};
use crate::error::{self, Code};
use crate::metrics::Metrics;
use crate::metrics::nodes::Limit;
use crate::request_id;

/// The most callers the rate limit tracks before it forgets the ones whose
/// bucket has refilled.
pub const TRACKED_CALLERS: usize = 10_000;

/// The concurrency limit of one listener.
#[derive(Debug)]
pub struct Admission {
    slots: Arc<Semaphore>,
    retry_after: Duration,
    exempt: [String; 2],
    metrics: Arc<Metrics>,
}

impl Admission {
    /// Returns the limit `overload` sets on the surface under `base`,
    /// counting each refusal in `metrics`.
    #[must_use]
    pub fn new(overload: &Overload, base: &BasePath, metrics: Arc<Metrics>) -> Self {
        // NOTE: no specification governs this: our own design; a bound past
        // what a semaphore holds is held to that maximum.
        let permits = usize::try_from(overload.max_concurrent_requests.get())
            .unwrap_or(Semaphore::MAX_PERMITS)
            .min(Semaphore::MAX_PERMITS);
        Self {
            slots: Arc::new(Semaphore::new(permits)),
            retry_after: overload.retry_after,
            exempt: [base.join("/health"), base.join("/health/readiness")],
            metrics,
        }
    }
}

/// The middleware that serves a request while a slot is free and refuses it
/// `503 overloaded` otherwise; the health family always passes.
pub async fn admit(
    State(admission): State<Arc<Admission>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| request.uri().path(), |original| original.path());
    if admission.exempt.iter().any(|exempt| exempt == path) {
        return next.run(request).await;
    }
    let Ok(_slot) = Arc::clone(&admission.slots).try_acquire_owned() else {
        admission.metrics.shed(Limit::Concurrency);
        tracing::warn!(
            limit = Limit::Concurrency.as_str(),
            "a request was refused: the gateway serves as many requests as it takes at once"
        );
        let request_id = request_id::of(request.headers()).unwrap_or_default();
        return refused(Code::Overloaded, admission.retry_after, request_id);
    };
    next.run(request).await
}

/// The rate limit of each verified caller.
pub struct CallerLimit {
    limiter: DefaultKeyedRateLimiter<(String, String)>,
    clock: DefaultClock,
    metrics: Arc<Metrics>,
}

impl CallerLimit {
    /// Returns the limit `rate` sets, counting each refusal in `metrics`.
    #[must_use]
    pub fn new(rate: CallerRateSettings, metrics: Arc<Metrics>) -> Self {
        let quota = Quota::per_second(rate.requests_per_second).allow_burst(rate.burst);
        Self {
            limiter: RateLimiter::keyed(quota),
            clock: DefaultClock::default(),
            metrics,
        }
    }

    /// Takes one request of the caller `key` from its bucket, or returns how
    /// long it waits before its next request is admitted.
    fn take(&self, key: &(String, String)) -> Result<(), Duration> {
        let taken = self
            .limiter
            .check_key(key)
            .map_err(|refused| refused.wait_time_from(self.clock.now()));
        if self.limiter.len() > TRACKED_CALLERS {
            self.limiter.retain_recent();
        }
        taken
    }
}

impl std::fmt::Debug for CallerLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallerLimit").finish_non_exhaustive()
    }
}

/// The middleware that admits a verified caller's request while its bucket
/// holds one.
///
/// A request past the bucket is refused `429 rate-limited`; a request outside
/// client authentication carries no caller and always passes.
pub async fn per_caller(
    State(limit): State<Arc<CallerLimit>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(caller) = request.extensions().get::<Caller>() else {
        return next.run(request).await;
    };
    // NOTE: no specification governs this: our own design; the key is the
    // verified issuer and client, never an address a client or proxy names.
    let key = (caller.issuer().to_owned(), caller.client_id().to_owned());
    match limit.take(&key) {
        Ok(()) => next.run(request).await,
        Err(wait) => {
            limit.metrics.shed(Limit::CallerRate);
            tracing::warn!(
                limit = Limit::CallerRate.as_str(),
                "a request was refused: its caller sent more requests than one caller may"
            );
            let request_id = request_id::of(request.headers()).unwrap_or_default();
            refused(Code::RateLimited, wait, request_id)
        }
    }
}

/// The answer to a refused request: `code` with its fixed message, and
/// `Retry-After` naming `wait` in whole seconds, rounded up and at least
/// one (RFC 9110 §10.2.3).
fn refused(code: Code, wait: Duration, request_id: &str) -> Response {
    let mut response = error::fixed(code, request_id);
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(retry_seconds(wait)));
    response
}

/// `wait` in whole seconds, rounded up and at least one.
fn retry_seconds(wait: Duration) -> u64 {
    let seconds = wait
        .as_secs()
        .saturating_add(u64::from(wait.subsec_nanos() > 0));
    seconds.max(1)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::retry_seconds;

    #[test]
    fn a_wait_is_rounded_up_to_whole_seconds_and_never_zero() {
        assert_eq!(1, retry_seconds(Duration::ZERO));
        assert_eq!(1, retry_seconds(Duration::from_millis(1)));
        assert_eq!(1, retry_seconds(Duration::from_secs(1)));
        assert_eq!(2, retry_seconds(Duration::from_millis(1001)));
    }
}
