// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the gateway's own clients see of it: every request counted and timed
//! by route template and status, the resolver's calls by outcome, and the
//! security events by kind, none carrying a value a request sent (§5.4.1,
//! N33). No specification governs metrics: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode, header};

use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, body, crossref, node_answering, patient_query,
    registry,
};
use crate::metrics::{Metered, count, value};
use crate::support::{call, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// The inbound series' Prometheus names.
const REQUESTS: &str = "ferrofed_http_requests_total";
const DURATION_COUNT: &str = "http_server_request_duration_seconds_count";
const ACTIVE: &str = "http_server_active_requests";
const SECURITY: &str = "ferrofed_security_events_total";
const RESOLVER: &str = "ferrofed_resolver_requests_total";
const RESOLVER_DURATION_COUNT: &str = "ferrofed_resolver_request_duration_seconds_count";

/// A development gateway over two answering members that both know the
/// patient.
async fn gateway(
    dir: &std::path::Path,
) -> Result<(Metered, [ferrofed_testkit::mock::Server; 2]), Box<dyn Error>> {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let gateway = Metered::start(
        dir,
        &registry(&a.uri(), &b.uri(), ""),
        (
            "profile = \"development\"",
            &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        ),
        (2_000, 3_000),
    )?;
    Ok((gateway, [a, b]))
}

/// A `POST {base}/v1/query/aql` of `aql`.
fn query(aql: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body(aql)?))?)
}

#[tokio::test]
async fn each_request_is_counted_and_timed_by_route_template_and_status() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (gateway, _nodes) = gateway(dir.path()).await?;
    let (status, text) = call(gateway.app.clone(), query(&patient_query())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let unknown = Request::get(format!("/v1/no-such-area/{PATIENT}")).body(Body::empty())?;
    call(gateway.app.clone(), unknown).await?;

    let samples = gateway.scraped()?;
    assert_eq!(
        Some("1".to_owned()),
        count(
            &samples,
            REQUESTS,
            &[
                ("http_request_method", "POST"),
                ("http_route", "/v1/query/aql"),
                ("status_class", "2xx"),
            ]
        ),
        "{samples:?}"
    );
    assert_eq!(
        Some("1".to_owned()),
        count(
            &samples,
            DURATION_COUNT,
            &[
                ("http_route", "/v1/query/aql"),
                ("http_response_status_code", "200"),
                ("url_scheme", "http"),
            ]
        ),
    );
    assert_eq!(
        Some("1".to_owned()),
        count(
            &samples,
            REQUESTS,
            &[("http_route", "<unmatched>"), ("status_class", "5xx")]
        ),
        "a path under {{base}}/v1/ ITS-REST does not define answers 501 (N32), counted under no \
         path of its own: {samples:?}"
    );
    let active: f64 = samples
        .iter()
        .filter(|sample| sample.name == ACTIVE)
        .map(|sample| sample.value)
        .sum();
    assert_eq!(
        "0",
        active.to_string(),
        "no request is in flight once answered"
    );
    let text = gateway.state.metrics().render()?;
    assert!(
        !text.contains(PATIENT_TAIL),
        "no label carries the path: {text}"
    );
    assert!(
        !text.contains(NAMESPACE),
        "no label carries the namespace: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn each_resolver_call_is_counted_by_outcome_and_timed() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (gateway, _nodes) = gateway(dir.path()).await?;
    let (status, text) = call(gateway.app.clone(), query(&patient_query())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");

    let samples = gateway.scraped()?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, RESOLVER, &[("outcome", "resolved")]),
        "{samples:?}"
    );
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, RESOLVER_DURATION_COUNT, &[]),
    );
    Ok(())
}

#[tokio::test]
async fn a_caller_refused_at_authentication_is_counted_by_its_reason() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (gateway, _nodes) = gateway(dir.path()).await?;
    let labels = [("event", "caller-refused"), ("reason", "missing")];
    let before = value(&gateway.scraped()?, SECURITY, &labels).ok_or("the series is at 0")?;
    let response = send_as_is(gateway.app.clone(), query(&patient_query())?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());

    let after = value(&gateway.scraped()?, SECURITY, &labels).ok_or("the series is counted")?;
    assert_eq!((before + 1.0).to_string(), after.to_string());
    Ok(())
}

#[tokio::test]
async fn a_query_refused_before_dispatch_is_counted_as_its_event() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (gateway, _nodes) = gateway(dir.path()).await?;
    let labels = [("event", "aql-refused")];
    let before = value(&gateway.scraped()?, SECURITY, &labels).ok_or("the series is at 0")?;
    let second = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/id/value = 'SENTINEL-OTHER-71xz' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let (status, text) = call(gateway.app.clone(), query(&second)?).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        status,
        "a second subject is refused: {text}"
    );

    let after = value(&gateway.scraped()?, SECURITY, &labels).ok_or("the series is counted")?;
    assert_eq!((before + 1.0).to_string(), after.to_string());
    let text = gateway.state.metrics().render()?;
    assert!(
        !text.contains(PATIENT_TAIL),
        "no label carries the identifier: {text}"
    );
    Ok(())
}
