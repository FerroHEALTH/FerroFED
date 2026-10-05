// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Graceful shutdown: a request in flight finishes, and the drain is bounded.

use crate::support;
use axum::Router;
use axum::routing::get;
use ferrofed_server::{serve_until, with_middleware};
use std::error::Error as StdError;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::Notify;

/// How long a test waits for a request to reach its handler.
const REACH: Duration = Duration::from_secs(5);

/// The router the shutdown cases serve: a slow route, a stuck one and a fast
/// one. `entered` is notified once a slow or stuck request is in its handler,
/// so a test signals stop only while that request is in flight.
fn app(entered: Arc<Notify>) -> Router {
    let slow = Arc::clone(&entered);
    let mut settings = support::settings();
    settings.request_timeout = Duration::from_secs(60);
    with_middleware(
        Router::new()
            .route(
                "/slow",
                get(move || async move {
                    slow.notify_one();
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    "finished"
                }),
            )
            .route(
                "/stuck",
                get(move || async move {
                    entered.notify_one();
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    "never"
                }),
            )
            .route("/fast", get(|| async { "now" })),
        &settings,
    )
}

/// Serves [`app`] with `drain` until the returned sender fires; the returned
/// [`Notify`] fires when a slow or stuck request reaches its handler.
async fn start(
    drain: Duration,
) -> Result<
    (
        std::net::SocketAddr,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<std::io::Result<()>>,
        Arc<Notify>,
    ),
    Box<dyn StdError>,
> {
    let entered = Arc::new(Notify::new());
    let router = app(Arc::clone(&entered));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        serve_until(listener, router, drain, async move {
            if stopped.await.is_err() {
                tracing::debug!("the stop channel closed");
            }
        })
        .await
    });
    Ok((address, stop, server, entered))
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_request_in_flight_finishes_after_the_stop_signal() -> Result<(), Box<dyn StdError>> {
    let (address, stop, server, entered) = start(Duration::from_secs(5)).await?;
    let request = tokio::spawn(async move {
        reqwest::Client::new()
            .get(format!("http://{address}/slow"))
            .send()
            .await
    });
    tokio::time::timeout(REACH, entered.notified()).await?;
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
    let (address, stop, server, entered) = start(Duration::from_millis(200)).await?;
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
    tokio::time::timeout(REACH, entered.notified()).await?;
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
