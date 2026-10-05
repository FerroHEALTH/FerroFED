// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound request instruments: what the gateway's own clients see of
//! it, by route template and status.
//!
//! [`REQUEST_DURATION`] and [`ACTIVE_REQUESTS`] are the `OpenTelemetry` HTTP
//! server metrics, `http.server.request.duration` and
//! `http.server.active_requests`, with the attributes and the bucket
//! boundaries the semantic conventions name
//! (<https://opentelemetry.io/docs/specs/semconv/http/http-metrics/>).
//! [`REQUESTS`] counts the same requests by route template and status class.
//! Every attribute value is drawn from a closed set: the method from the
//! RFC 9110 §9 methods and `PATCH` (RFC 5789), any other as `_OTHER`; the
//! route from the gateway's own route templates, the ITS-REST operation
//! templates of `openehr-its`, or [`UNMATCHED`](crate::request_log::UNMATCHED);
//! the status from the codes the gateway answers. No path, query, header or
//! body value reaches an attribute (§5.4.1, N33).

use std::time::Duration;

use http::{Method, StatusCode};
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};

/// The duration of the requests the gateway served, by method, route
/// template and status; Prometheus `http_server_request_duration_seconds`.
pub const REQUEST_DURATION: &str = "http.server.request.duration";

/// The requests the gateway is serving now, by method; Prometheus
/// `http_server_active_requests`.
pub const ACTIVE_REQUESTS: &str = "http.server.active_requests";

/// The requests the gateway served, by method, route template and status
/// class; Prometheus `ferrofed_http_requests_total`.
pub const REQUESTS: &str = "ferrofed.http.requests";

/// The `http.request.method` value of a method no RFC the gateway knows
/// defines ([`method_label`]).
pub const OTHER_METHOD: &str = "_OTHER";

/// The bucket boundaries of [`REQUEST_DURATION`], in seconds: the advisory
/// boundaries of the semantic conventions, with `30` past the default
/// request timeout of 30 seconds.
pub const DURATION_BUCKETS: [f64; 15] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0, 30.0,
];

/// The scheme every request arrives over: the listener serves plain HTTP,
/// and TLS ends in front of it.
const SCHEME: &str = "http";

/// The inbound request instruments of one meter provider.
#[derive(Debug, Clone)]
pub struct Instruments {
    duration: Histogram<f64>,
    active: UpDownCounter<i64>,
    requests: Counter<u64>,
}

impl Instruments {
    /// Creates the instruments on `meter`.
    #[must_use]
    pub fn new(meter: &Meter) -> Self {
        Self {
            duration: meter
                .f64_histogram(REQUEST_DURATION)
                .with_description("Duration of HTTP server requests")
                .with_unit("s")
                .with_boundaries(DURATION_BUCKETS.to_vec())
                .build(),
            active: meter
                .i64_up_down_counter(ACTIVE_REQUESTS)
                .with_description("Number of active HTTP server requests")
                .with_unit("{request}")
                .build(),
            requests: meter
                .u64_counter(REQUESTS)
                .with_description("Requests the gateway served, by route template and status class")
                .build(),
        }
    }

    /// Counts a request of `method` as active until the returned guard is
    /// dropped, so a request whose client went away is counted out too.
    pub fn started(&self, method: &Method) -> Active {
        let labels = vec![
            KeyValue::new("http.request.method", method_label(method)),
            KeyValue::new("url.scheme", SCHEME),
        ];
        self.active.add(1, &labels);
        Active {
            active: self.active.clone(),
            labels,
        }
    }

    /// Records a request of `method` to `route` that the gateway answered
    /// with `status` after `elapsed`.
    pub fn served(&self, (method, route): (&Method, &str), status: StatusCode, elapsed: Duration) {
        let method = method_label(method);
        let mut labels = vec![
            KeyValue::new("http.request.method", method),
            KeyValue::new("url.scheme", SCHEME),
            KeyValue::new("http.route", route.to_owned()),
            KeyValue::new("http.response.status_code", i64::from(status.as_u16())),
        ];
        // NOTE: the semantic conventions set `error.type` to the status code of a
        // server error, and leave it unset on a request that succeeded.
        if status.is_server_error() {
            labels.push(KeyValue::new("error.type", status.as_str().to_owned()));
        }
        self.duration.record(elapsed.as_secs_f64(), &labels);
        self.requests.add(
            1,
            &[
                KeyValue::new("http.request.method", method),
                KeyValue::new("http.route", route.to_owned()),
                KeyValue::new("status_class", status_class(status)),
            ],
        );
    }
}

/// A request counted in [`ACTIVE_REQUESTS`], counted out when dropped.
#[derive(Debug)]
#[must_use = "the request is counted out when the guard is dropped"]
pub struct Active {
    active: UpDownCounter<i64>,
    labels: Vec<KeyValue>,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.active.add(-1, &self.labels);
    }
}

/// The `http.request.method` value of `method`: its name for a method RFC
/// 9110 §9 or RFC 5789 defines, `_OTHER` for any other, as the semantic
/// conventions ask, so a client cannot mint a label value.
#[must_use]
pub fn method_label(method: &Method) -> &'static str {
    match *method {
        Method::CONNECT => "CONNECT",
        Method::DELETE => "DELETE",
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        Method::PATCH => "PATCH",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::TRACE => "TRACE",
        _ => OTHER_METHOD,
    }
}

/// The class of `status`: `1xx` to `5xx` (RFC 9110 §15).
#[must_use]
pub fn status_class(status: StatusCode) -> &'static str {
    if status.is_informational() {
        "1xx"
    } else if status.is_success() {
        "2xx"
    } else if status.is_redirection() {
        "3xx"
    } else if status.is_client_error() {
        "4xx"
    } else {
        "5xx"
    }
}

#[cfg(test)]
mod tests {
    use http::{Method, StatusCode};

    use super::{method_label, status_class};

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_method_outside_the_registered_set_is_other() -> Result<(), http::method::InvalidMethod> {
        assert_eq!("GET", method_label(&Method::GET));
        assert_eq!("PATCH", method_label(&Method::PATCH));
        assert_eq!("_OTHER", method_label(&Method::from_bytes(b"SYNTHETIC-1")?));
        Ok(())
    }

    #[test]
    fn each_status_falls_in_its_class() {
        assert_eq!("2xx", status_class(StatusCode::OK));
        assert_eq!("3xx", status_class(StatusCode::NOT_MODIFIED));
        assert_eq!("4xx", status_class(StatusCode::TOO_MANY_REQUESTS));
        assert_eq!("5xx", status_class(StatusCode::SERVICE_UNAVAILABLE));
        assert_eq!("1xx", status_class(StatusCode::CONTINUE));
    }
}
