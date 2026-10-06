// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The overload limits of the listener: how many requests the gateway
//! serves at once, and how many one verified caller may send.
//!
//! A request past the concurrency limit is answered `503` with
//! `Retry-After` (RFC 9110 §15.6.4, §10.2.3), and one past a caller's rate
//! `429` with `Retry-After` (RFC 6585 §4). No specification governs the
//! limits themselves: our own design.

use std::num::NonZeroU32;
use std::time::Duration;

use serde::Deserialize;

use crate::config::error::Error;

/// The per-caller rate limit, `[server.caller_rate]`: a token bucket per
/// verified caller, refilled at `requests_per_second` and holding `burst`.
///
/// The caller is the one client authentication verified, by its issuer
/// and `client_id`; a forwarded address is never read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CallerRate {
    /// The requests one caller may send per second, sustained; zero is
    /// refused.
    pub requests_per_second: u32,
    /// The requests one caller may send at once after a quiet spell; zero is
    /// refused.
    pub burst: u32,
}

impl Default for CallerRate {
    fn default() -> Self {
        Self {
            requests_per_second: 10,
            burst: 20,
        }
    }
}

/// The overload limits, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overload {
    /// The most requests the listener serves at once; one past it is `503`.
    pub max_concurrent_requests: NonZeroU32,
    /// What a `503` past [`Overload::max_concurrent_requests`] asks in
    /// `Retry-After`, in whole seconds.
    pub retry_after: Duration,
    /// The per-caller rate limit, when `[server.caller_rate]` is set.
    pub caller_rate: Option<CallerRateSettings>,
}

/// The per-caller rate limit, resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallerRateSettings {
    /// The requests one caller may send per second, sustained.
    pub requests_per_second: NonZeroU32,
    /// The requests one caller may send at once.
    pub burst: NonZeroU32,
}

impl crate::config::server::Server {
    /// Resolves the overload limits of `[server]`.
    ///
    /// # Errors
    /// Returns [`Error::Zero`] naming the key when
    /// `server.max_concurrent_requests`, `server.overload_retry_after_s`,
    /// `server.caller_rate.requests_per_second` or `server.caller_rate.burst`
    /// is zero.
    pub fn resolve_overload(&self) -> Result<Overload, Error> {
        let zero = |key: &str| Error::Zero {
            key: key.to_owned(),
        };
        let max_concurrent_requests = NonZeroU32::new(self.max_concurrent_requests)
            .ok_or_else(|| zero("server.max_concurrent_requests"))?;
        let retry_after = NonZeroU32::new(self.overload_retry_after_s)
            .ok_or_else(|| zero("server.overload_retry_after_s"))?;
        let caller_rate = self
            .caller_rate
            .as_ref()
            .map(|rate| {
                Ok::<_, Error>(CallerRateSettings {
                    requests_per_second: NonZeroU32::new(rate.requests_per_second)
                        .ok_or_else(|| zero("server.caller_rate.requests_per_second"))?,
                    burst: NonZeroU32::new(rate.burst)
                        .ok_or_else(|| zero("server.caller_rate.burst"))?,
                })
            })
            .transpose()?;
        Ok(Overload {
            max_concurrent_requests,
            retry_after: Duration::from_secs(u64::from(retry_after.get())),
            caller_rate,
        })
    }
}
