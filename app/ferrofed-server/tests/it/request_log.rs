// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The request log: one line per request with the method, the matched route,
//! the status, the latency and the gateway's own request id, and nothing a
//! client sent beyond that: not even the client's `x-request-id` (§5.4.3).

use crate::support::{self, Logs, request_lines};
use axum::Router;
use axum::body::Body;
use ferrofed_server::request_id;
use ferrofed_server::request_log::UNMATCHED;
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::{Request, StatusCode};
use std::error::Error as StdError;
use tower::ServiceExt as _;

/// Sends every request in `requests` through `app` under a capturing JSON
/// subscriber at `filter`, and returns what was logged.
pub(crate) fn logged(
    app: &Router,
    filter: &str,
    requests: Vec<Request<Body>>,
) -> Result<String, Box<dyn StdError>> {
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, filter, false, logs.clone())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    tracing::subscriber::with_default(capture, || {
        runtime.block_on(async {
            for request in requests {
                app.clone().oneshot(request).await?;
            }
            Ok::<(), Box<dyn StdError>>(())
        })
    })?;
    Ok(logs.text())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_request_line_carries_the_method_the_route_the_status_the_latency_and_the_id()
-> Result<(), Box<dyn StdError>> {
    let text = logged(
        &support::app(),
        "info",
        vec![
            Request::get("/health")
                .header(request_id::HEADER, "corr-log")
                .body(Body::empty())?,
        ],
    )?;
    let lines = request_lines(&text)?;
    assert_eq!(1, lines.len(), "one line per request: {text}");
    let line = lines.first().ok_or("one line")?;
    assert_eq!(Some("GET"), line.method.as_deref());
    assert_eq!(Some("/health"), line.route.as_deref());
    assert_eq!(Some(StatusCode::OK.as_u16()), line.status);
    assert!(line.latency_ms.is_some_and(|ms| ms >= 0.0), "{text}");
    let logged_id = line.request_id.as_deref().ok_or("an id is logged")?;
    assert_eq!(
        Some(uuid::Version::Random),
        uuid::Uuid::parse_str(logged_id)?.get_version(),
        "the logged id is the gateway's own: {text}"
    );
    assert_eq!(Some(true), line.client_named, "{text}");
    assert_eq!(Some(""), line.query.as_deref());
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_client_request_id_is_echoed_and_never_logged() -> Result<(), Box<dyn StdError>> {
    let client_id = "SYNTHETIC-NATIONAL-ID-0002";
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let response = runtime.block_on(support::send(
        support::app(),
        Request::get("/health")
            .header(request_id::HEADER, client_id)
            .body(Body::empty())?,
    ))?;
    assert_eq!(
        Some(client_id),
        response
            .headers()
            .get(request_id::HEADER)
            .and_then(|value| value.to_str().ok()),
        "the client's own id names the exchange"
    );
    let text = logged(
        &support::app(),
        "trace",
        vec![
            Request::get("/health")
                .header(request_id::HEADER, client_id)
                .body(Body::empty())?,
            Request::get("/v1/ehr")
                .header(request_id::HEADER, client_id)
                .body(Body::empty())?,
        ],
    )?;
    assert_eq!(2, request_lines(&text)?.len(), "both were logged: {text}");
    assert!(
        !text.contains(client_id),
        "the client's id reached the log: {text}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_unnamed_request_logs_the_id_its_response_carries() -> Result<(), Box<dyn StdError>> {
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let response = tracing::subscriber::with_default(capture, || {
        runtime.block_on(support::send(
            support::app(),
            Request::get("/health").body(Body::empty())?,
        ))
    })?;
    let answered = response
        .headers()
        .get(request_id::HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or("every response carries an id")?;
    let text = logs.text();
    let line = request_lines(&text)?
        .into_iter()
        .next()
        .ok_or("one request line")?;
    assert_eq!(Some(answered), line.request_id.as_deref(), "{text}");
    assert_eq!(Some(false), line.client_named, "{text}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_path_no_route_matched_is_logged_as_unmatched_never_raw() -> Result<(), Box<dyn StdError>> {
    let text = logged(
        &support::app(),
        "info",
        vec![Request::get("/v1/ehr/SYNTHETIC-PATH-ID/composition").body(Body::empty())?],
    )?;
    let line = request_lines(&text)?
        .into_iter()
        .next()
        .ok_or("one request line")?;
    assert_eq!(Some(UNMATCHED), line.route.as_deref());
    assert_eq!(Some(StatusCode::NOT_IMPLEMENTED.as_u16()), line.status);
    assert!(!text.contains("SYNTHETIC-PATH-ID"), "{text}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn only_the_paging_parameters_reach_the_log_and_only_as_digits() -> Result<(), Box<dyn StdError>> {
    let text = logged(
        &support::app(),
        "info",
        vec![
            Request::get("/v1/query/aql?q=SYNTHETIC-AQL&offset=20&fetch=10&patient=SYNTHETIC-P")
                .body(Body::empty())?,
        ],
    )?;
    let line = request_lines(&text)?
        .into_iter()
        .next()
        .ok_or("one request line")?;
    assert_eq!(Some("offset=20 fetch=10"), line.query.as_deref());
    assert!(!text.contains("SYNTHETIC-AQL"), "{text}");
    assert!(!text.contains("SYNTHETIC-P"), "{text}");
    assert!(
        !text.contains("patient"),
        "an unlisted name is not logged: {text}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn no_body_and_no_header_value_reaches_the_log() -> Result<(), Box<dyn StdError>> {
    let text = logged(
        &support::app(),
        "info",
        vec![
            Request::post("/v1/query/aql")
                .header("x-synthetic", "SYNTHETIC-HEADER-VALUE")
                .header(http::header::AUTHORIZATION, "Bearer SYNTHETIC-TOKEN")
                .body(Body::from("SYNTHETIC-BODY-CONTENT"))?,
        ],
    )?;
    assert_eq!(
        1,
        request_lines(&text)?.len(),
        "the request was logged: {text}"
    );
    for value in [
        "SYNTHETIC-HEADER-VALUE",
        "SYNTHETIC-TOKEN",
        "SYNTHETIC-BODY-CONTENT",
    ] {
        assert!(!text.contains(value), "{value} reached the log: {text}");
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_server_error_logs_at_error_and_a_client_error_at_warn() -> Result<(), Box<dyn StdError>> {
    let error_only = logged(
        &support::app(),
        "error",
        vec![
            Request::get("/v1/query/aql").body(Body::empty())?,
            Request::get("/nowhere").body(Body::empty())?,
            Request::get("/health").body(Body::empty())?,
        ],
    )?;
    let lines = request_lines(&error_only)?;
    assert_eq!(
        vec![Some(StatusCode::NOT_IMPLEMENTED.as_u16())],
        lines.iter().map(|line| line.status).collect::<Vec<_>>(),
        "only the 5xx line is at error: {error_only}"
    );
    let warn_and_above = logged(
        &support::app(),
        "warn",
        vec![
            Request::get("/nowhere").body(Body::empty())?,
            Request::get("/health").body(Body::empty())?,
        ],
    )?;
    let lines = request_lines(&warn_and_above)?;
    assert_eq!(
        vec![Some(StatusCode::NOT_FOUND.as_u16())],
        lines.iter().map(|line| line.status).collect::<Vec<_>>(),
        "the 4xx line is at warn and the 2xx line below it: {warn_and_above}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_panic_is_logged_under_the_gateways_id_never_the_clients() -> Result<(), Box<dyn StdError>> {
    let client_id = "SYNTHETIC-NATIONAL-ID-0003";
    let router = ferrofed_server::with_middleware(
        Router::new().route(
            "/boom",
            axum::routing::get(|| async {
                panic!("the handler gave up");
                #[expect(unreachable_code, reason = "the route panics by design")]
                StatusCode::OK
            }),
        ),
        &support::settings(),
    );
    let text = logged(
        &router,
        "info",
        vec![
            Request::get("/boom")
                .header(request_id::HEADER, client_id)
                .body(Body::empty())?,
        ],
    )?;
    let panicked = support::lines(&text)?
        .into_iter()
        .find(|line| line.message == "the request handler panicked")
        .ok_or("the panic is logged")?;
    let logged_id = panicked.request_id.ok_or("under an id")?;
    assert_eq!(
        Some(uuid::Version::Random),
        uuid::Uuid::parse_str(&logged_id)?.get_version(),
        "the panic is logged under the gateway's id: {text}"
    );
    assert!(
        !text.contains(client_id),
        "the client's id reached the log: {text}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_panicking_request_has_its_line_with_five_hundred_under_the_gateways_id()
-> Result<(), Box<dyn StdError>> {
    let router = ferrofed_server::with_middleware(
        Router::new().route(
            "/boom",
            axum::routing::get(|| async {
                panic!("the handler gave up");
                #[expect(unreachable_code, reason = "the route panics by design")]
                StatusCode::OK
            }),
        ),
        &support::settings(),
    );
    let text = logged(
        &router,
        "info",
        vec![
            Request::get("/boom")
                .header(request_id::HEADER, "corr-panic-line")
                .body(Body::empty())?,
        ],
    )?;
    let lines = request_lines(&text)?;
    assert_eq!(1, lines.len(), "the panicking request has its line: {text}");
    let line = lines.first().ok_or("one line")?;
    assert_eq!(Some("/boom"), line.route.as_deref(), "{text}");
    assert_eq!(
        Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
        line.status,
        "{text}"
    );
    assert_eq!(Some(true), line.client_named, "{text}");
    let logged_id = line.request_id.as_deref().ok_or("an id is logged")?;
    assert_eq!(
        Some(uuid::Version::Random),
        uuid::Uuid::parse_str(logged_id)?.get_version(),
        "the line names the gateway's own id: {text}"
    );
    let panicked = support::lines(&text)?
        .into_iter()
        .find(|logged| logged.message == "the request handler panicked")
        .ok_or("the panic is logged")?;
    assert_eq!(
        Some(logged_id),
        panicked.request_id.as_deref(),
        "the request line and the panic line name one request: {text}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_request_past_the_timeout_has_its_line_with_four_hundred_and_eight()
-> Result<(), Box<dyn StdError>> {
    let mut settings = support::settings();
    settings.request_timeout = std::time::Duration::from_millis(20);
    let router = ferrofed_server::with_middleware(
        Router::new().route(
            "/slow",
            axum::routing::get(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                StatusCode::OK
            }),
        ),
        &settings,
    );
    let text = logged(
        &router,
        "info",
        vec![Request::get("/slow").body(Body::empty())?],
    )?;
    assert_eq!(
        vec![Some(StatusCode::REQUEST_TIMEOUT.as_u16())],
        request_lines(&text)?
            .iter()
            .map(|line| line.status)
            .collect::<Vec<_>>(),
        "the timed-out request has its line: {text}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_body_over_the_ceiling_has_its_line_with_four_hundred_and_thirteen()
-> Result<(), Box<dyn StdError>> {
    let oversized = "x".repeat(support::settings().body_limit + 1);
    let text = logged(
        &support::app(),
        "info",
        vec![
            Request::post("/v1/query/aql")
                .header(http::header::CONTENT_LENGTH, oversized.len())
                .body(Body::from(oversized))?,
        ],
    )?;
    assert_eq!(
        vec![Some(StatusCode::PAYLOAD_TOO_LARGE.as_u16())],
        request_lines(&text)?
            .iter()
            .map(|line| line.status)
            .collect::<Vec<_>>(),
        "the refused request has its line: {text}"
    );
    Ok(())
}
