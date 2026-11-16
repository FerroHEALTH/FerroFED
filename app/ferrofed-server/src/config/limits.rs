// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The overload limits of the listener: how many requests the gateway
//! serves at once, and how many one verified caller may send.
//!
//! A request past the concurrency limit is answered `503` with
//! `Retry-After` (RFC 9110 §15.6.4, §10.2.3), and one past a caller's rate
//! `429` with `Retry-After` (RFC 6585 §4). No specification governs the
//! limits themselves: our own design.

use std::num::NonZeroUsize;
use std::time::Duration;

use ferrohealth_server::section::{CallerRate, ServerSection};

use crate::config::error::Error;

/// The overload limits, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overload {
    /// The most requests the listener serves at once; one past it is `503`.
    pub max_concurrent_requests: NonZeroUsize,
    /// What a `503` past [`Overload::max_concurrent_requests`] asks in
    /// `Retry-After`, in whole seconds.
    pub retry_after: Duration,
    /// The per-caller rate limit, when `[server.caller_rate]` is set.
    pub caller_rate: Option<CallerRate>,
}

/// Resolves the overload limits of `[server]`.
///
/// # Errors
/// Returns [`Error::Zero`] when `server.retry_after` is zero.
pub fn resolve_overload(section: &ServerSection) -> Result<Overload, Error> {
    if section.retry_after.is_zero() {
        return Err(Error::Zero {
            key: String::from("server.retry_after"),
        });
    }
    Ok(Overload {
        max_concurrent_requests: section.max_concurrent_requests,
        retry_after: section.retry_after.get(),
        caller_rate: section.caller_rate,
    })
}
