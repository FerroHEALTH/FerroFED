// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener authenticates no one, so its peer decides: any peer
//! reads `GET /metrics`, and a write action answers a loopback peer alone,
//! `403 operation-refused` to every other whatever `metrics.allow_remote`
//! says. No specification governs the admin listener: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use ferrofed_server::admin;
use ferrofed_server::metrics::security::Event;
use ferrofed_server::state::AppState;
use http::{Request, StatusCode};
use tokio::net::TcpListener;

use crate::support::{error_body, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// A remote peer under the documentation range of RFC 5737.
const REMOTE: ([u8; 4], u16) = ([192, 0, 2, 10], 40_000);

/// The operator's distribution of a stored-query version.
const WRITE: &str = "/admin/stored-queries/example::query/1.0.0/distribute";

/// The admin listener's application, as a request from `peer` reaches it.
fn from(peer: SocketAddr) -> Router {
    admin::router(Arc::new(AppState::default())).layer(MockConnectInfo(peer))
}

/// Sends `request` to `app` and reads the status and the body.
async fn answer(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let response = send_as_is(app, request).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, String::from_utf8(bytes.to_vec())?))
}

/// Whether `status` and `text` are the refusal of a write action.
fn refused(status: StatusCode, text: &str) -> bool {
    status == StatusCode::FORBIDDEN
        && error_body(text).is_ok_and(|body| body.code == "operation-refused")
}

#[tokio::test]
async fn a_remote_peer_reads_the_metrics_and_is_refused_every_write() -> TestResult {
    let app = from(SocketAddr::from(REMOTE));
    let (status, _) = answer(app.clone(), Request::get("/metrics").body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "the scrape is read-only and open");
    let before = Event::AdminWriteRefused.counted();
    let (status, text) = answer(app, Request::post(WRITE).body(Body::empty())?).await?;
    assert!(refused(status, &text), "{status}: {text}");
    assert!(
        text.contains("loopback"),
        "the refusal says how to reach it: {text}"
    );
    assert_eq!(
        before + 1,
        Event::AdminWriteRefused.counted(),
        "the refusal is counted"
    );
    Ok(())
}

#[tokio::test]
async fn a_loopback_peer_keeps_the_write_actions() -> TestResult {
    for peer in [
        SocketAddr::from(([127, 0, 0, 1], 40_000)),
        SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 40_000)),
    ] {
        let (status, text) = answer(from(peer), Request::post(WRITE).body(Body::empty())?).await?;
        assert!(
            !refused(status, &text),
            "{peer} reaches the action: {status} {text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_request_with_no_recorded_peer_is_refused_every_write() -> TestResult {
    let app = admin::router(Arc::new(AppState::default()));
    let (status, text) = answer(app, Request::post(WRITE).body(Body::empty())?).await?;
    assert!(refused(status, &text), "{status}: {text}");
    Ok(())
}

#[tokio::test]
async fn the_served_listener_records_a_loopback_peer() -> TestResult {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(admin::serve(
        listener,
        admin::router(Arc::new(AppState::default())),
    ));
    let response = reqwest::Client::new()
        .post(format!("http://{address}{WRITE}"))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    server.abort();
    assert!(
        !refused(status, &text),
        "a loopback connection reaches the action: {status} {text}"
    );
    Ok(())
}
