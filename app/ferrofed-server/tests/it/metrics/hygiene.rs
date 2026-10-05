// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! No label value comes from a request: a client that sends the patient
//! identifier in the query, a path and a header finds it nowhere in the
//! exposition, and every label is drawn from a closed set or from the
//! registry (§5.4.1, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;

use axum::body::Body;
use ferrofed_registry::incident::Kind;
use ferrofed_server::auth::refusal::Refusal;
use ferrofed_server::binding::ihe::metrics::FeedResult;
use ferrofed_server::metrics::ReloadResult;
use ferrofed_server::metrics::nodes::Limit;
use ferrofed_server::metrics::security::{CALLER_REFUSED, Event};
use http::{Request, header};
use openehr_federation::status::EndpointStatus;

use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, body, crossref, node_answering, patient_query,
    registry,
};
use crate::metrics::Metered;
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The labels the resource carries on `target_info`: the service and the
/// telemetry SDK, set by the gateway and the SDK, never by a request.
const RESOURCE: [&str; 5] = [
    "service_name",
    "service_version",
    "telemetry_sdk_language",
    "telemetry_sdk_name",
    "telemetry_sdk_version",
];

#[tokio::test]
async fn no_label_carries_what_a_request_sent() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let gateway = Metered::start(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        (
            "profile = \"development\"",
            &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        ),
        (2_000, 3_000),
    )?;
    let query = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-request-id", PATIENT)
        .header("openEHR-federation-endpoint", PATIENT)
        .body(Body::from(body(&patient_query())?))?;
    let read = Request::get(format!("/v1/ehr/{PATIENT}/ehr_status"))
        .header("x-request-id", PATIENT)
        .body(Body::empty())?;
    for request in [query, read] {
        call(gateway.app.clone(), request).await?;
    }

    let text = gateway.state.metrics().render()?;
    assert!(
        !text.contains(PATIENT_TAIL),
        "the identifier reaches no label: {text}"
    );
    assert!(
        !text.contains(NAMESPACE),
        "the namespace reaches no label: {text}"
    );
    let kinds: BTreeSet<&str> = Kind::ALL.iter().map(|kind| kind.as_str()).collect();
    let outcomes: BTreeSet<&str> = EndpointStatus::ALL.iter().map(|s| s.as_str()).collect();
    let results: BTreeSet<&str> = ReloadResult::ALL.iter().map(|r| r.as_str()).collect();
    let fed: BTreeSet<&str> = FeedResult::ALL.iter().map(|r| r.as_str()).collect();
    let endpoints = BTreeSet::from(["node-a-pub", "node-b-pub"]);
    let resolver: BTreeSet<&str> = ferrofed_server::metrics::resolver::Outcome::ALL
        .iter()
        .map(|outcome| outcome.as_str())
        .collect();
    let mut events: BTreeSet<&str> = Event::ALL.iter().map(|event| event.as_str()).collect();
    events.insert(CALLER_REFUSED);
    let reasons: BTreeSet<&str> = Refusal::ALL
        .iter()
        .map(|refusal| refusal.reason())
        .collect();
    let limits: BTreeSet<&str> = Limit::ALL.iter().map(|limit| limit.as_str()).collect();
    let methods = BTreeSet::from([
        "CONNECT", "DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT", "TRACE", "_OTHER",
    ]);
    let classes = BTreeSet::from(["1xx", "2xx", "3xx", "4xx", "5xx"]);
    // NOTE: §5.4.1, N33: the two routes are the templates of the two requests,
    // the read's path identifier written as its parameter name.
    let routes = BTreeSet::from(["/v1/query/aql", "/v1/ehr/{ehr_id}/ehr_status"]);
    for sample in gateway.scraped()? {
        for (key, value) in &sample.labels {
            let drawn = match (sample.name.as_str(), key.as_str()) {
                ("target_info", key) => RESOURCE.contains(&key),
                ("ferrofed_resolver_requests_total", "outcome") => {
                    resolver.contains(value.as_str())
                }
                ("ferrofed_security_events_total", "event") => events.contains(value.as_str()),
                ("ferrofed_security_events_total", "reason") => reasons.contains(value.as_str()),
                (_, "limit") => limits.contains(value.as_str()),
                (_, "http_request_method") => methods.contains(value.as_str()),
                (_, "url_scheme") => value == "http",
                (_, "http_route") => routes.contains(value.as_str()),
                (_, "http_response_status_code") => value.parse::<u16>().is_ok(),
                (_, "error_type") => value.parse::<u16>().is_ok_and(|status| status >= 500),
                (_, "status_class") => classes.contains(value.as_str()),
                (_, "kind") => kinds.contains(value.as_str()),
                (_, "outcome") => outcomes.contains(value.as_str()),
                ("ferrofed_identity_feed_messages_total", "result") => fed.contains(value.as_str()),
                (_, "result") => results.contains(value.as_str()),
                (_, "endpoint") => endpoints.contains(value.as_str()),
                (_, "le") => value == "+Inf" || value.parse::<f64>().is_ok(),
                _ => false,
            };
            assert!(drawn, "{}: {key}={value:?} is no closed label", sample.name);
        }
    }
    Ok(())
}
