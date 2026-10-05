// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener: the operator's surface beside the client face, bound
//! only when `[metrics] listen` is set, to a loopback address unless
//! `[metrics] allow_remote` allows another.
//!
//! It serves the Prometheus text exposition at `GET /metrics`
//! ([`metrics`]), and the distribution of a stored-query version the
//! registry holds to the members that miss it at `POST`
//! [`DISTRIBUTE`]. Every other path is `404`. Neither route is on the
//! gateway's client listener, and neither is part of the ITS-REST surface.
//!
//! The listener authenticates no one, so its peer decides what it may do.
//! Any peer may read `GET /metrics`. A write or administrative action, the
//! distribution among them, is served to a loopback peer alone and answered
//! `403 operation-refused` to every other, whatever `allow_remote` says
//! ([`loopback_only`]). A remote scraper reads the metrics, and an operator
//! reaches the write actions from the host, or in Kubernetes through
//! `kubectl port-forward` or `kubectl exec` in the pod.
// NOTE: no specification governs this: our own design; §12.7 gives drift repair
// no request, so the gateway offers it to the operator only, beside the metrics.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::rejection::ExtensionRejection;
use axum::extract::{Path, Request, State};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::post;
use http::{HeaderMap, StatusCode};

use crate::config::settings::MetricsSettings;
use crate::error::{self, Code};
use crate::facade::stored;
use crate::metrics;
use crate::request_id;
use crate::state::AppState;

/// The path of the operator's distribution of a held stored-query version:
/// the qualified name and the `major.minor.patch` version, as ITS-REST's
/// Definition API spells them.
pub const DISTRIBUTE: &str = "/admin/stored-queries/{qualified_query_name}/{version}/distribute";

/// The message of a write action refused to a peer that is not loopback.
pub const LOOPBACK_ONLY: &str = "the admin listener serves its write actions to a loopback peer only; run them from the gateway's host, or in Kubernetes through kubectl port-forward or kubectl exec in the pod";

/// Returns the admin listener's application over `state`: `GET /metrics` and
/// `POST` [`DISTRIBUTE`], and `404` on every other path.
///
/// The listener carries no authentication, so it binds a loopback address
/// unless the operator allows a remote one, and its write actions answer a
/// loopback peer alone ([`loopback_only`]). The peer is the
/// [`ConnectInfo<SocketAddr>`] the server records, so the application is
/// served with `into_make_service_with_connect_info::<SocketAddr>()`; a
/// request with no recorded peer is refused the write actions.
pub fn router(state: Arc<AppState>) -> Router {
    let metrics = metrics::routes(Arc::clone(state.metrics()));
    Router::new()
        .route(DISTRIBUTE, post(distribute))
        .route_layer(middleware::from_fn(loopback_only))
        .with_state(state)
        .merge(metrics)
        .fallback(|| async { StatusCode::NOT_FOUND })
}

/// Returns the address and the application of the admin listener `settings`
/// describe over `state`, or `None` when `settings.listen` is unset, so no
/// admin route exists.
#[must_use]
pub fn listener(settings: &MetricsSettings, state: &Arc<AppState>) -> Option<(SocketAddr, Router)> {
    settings
        .listen
        .map(|address| (address, router(Arc::clone(state))))
}

/// Serves the admin listener's `app` on `listener`, recording each
/// connection's peer for [`loopback_only`].
///
/// # Errors
/// Returns the I/O error from accepting or serving connections.
pub async fn serve(listener: tokio::net::TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

/// The middleware in front of every write action: a loopback peer passes.
///
/// Any other peer, or a request whose peer was not recorded, is answered
/// `403 operation-refused` before the action reads anything.
pub async fn loopback_only(
    peer: Result<ConnectInfo<SocketAddr>, ExtensionRejection>,
    request: Request,
    next: Next,
) -> Response {
    // NOTE: no specification governs this: our own design; a peer the server did
    // not record is read as unknown, so the write action is refused.
    let peer = peer.ok().map(|ConnectInfo(peer)| peer);
    // TODO(#635): a remote write action waits for the admin listener's own authentication.
    if peer.is_some_and(|peer| peer.ip().is_loopback()) {
        return next.run(request).await;
    }
    metrics::security::Event::AdminWriteRefused.record();
    tracing::warn!(
        target: crate::facade::security::TARGET,
        event = "admin-write-refused",
        "a write action on the admin listener came from a peer that is not loopback, and was refused"
    );
    let request_id = request_id::of(request.headers()).unwrap_or_default();
    error::response(Code::OperationRefused, LOOPBACK_ONLY, request_id)
}

/// `POST` [`DISTRIBUTE`]: the registry's held copy sent to the members the
/// targeting headers name (`facade::stored::distribute_held`).
async fn distribute(
    State(state): State<Arc<AppState>>,
    Path((name, version)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    stored::distribute_held(&state, (&name, &version), (&headers, &body)).await
}
