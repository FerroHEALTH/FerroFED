// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Overload protection at the listener: past `server.max_concurrent_requests`
//! a request is `503 overloaded` with `Retry-After` (RFC 9110 §15.6.4,
//! §10.2.3) and the health family still answers; past `[server.caller_rate]`
//! a verified caller is `429 rate-limited` with `Retry-After` (RFC 6585 §4),
//! keyed on the caller and never on a forwarded address; past
//! `federation.max_in_flight_per_node` a member is `time-out` (§11.5, N38).
//! Every refusal is counted by its limit. No specification governs the
//! limits: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::limits::CallerRateSettings;
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use openehr_federation::headers::COMPLETENESS;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{EHR_A, EHR_B, body, crossref, node_answering, patient_query, registry};
use crate::metrics::{count, parse};
use crate::support::{call, claims, error_body, issuer, send, settings};

type TestResult = Result<(), Box<dyn Error>>;

/// The refusal counter's Prometheus name.
const REFUSALS: &str = "ferrofed_overload_refusals_total";

/// How long the slow member keeps a query.
const HOLD: Duration = Duration::from_millis(1_500);

/// A member answering the query with one row after `delay`.
async fn node_after(delay: Duration) -> Server {
    let server = Server::start().await;
    let answer = r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-a::cdr-a.example.org::1"]]}"##;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(answer.as_bytes().to_vec(), "application/json")
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

/// A development gateway over members `a` and `b`, with the `[federation]`
/// keys `federation` added, served under `server`.
fn gateway(
    dir: &Path,
    (a, b): (&str, &str),
    federation: &str,
    server: &ServerSettings,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n{federation}\n\n{}",
        crossref(&[("node-a", EHR_A), ("node-b", EHR_B)])
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    let state = Arc::new(AppState::with_federation(federation));
    Ok((ferrofed_server::router(Arc::clone(&state), server), state))
}

/// The middleware settings with room for the fan-out budget.
fn roomy() -> ServerSettings {
    let mut server = settings();
    server.request_timeout = Duration::from_secs(10);
    server.body_limit = 64 * 1024;
    server
}

/// A federated query of the patient, best effort.
fn query() -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .header(COMPLETENESS, "partial")
        .body(Body::from(body(&patient_query())?))?)
}

