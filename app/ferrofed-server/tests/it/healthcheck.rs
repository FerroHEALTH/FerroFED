// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `healthcheck` job on the real binary: `0` only when readiness answers
//! `200`, `1` for a `503`, a refused connection and a gateway that never
//! answers, and one line every time, never the body. No specification
//! governs health probes: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::process::Output;

use axum::Router;
use axum::routing::get;
use ferrofed_server::healthcheck::READINESS;
use ferrofed_testkit::unreachable;
use http::StatusCode;
use tokio::net::TcpListener;

use crate::run::binary;

type TestResult = Result<(), Box<dyn Error>>;

/// A body no line of the job may repeat.
const BODY: &str = "SYNTHETIC-READINESS-BODY";

/// Serves `GET /health/readiness` with `status` on a loopback port, and
/// returns the port's address.
async fn gateway(status: StatusCode) -> Result<SocketAddr, Box<dyn Error>> {
    gateway_at(READINESS, status).await
}

/// Serves `GET path` with `status` on a loopback port, and returns the
/// port's address.
async fn gateway_at(path: &str, status: StatusCode) -> Result<SocketAddr, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let app = Router::new().route(path, get(move || async move { (status, BODY) }));
    tokio::spawn(async move { axum::serve(listener, app).await });
    Ok(address)
}

/// Runs `ferrofed healthcheck` against a gateway configured to listen on
/// `listen`.
async fn healthcheck(listen: &str) -> Result<Output, Box<dyn Error>> {
    let toml = format!("[server]\nlisten = \"{listen}\"\n");
    let output = tokio::task::spawn_blocking(move || {
        binary(&["healthcheck"], &toml).map_err(|error| error.to_string())
    })
    .await??;
    Ok(output)
}

/// Returns everything the job wrote, both streams.
fn written(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Asserts the job wrote exactly one line and no body.
fn one_line(output: &Output) {
    let text = written(output);
    assert_eq!(1, text.lines().count(), "one line: {text:?}");
    assert!(!text.contains(BODY), "never the body: {text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn ready_is_exit_zero() -> TestResult {
    let address = gateway(StatusCode::OK).await?;
    let output = healthcheck(&address.to_string()).await?;
    assert_eq!(Some(0), output.status.code(), "{}", written(&output));
    one_line(&output);
    assert!(written(&output).contains("ready"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wildcard_listen_address_is_asked_on_loopback() -> TestResult {
    let address = gateway(StatusCode::OK).await?;
    let output = healthcheck(&format!("0.0.0.0:{}", address.port())).await?;
    assert_eq!(Some(0), output.status.code(), "{}", written(&output));
    assert!(written(&output).contains("127.0.0.1"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn readiness_is_asked_under_the_configured_base_path() -> TestResult {
    let address = gateway_at("/fed/openehr/health/readiness", StatusCode::OK).await?;
    let toml = format!("[server]\nlisten = \"{address}\"\nbase_path = \"/fed/openehr\"\n");
    let output = tokio::task::spawn_blocking(move || {
        binary(&["healthcheck"], &toml).map_err(|error| error.to_string())
    })
    .await??;
    assert_eq!(Some(0), output.status.code(), "{}", written(&output));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn not_ready_is_exit_one() -> TestResult {
    let address = gateway(StatusCode::SERVICE_UNAVAILABLE).await?;
    let output = healthcheck(&address.to_string()).await?;
    assert_eq!(Some(1), output.status.code(), "{}", written(&output));
    one_line(&output);
    assert!(written(&output).contains("503"), "{}", written(&output));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn another_success_status_is_not_ready() -> TestResult {
    let address = gateway(StatusCode::NO_CONTENT).await?;
    let output = healthcheck(&address.to_string()).await?;
    assert_eq!(Some(1), output.status.code(), "only 200 is ready");
    one_line(&output);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_connection_is_exit_one() -> TestResult {
    let output = healthcheck(&unreachable::ADDRESS.to_string()).await?;
    assert_eq!(Some(1), output.status.code(), "{}", written(&output));
    one_line(&output);
    assert!(
        written(&output).contains("no connection"),
        "{}",
        written(&output)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_gateway_that_never_answers_is_exit_one_within_the_timeout() -> TestResult {
    // The kernel accepts the connection into the backlog, and nothing ever
    // answers on it.
    let silent = TcpListener::bind("127.0.0.1:0").await?;
    let address = silent.local_addr()?;
    let started = std::time::Instant::now();
    let output = healthcheck(&address.to_string()).await?;
    let elapsed = started.elapsed();
    assert_eq!(Some(1), output.status.code(), "{}", written(&output));
    one_line(&output);
    assert!(
        written(&output).contains("no answer within"),
        "{}",
        written(&output)
    );
    assert!(
        elapsed < ferrofed_server::healthcheck::TIMEOUT * 3,
        "the job gives up at its timeout: {elapsed:?}"
    );
    drop(silent);
    Ok(())
}

#[test]
fn a_configuration_that_does_not_load_is_exit_one() -> TestResult {
    let output = binary(&["healthcheck"], "[server]\nlisten_port = 8080\n")?;
    assert_eq!(
        Some(1),
        output.status.code(),
        "a runtime health check reads only 0 and 1"
    );
    one_line(&output);
    Ok(())
}
