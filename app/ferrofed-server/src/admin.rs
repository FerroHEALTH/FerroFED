// SPDX-FileCopyrightText: Vernum Projecten B.V.
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
// NOTE: no specification governs this: our own design; §12.7 gives drift repair
// no request, so the gateway offers it to the operator only, beside the metrics.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::post;
use http::{HeaderMap, StatusCode};

use crate::config::settings::MetricsSettings;
use crate::facade::stored;
use crate::metrics;
use crate::state::AppState;

/// The path of the operator's distribution of a held stored-query version:
/// the qualified name and the `major.minor.patch` version, as ITS-REST's
/// Definition API spells them.
pub const DISTRIBUTE: &str = "/admin/stored-queries/{qualified_query_name}/{version}/distribute";

/// Returns the admin listener's application over `state`: `GET /metrics` and
/// `POST` [`DISTRIBUTE`], and `404` on every other path.
///
/// The listener carries no authentication, so it binds a loopback address
/// unless the operator allows a remote one.
pub fn router(state: Arc<AppState>) -> Router {
    let metrics = metrics::routes(Arc::clone(state.metrics()));
    Router::new()
        .route(DISTRIBUTE, post(distribute))
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