/// Waits until `server` received `count` requests, or fails after a bound.
async fn until_received(server: &Server, count: usize) -> TestResult {
    for _ in 0..500 {
        if server.received_requests().await.unwrap_or_default().len() >= count {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("the member never received {count} requests").into())
}

/// The `Authorization` value of a caller whose client is `client_id`.
fn bearer_of(client_id: &str) -> Result<String, Box<dyn Error>> {
    let mut claims = claims();
    client_id.clone_into(&mut claims.client_id);
    Ok(format!("Bearer {}", issuer().mint(&claims)?))
}

#[tokio::test]
async fn a_request_past_the_concurrency_limit_is_503_with_retry_after() -> TestResult {
    let slow = node_after(HOLD).await;
    let quick = node_answering("uid-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let mut server = roomy();
    server.overload.max_concurrent_requests = NonZeroU32::new(1).ok_or("one is positive")?;
    server.overload.retry_after = Duration::from_secs(7);
    let (app, state) = gateway(dir.path(), (&slow.uri(), &quick.uri()), "", &server)?;
    let held = tokio::spawn({
        let (app, request) = (app.clone(), query()?);
        async move { call(app, request).await.map_err(|error| error.to_string()) }
    });
    until_received(&slow, 1).await?;

    let (status, text) = call(app.clone(), query()?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert_eq!("overloaded", error_body(&text)?.code);
    let response = send(app.clone(), query()?).await?;
    assert_eq!(
        Some("7"),
        response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
        "Retry-After names the configured seconds (RFC 9110 §10.2.3)"
    );
    let (health, _) = call(app.clone(), Request::get("/health").body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, health, "the health family is never refused");
    assert_eq!(
        1,
        slow.received_requests().await.unwrap_or_default().len(),
        "a refused request reaches no member"
    );

    let (first, text) = held.await??;
    assert_eq!(
        StatusCode::OK,
        first,
        "the request holding the slot is served: {text}"
    );
    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("2".to_owned()),
        count(&samples, REFUSALS, &[("limit", "concurrency")]),
        "{samples:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_caller_past_its_rate_is_429_and_another_caller_is_not() -> TestResult {
    let mut server = settings();
    server.overload.caller_rate = Some(CallerRateSettings {
        requests_per_second: NonZeroU32::new(1).ok_or("one is positive")?,
        burst: NonZeroU32::new(1).ok_or("one is positive")?,
    });
    let state = Arc::new(AppState::default());
    let app = ferrofed_server::router(Arc::clone(&state), &server);
    let request = |client: &str, forwarded: &str| -> Result<Request<Body>, Box<dyn Error>> {
        Ok(Request::get("/v1/query/aql?q=SELECT%201")
            .header(header::AUTHORIZATION, bearer_of(client)?)
            .header("x-forwarded-for", forwarded)
            .body(Body::empty())?)
    };

    let (first, text) = call(app.clone(), request("client-one", "192.0.2.1")?).await?;
    assert_ne!(
        StatusCode::TOO_MANY_REQUESTS,
        first,
        "the first request is admitted: {text}"
    );
    let response = send(app.clone(), request("client-one", "192.0.2.2")?).await?;
    assert_eq!(
        StatusCode::TOO_MANY_REQUESTS,
        response.status(),
        "another forwarded address is the same caller"
    );
    let retry = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or("a 429 carries Retry-After in seconds")?;
    assert!(
        (1..=2).contains(&retry),
        "a whole second, rounded up: {retry}"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    assert_eq!(
        "rate-limited",
        error_body(std::str::from_utf8(&bytes)?)?.code
    );
    let (other, text) = call(app.clone(), request("client-two", "192.0.2.2")?).await?;
    assert_ne!(
        StatusCode::TOO_MANY_REQUESTS,
        other,
        "another caller has a bucket of its own: {text}"
    );

    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, REFUSALS, &[("limit", "caller-rate")]),
        "{samples:?}"
    );
    Ok(())
}

#[tokio::test]
async fn without_a_caller_rate_no_caller_is_limited() -> TestResult {
    let app = ferrofed_server::router(Arc::new(AppState::default()), &settings());
    for _ in 0..5 {
        let (status, text) = call(
            app.clone(),
            Request::get("/v1/query/aql?q=SELECT%201").body(Body::empty())?,
        )
        .await?;
        assert_ne!(StatusCode::TOO_MANY_REQUESTS, status, "{text}");
    }
    Ok(())
}

// NOTE: §11.5, N38: the member past its cap is abandoned at its per-node
// deadline and reported time-out, which clears complete under partial (§11.4).
#[tokio::test]
async fn a_member_past_its_in_flight_cap_is_time_out_and_counted() -> TestResult {
    let slow = node_after(Duration::from_millis(2_500)).await;
    let quick = node_answering("uid-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(
        dir.path(),
        (&slow.uri(), &quick.uri()),
        "max_in_flight_per_node = 1",
        &roomy(),
    )?;
    let held = tokio::spawn({
        let (app, request) = (app.clone(), query()?);
        async move { call(app, request).await.map_err(|error| error.to_string()) }
    });
    until_received(&slow, 1).await?;

    // NOTE: §11.5 client-deadline: the shorter budget ends this request's wait
    // for member A before the request holding its slot is abandoned.
    let mut shorter = query()?;
    shorter
        .headers_mut()
        .insert(header::HeaderName::from_static("prefer"), "wait=1".parse()?);
    let (status, text) = call(app.clone(), shorter).await?;
    assert_eq!(StatusCode::OK, status, "a partial answer: {text}");
    assert!(
        text.contains("in-flight cap"),
        "member A is time-out for its cap: {text}"
    );
    assert_eq!(
        1,
        slow.received_requests().await.unwrap_or_default().len(),
        "the capped request never reached member A"
    );
    held.await??;

    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(
            &samples,
            REFUSALS,
            &[("limit", "node-in-flight"), ("endpoint", "node-a-pub")]
        ),
        "{samples:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_zero_limit_is_refused_by_its_key() -> TestResult {
    for (text, key) in [
        (
            "[server]\nmax_concurrent_requests = 0\n",
            "server.max_concurrent_requests",
        ),
        (
            "[server]\noverload_retry_after_s = 0\n",
            "server.overload_retry_after_s",
        ),
        (
            "[server.caller_rate]\nrequests_per_second = 0\n",
            "server.caller_rate.requests_per_second",
        ),
        (
            "[server.caller_rate]\nburst = 0\n",
            "server.caller_rate.burst",
        ),
        (
            "[federation]\nmax_in_flight_per_node = 0\n",
            "federation.max_in_flight_per_node",
        ),
    ] {
        let refused = Config::from_sources(Some(text), &BTreeMap::new())?
            .resolve()
            .err()
            .ok_or_else(|| format!("{key} = 0 is refused"))?;
        assert!(refused.to_string().contains(key), "{key}: {refused}");
    }
    Ok(())
}
