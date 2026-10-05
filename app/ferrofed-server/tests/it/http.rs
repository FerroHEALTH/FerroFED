// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HTTP surface and the middleware stack: the routes, the honest refusal
//! of the unbuilt façade, the request id, the panic body, the timeout and the
//! body ceiling.

use crate::support::{self, app, call, error_body, send};
use axum::Router;
use axum::body::Body;
use axum::routing::get;
use ferrofed_server::{request_id, with_middleware};
use http::{Request, StatusCode, header};
use serde::Deserialize;
use std::error::Error as StdError;
use std::time::Duration;

/// A synthetic `ehr_id` a request path carries.
///
/// A test searches a response for the whole id: a fragment of it is
/// hexadecimal, so a minted request id in the same body could contain one.
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The root document.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Root {
    product: String,
    version: String,
}

/// A liveness or readiness document's aggregate state.
#[derive(Debug, Deserialize)]
struct Aggregate {
    state: String,
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn the_root_document_names_the_product_and_the_version() -> Result<(), Box<dyn StdError>> {
    let (status, body) = call(app(), Request::get("/").body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status);
    let document: Root = serde_json::from_str(&body)?;
    assert_eq!("FerroFED", document.product);
    assert_eq!(env!("CARGO_PKG_VERSION"), document.version);
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn liveness_is_two_hundred_once_the_router_is_built() -> Result<(), Box<dyn StdError>> {
    let (status, body) = call(app(), Request::get("/health").body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status);
    let document: Aggregate = serde_json::from_str(&body)?;
    assert_eq!("up", document.state);
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn the_unbuilt_its_rest_surface_answers_five_hundred_and_one_and_echoes_nothing()
-> Result<(), Box<dyn StdError>> {
    for request in [
        Request::get("/v1/query/aql?q=SELECT%20e%20FROM%20EHR%20e").body(Body::empty())?,
        Request::post("/v1/query/aql")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"q":"SELECT e FROM EHR e"}"#))?,
        Request::get(format!("/v1/ehr/{EHR_ID}")).body(Body::empty())?,
        Request::get("/v1/demographic/party/1").body(Body::empty())?,
    ] {
        let path = request.uri().path().to_owned();
        let (status, body) = call(app(), request).await?;
        assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "{path}");
        let document = error_body(&body)?;
        assert_eq!("not-implemented", document.code, "{path}");
        assert!(!document.request_id.is_empty(), "a request id is named");
        assert!(!body.contains("SELECT"), "the body echoes no query: {body}");
        assert!(!body.contains(EHR_ID), "the body echoes no path: {body}");
    }
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_path_outside_every_surface_answers_four_hundred_and_four()
-> Result<(), Box<dyn StdError>> {
    let (status, body) = call(app(), Request::get("/nowhere").body(Body::empty())?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    let document = error_body(&body)?;
    assert_eq!("not-found", document.code);
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_legal_request_id_is_echoed_and_an_illegal_one_is_replaced()
-> Result<(), Box<dyn StdError>> {
    let response = send(
        app(),
        Request::get("/health")
            .header(request_id::HEADER, "corr-42")
            .body(Body::empty())?,
    )
    .await?;
    assert_eq!(
        Some("corr-42"),
        response
            .headers()
            .get(request_id::HEADER)
            .and_then(|value| value.to_str().ok())
    );

    // A newline cannot be built as a header value at all, so the illegal
    // forms a wire can carry are an empty value, an over-long one and a
    // non-ASCII byte.
    for illegal in [
        header::HeaderValue::from_static(""),
        header::HeaderValue::from_str(&"a".repeat(request_id::MAX_LENGTH + 1))?,
        header::HeaderValue::from_bytes(&[0xff, 0xfe])?,
    ] {
        let response = send(
            app(),
            Request::get("/health")
                .header(request_id::HEADER, illegal.clone())
                .body(Body::empty())?,
        )
        .await?;
        let minted = response
            .headers()
            .get(request_id::HEADER)
            .and_then(|value| value.to_str().ok())
            .ok_or("a request id is always answered")?;
        assert!(
            request_id::is_legal(minted),
            "the minted id is legal: {minted}"
        );
        assert_ne!(illegal.as_bytes(), minted.as_bytes());
    }
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_panicking_handler_yields_a_five_hundred_with_the_request_id_and_no_message()
-> Result<(), Box<dyn StdError>> {
    let router = with_middleware(
        Router::new().route(
            "/boom",
            get(|| async {
                panic!("the handler held SYNTHETIC-PANIC-VALUE");
                #[expect(unreachable_code, reason = "the route panics by design")]
                StatusCode::OK
            }),
        ),
        &support::settings(),
    );
    let response = send(
        router,
        Request::get("/boom")
            .header(request_id::HEADER, "corr-panic")
            .body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, response.status());
    assert_eq!(
        Some("corr-panic"),
        response
            .headers()
            .get(request_id::HEADER)
            .and_then(|value| value.to_str().ok())
    );
    assert_eq!(
        Some("application/json"),
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
    );
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let document = error_body(std::str::from_utf8(&bytes)?)?;
    assert_eq!("internal", document.code);
    assert_eq!("corr-panic", document.request_id);
    assert!(
        !String::from_utf8_lossy(&bytes).contains("SYNTHETIC-PANIC-VALUE"),
        "the panic message never reaches the client"
    );
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_request_past_the_timeout_yields_four_hundred_and_eight() -> Result<(), Box<dyn StdError>>
{
    let mut settings = support::settings();
    settings.request_timeout = Duration::from_millis(20);
    let router = with_middleware(
        Router::new().route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                StatusCode::OK
            }),
        ),
        &settings,
    );
    let (status, _) = call(router, Request::get("/slow").body(Body::empty())?).await?;
    assert_eq!(StatusCode::REQUEST_TIMEOUT, status);
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_body_over_the_ceiling_yields_four_hundred_and_thirteen() -> Result<(), Box<dyn StdError>>
{
    let settings = support::settings();
    let oversized = "x".repeat(settings.body_limit + 1);
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_LENGTH, oversized.len())
        .body(Body::from(oversized))?;
    let (status, _) = call(app(), request).await?;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, status);
    Ok(())
}
