// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Overload protection at the listener: a bound on the requests served at
//! once, and an optional rate per verified caller, both of
//! `ferrohealth-server`.
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
//! verified, by its issuer and `client_id` ([`caller_key`]); a forwarded
//! address is never read, so a client cannot spread its load over addresses
//! it names.
//!
//! The per-member bound lives in the node dispatch
//! ([`ferrofed_engine::dispatch::cap`]). Every refusal is counted by its
//! limit ([`crate::metrics::nodes::Limit`], in [`crate::edge`]). No
//! specification governs the limits: our own design.

use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use ferrohealth_server::caller_rate::CallerKey;

use crate::auth::caller::Caller;

/// Names the verified caller of `request` to the caller rate of
/// `ferrohealth-server` ([`CallerKey`]); a request outside client
/// authentication carries no caller and is never limited.
pub async fn caller_key(mut request: Request, next: Next) -> Response {
    // NOTE: no specification governs this: our own design; the key is the
    // verified issuer and client, never an address a client or proxy names.
    let key = request.extensions().get::<Caller>().map(|caller| {
        CallerKey(Arc::from(format!(
            "{} {}",
            caller.issuer(),
            caller.client_id()
        )))
    });
    if let Some(key) = key {
        request.extensions_mut().insert(key);
    }
    next.run(request).await
}
