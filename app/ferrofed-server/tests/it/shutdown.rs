// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Graceful shutdown: a request in flight finishes, and the drain is bounded.

use crate::support;
use axum::Router;
use axum::routing::get;
use ferrofed_server::{serve_until, with_middleware};
use std::error::Error as StdError;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

/// The router the shutdown cases serve: a slow route, a stuck one and a fast
/// one.
fn app() -> Router {
    let mut settings = support::settings();
    settings.request_timeout = Duration::from_secs(60);
    with_middleware(
        Router::new()
            .route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    "finished"
                }),
            )
            .route(
                "/stuck",
                get(|| async {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    "never"
                }),
            )
            .route("/fast", get(|| async { "now" })),
        &settings,
    )
}

/// Serves [`app`] with `drain` until the returned sender fires.
async fn start(
    drain: Duration,
) -> Result<
    (
        std::net::SocketAddr,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ),
    Box<dyn StdError>,
> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        serve_until(listener, app(), drain, async move {
            if stopped.await.is_err() {
                tracing::debug!("the stop channel closed");
            }
        })
        .await
    });
    Ok((address, stop, server))
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_request_in_flight_finishes_after_the_stop_signal() -> Result<(), Box<dyn StdError>> {
    let (address, stop, server) = start(Duration::from_secs(5)).await?;
    let request = tokio::spawn(async move {
        reqwest::Client::new()
            .get(format!("http://{address}/slow"))
            .send()
            .await
    });
    // Give the request time to reach the handler before the signal arrives.
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop.send(()).map_err(|()| "the server is gone")?;
    let response = request.await??;
    assert!(response.status().is_success(), "{}", response.status());
    assert_eq!("finished", response.text().await?);
    server.await??;
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn the_drain_is_bounded_so_a_stuck_request_cannot_hold_the_process()
-> Result<(), Box<dyn StdError>> {
    let (address, stop, server) = start(Duration::from_millis(200)).await?;
    let fast = reqwest::Client::new()
        .get(format!("http://{address}/fast"))
        .send()
        .await?;
    assert!(fast.status().is_success());
    let stuck = tokio::spawn(async move {
        reqwest::Client::new()
            .get(format!("http://{address}/stuck"))
            .send()
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = Instant::now();
    stop.send(()).map_err(|()| "the server is gone")?;
    tokio::time::timeout(Duration::from_secs(5), server).await???;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the server returned once the drain elapsed: {:?}",
        started.elapsed()
    );
    stuck.abort();
    Ok(())
}
