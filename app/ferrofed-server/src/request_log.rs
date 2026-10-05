// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One log line per request: the method, the matched route, the status, the
//! latency, the gateway's request id and whether the client named its own.
//!
//! A façade query carries the patient identifier, in the AQL text, in a query
//! parameter or in a body (§5.4.1), and §5.4.3 says the identifier value MUST
//! NOT be logged. So the line carries none of the places it can travel: never
//! a body, never the raw path, never a header value, and never a query value
//! outside [`LOGGED_QUERY_PARAMETERS`]. The route is a path template
//! ([`route`]): a request under `{base}/v1/` that no route of the gateway's
//! own matched is logged under the template of the ITS-REST operation it
//! addresses, so a routed `ehr_id` or version uid stays out of the line, and
//! a path that names no route is logged as [`UNMATCHED`].
//!
//! The logged `request_id` is the gateway's own [`OutboundId`], the one every
//! node the request reaches receives. The client's `x-request-id` is free text
//! that may name a patient, so it is echoed to the client and never logged
//! ([`request_id`]); `client_named` says only whether the client sent one.
//! Past §5.4.3, no specification governs this: our own design, made
//! mechanical.
//!
//! The same middleware opens the `request` span every other span of the
//! request sits under, with the same fields as the line: the method, the
//! route template, the status and the gateway's request id. It is the root
//! of a trace of the gateway's own; a client's `traceparent` and
//! `tracestate` are never read ([`ferrofed_engine::trace_context`]).
//!
//! [`OutboundId`]: ferrofed_engine::outbound_id::OutboundId

use axum::extract::{MatchedPath, OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::Method;
use openehr_its::rest::routes::{self, Lookup};
use std::sync::Arc;
use std::time::Instant;
use tracing::Instrument as _;
use tracing::field::Empty;

use crate::base_path::BasePath;
use crate::metrics::inbound::Instruments;
use crate::{ITS_REST_PREFIX, request_id};

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

/// What the request log reads: the deployment's base path, and the inbound
/// request instruments it records each request through, when metered.
#[derive(Debug)]
pub struct RequestLog {
    base: BasePath,
    inbound: Option<Instruments>,
}

impl RequestLog {
    /// Returns the log of the surface under `base`, recording through
    /// `inbound` when it is set.
    #[must_use]
    pub fn new(base: BasePath, inbound: Option<Instruments>) -> Self {
        Self { base, inbound }
    }
}

/// Logs `request` after it completes and returns its response untouched,
/// naming its route under the deployment's `base` path ([`route`]).
///
/// The same method, route template and status are recorded in the inbound
/// request metrics ([`crate::metrics::inbound`]), and nothing else of the
/// request.
pub async fn log(State(log): State<Arc<RequestLog>>, request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let route = route(&log.base, &request);
    let active = log.inbound.as_ref().map(|inbound| inbound.started(&method));
    let query = paging_parameters(request.uri().query().unwrap_or_default());
    let id = request_id::outbound(request.extensions())
        .map(|id| id.to_string())
        .unwrap_or_default();
    // NOTE: no specification governs this: our own design, an exchange id
    // other than the outbound id is one the client chose, logged as a flag.
    let client_named = request_id::of(request.headers()).is_some_and(|echoed| echoed != id);
    let span = tracing::info_span!(
        "request",
        otel.name = %format_args!("{method} {route}"),
        otel.kind = "server",
        http.request.method = method.as_str(),
        http.route = route.as_str(),
        http.response.status_code = Empty,
        otel.status_code = Empty,
        request_id = id.as_str(),
    );
    let started = Instant::now();
    let response = next.run(request).instrument(span.clone()).await;
    let status = response.status();
    span.record("http.response.status_code", status.as_u16());
    if status.is_server_error() {
        span.record("otel.status_code", "ERROR");
    }
    let elapsed = started.elapsed();
    if let Some(inbound) = &log.inbound {
        inbound.served((&method, &route), status, elapsed);
    }
    drop(active);
    let latency_ms = elapsed.as_secs_f64() * 1000.0;
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
            client_named,
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
            client_named,
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
            client_named,
            "request"
        );
    }
    response
}

/// Returns the route `request` is logged under.
///
/// That is the path template a route of the gateway's own matched, or else
/// the template of the ITS-REST operation a path under `{base}/v1/`
/// addresses, under that prefix, or [`UNMATCHED`]. The ITS-REST template is
/// `openehr-its`'s `routes::lookup`, the table the gateway routes by, which
/// names each identifier by its parameter (`/ehr/{ehr_id}`) and never by its
/// value. The path is read as the client sent it, before any prefix is
/// stripped, so `base` is named once.
#[must_use]
pub fn route(base: &BasePath, request: &Request) -> String {
    if let Some(matched) = request.extensions().get::<MatchedPath>() {
        return matched.as_str().to_owned();
    }
    let path = request
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| request.uri().path(), |original| original.path());
    its_rest_template(base, request.method(), path).unwrap_or_else(|| UNMATCHED.to_owned())
}

/// The template of the ITS-REST operation `method` and `path` address under
/// `{base}/v1`, with that prefix, or `None` when `path` is outside it or
/// names no operation that declares `method`.
///
/// An `OPTIONS` request describes a resource, which ITS-REST declares no
/// `OPTIONS` operation for, so it is logged under the template of the
/// resource its path names, read through any method declared there.
fn its_rest_template(base: &BasePath, method: &Method, path: &str) -> Option<String> {
    let prefix = base.join(ITS_REST_PREFIX.trim_end_matches('/'));
    let relative = path
        .strip_prefix(prefix.as_str())
        .filter(|relative| relative.starts_with('/'))?;
    let template = match routes::lookup(method, relative) {
        Lookup::Matched(matched) => matched.template,
        Lookup::MethodNotAllowed { allowed } if *method == Method::OPTIONS => {
            resource_template(relative, &allowed)?
        }
        Lookup::MethodNotAllowed { .. } | Lookup::NotFound => return None,
    };
    Some(format!("{prefix}{template}"))
}

/// The template of the resource at `path`, read through the first of its
/// `allowed` methods whose operation the route table matches.
fn resource_template(path: &str, allowed: &[&str]) -> Option<&'static str> {
    allowed.iter().find_map(|name| {
        // NOTE: RFC 9110 §9.1, a declared name that is no method token names no
        // operation, so it is passed over for the next declared method.
        let method = Method::from_bytes(name.as_bytes()).ok()?;
        match routes::lookup(&method, path) {
            Lookup::Matched(matched) => Some(matched.template),
            Lookup::MethodNotAllowed { .. } | Lookup::NotFound => None,
        }
    })
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
