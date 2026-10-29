// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The count of the requests the gateway served, by route template and
//! status class.
//!
//! The `OpenTelemetry` HTTP server metrics, `http.server.request.duration`
//! and `http.server.active_requests`, are the request log's of
//! `ferrohealth-telemetry`. [`REQUESTS`] counts the same requests, fed by
//! that log as a recorder. Every attribute value is drawn from a closed set:
//! the method from the methods the semantic conventions know, any other as
//! `_OTHER`; the route from the gateway's own route templates, the ITS-REST
//! operation templates of `openehr-its`, or the unmatched marker; the status
//! class from RFC 9110 §15. No path, query, header or body value reaches an
//! attribute (§5.4.1, N33).

use ferrohealth_telemetry::request_log::{Completed, Recorder, UNMATCHED, method_label};
use http::StatusCode;
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Meter};

/// The requests the gateway served, by method, route template and status
/// class; Prometheus `ferrofed_http_requests_total`.
pub const REQUESTS: &str = "ferrofed.http.requests";

/// The counter of the requests the gateway served.
#[derive(Debug, Clone)]
pub struct Requests {
    requests: Counter<u64>,
}

impl Requests {
    /// Creates the counter on `meter`.
    #[must_use]
    pub fn new(meter: &Meter) -> Self {
        Self {
            requests: meter
                .u64_counter(REQUESTS)
                .with_description("Requests the gateway served, by route template and status class")
                .build(),
        }
    }
}

impl Recorder for Requests {
    fn record(&self, completed: &Completed<'_>) {
        self.requests.add(
            1,
            &[
                KeyValue::new("http.request.method", method_label(completed.method)),
                KeyValue::new(
                    "http.route",
                    completed.route.unwrap_or(UNMATCHED).to_owned(),
                ),
                KeyValue::new("status_class", status_class(completed.status)),
            ],
        );
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
    use http::StatusCode;

    use super::status_class;

    #[test]
    fn each_status_falls_in_its_class() {
        assert_eq!("2xx", status_class(StatusCode::OK));
        assert_eq!("3xx", status_class(StatusCode::NOT_MODIFIED));
        assert_eq!("4xx", status_class(StatusCode::TOO_MANY_REQUESTS));
        assert_eq!("5xx", status_class(StatusCode::SERVICE_UNAVAILABLE));
        assert_eq!("1xx", status_class(StatusCode::CONTINUE));
    }
}
