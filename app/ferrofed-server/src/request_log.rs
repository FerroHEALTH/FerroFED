// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The request log of the gateway: `ferrohealth-telemetry`'s, with the query
//! parameters it may carry and the route it names a request no route matched.
//!
//! One line per request carries the method, the matched route, the status,
//! the latency, the gateway's request id and whether the client named its
//! own. A façade query carries the patient identifier, in the AQL text, in a
//! query parameter or in a body (§5.4.1), and §5.4.3 says the identifier
//! value MUST NOT be logged. So the line carries none of the places it can
//! travel: never a body, never the raw path, never a header value, and never
//! a query value outside [`LOGGED_QUERY_PARAMETERS`], each of up to
//! [`MAX_VALUE_LENGTH`] digits. The route is a path template: a request
//! under `{base}/v1/` that no route of the gateway's own matched is logged
//! under the template of the ITS-REST operation it addresses
//! ([`ItsRestRoutes`]), so a routed `ehr_id` or version uid stays out of the
//! line, and a path that names no route is logged as unmatched.
//!
//! The logged `request_id` is the gateway's own [`OutboundId`], the one every
//! node the request reaches receives ([`crate::request_id`]). The client's
//! `x-request-id` is free text that may name a patient, so it is echoed to
//! the client and never logged; `client_named` says only whether the client
//! sent one. The same log opens the `request` span every other span of the
//! request sits under, the root of a trace of the gateway's own. Past
//! §5.4.3, no specification governs this: our own design, made mechanical.
//!
//! [`OutboundId`]: ferrofed_engine::outbound_id::OutboundId

use std::sync::Arc;

use ferrohealth_telemetry::request_log::{RequestLog, RouteNamer, ValueShape};
use http::Method;
use openehr_its::rest::routes::{self, Lookup};

use crate::ITS_REST_PREFIX;
use crate::base_path::BasePath;

/// The query parameters whose values may reach the log.
///
/// The ITS-REST paging parameters `offset` and `fetch` are row counts and
/// nothing else, and a value is logged only when it is all digits, so even a
/// misuse of either name cannot carry an identifier into the log. No other
/// parameter is ever logged: `q` is the AQL text, and every other name may be
/// an AQL query parameter carrying the patient identifier. The list is fixed in
/// code, not configuration, so a deployment cannot widen it.
pub const LOGGED_QUERY_PARAMETERS: [&str; 2] = ["offset", "fetch"];

/// The most digits a logged query value may have.
pub const MAX_VALUE_LENGTH: usize = 10;

/// Returns the request log of the surface under `base`: the paging values
/// alone, and the ITS-REST template of a request no route matched.
#[must_use]
pub fn of(base: BasePath) -> RequestLog {
    RequestLog::new(&LOGGED_QUERY_PARAMETERS.map(String::from))
        .with_default_query_shape(ValueShape::Digits {
            max: MAX_VALUE_LENGTH,
        })
        .with_route_namer(Arc::new(ItsRestRoutes::new(base)))
}

/// Names a request under `{base}/v1/` that no route of the gateway's own
/// matched by the template of the ITS-REST operation it addresses.
///
/// The template is `openehr-its`'s `routes::lookup`, the table the gateway
/// routes by, which names each identifier by its parameter
/// (`/ehr/{ehr_id}`) and never by its value. The path is read as the client
/// sent it, before any prefix is stripped, so the base is named once.
#[derive(Debug, Clone)]
pub struct ItsRestRoutes {
    base: BasePath,
}

impl ItsRestRoutes {
    /// The namer of the surface under `base`.
    #[must_use]
    pub const fn new(base: BasePath) -> Self {
        Self { base }
    }
}

impl RouteNamer for ItsRestRoutes {
    fn route(&self, method: &Method, path: &str) -> Option<String> {
        its_rest_template(&self.base, method, path)
    }
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
