// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One log line per request: the method, the matched route, the status, the
//! latency and the request id.
//!
//! A façade query carries the patient identifier, in the AQL text, in a query
//! parameter or in a body (§5.4.1), and §5.4.3 says the identifier value MUST
//! NOT be logged. So the line carries none of the places it can travel: never
//! a body, never the raw path (the matched route is the path template, and a
//! path no route matched is logged as [`UNMATCHED`]), never a header value
//! other than the request id this server has already normalized, and never a
//! query value outside [`LOGGED_QUERY_PARAMETERS`]. Past §5.4.3, no
//! specification governs this: our own design, made mechanical.

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;

use crate::request_id;

/// The route a request is logged under when no route matched it.
pub const UNMATCHED: &str = "<unmatched>";

/// The query parameters whose values may reach the log.
///
/// The ITS-REST paging parameters `offset` and `fetch` are row counts and
/// nothing else, and a value is logged only when it is all digits, so even a
/// misuse of either name cannot carry an identifier into the log. No other
/// parameter is ever logged: `q` is the AQL text, and every other name may be
/// an AQL query parameter carrying the patient identifier. The list is fixed in
/// code, not configuration, so a deployment cannot widen it.
pub const LOGGED_QUERY_PARAMETERS: [&str; 2] = ["offset", "fetch"];

/// The longest logged query value.
pub const MAX_VALUE_LENGTH: usize = 10;

/// Logs `request` after it completes and returns its response untouched.
pub async fn log(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let route = request.extensions().get::<MatchedPath>().map_or_else(
        || UNMATCHED.to_owned(),
        |matched| matched.as_str().to_owned(),
    );
    let query = paging_parameters(request.uri().query().unwrap_or_default());
    let id = request_id::of(request.headers())
        .unwrap_or_default()
        .to_owned();
    let started = Instant::now();
    let response = next.run(request).await;
    let status = response.status();
    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (method, route, query, request_id) =
        (method.as_str(), route.as_str(), query.as_str(), id.as_str());
    let status_code = status.as_u16();
    if status.is_server_error() {
        tracing::error!(
            method,
            route,
            status = status_code,
            latency_ms,
            query,
            request_id,
            "request"
        );
    } else if status.is_client_error() {
        tracing::warn!(
            method,
            route,
            status = status_code,
            latency_ms,
            query,
            request_id,
            "request"
        );
    } else {
        tracing::info!(
            method,
            route,
            status = status_code,
            latency_ms,
            query,
            request_id,
            "request"
        );
    }
    response
}

/// Returns the `key=value` pairs of `query` this server may log.
///
/// A pair is kept when its key is in [`LOGGED_QUERY_PARAMETERS`] and its value
/// is one to [`MAX_VALUE_LENGTH`] ASCII digits. The pairs keep the query's own
/// order, separated by a space; every other pair contributes nothing, key
/// included.
fn paging_parameters(query: &str) -> String {
    let mut out = String::new();
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let digits = !value.is_empty()
            && value.len() <= MAX_VALUE_LENGTH
            && value.bytes().all(|byte| byte.is_ascii_digit());
        if !digits || !LOGGED_QUERY_PARAMETERS.contains(&key) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(key);
        out.push('=');
        out.push_str(value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{MAX_VALUE_LENGTH, paging_parameters};

    #[test]
    fn only_the_paging_parameters_with_digit_values_are_kept() {
        assert_eq!(
            "offset=20 fetch=10",
            paging_parameters("q=SELECT&offset=20&subject=1234&fetch=10")
        );
    }

    #[test]
    fn a_paging_name_carrying_anything_but_digits_is_dropped() {
        assert_eq!("", paging_parameters("offset=SYNTHETIC-1"));
        assert_eq!("", paging_parameters("fetch=1%202"));
        assert_eq!(
            "",
            paging_parameters(&format!("fetch={}", "9".repeat(MAX_VALUE_LENGTH + 1)))
        );
    }

    #[test]
    fn a_pair_without_a_value_and_an_empty_query_contribute_nothing() {
        assert_eq!("", paging_parameters("fetch"));
        assert_eq!("", paging_parameters("fetch="));
        assert_eq!("", paging_parameters(""));
    }
}
