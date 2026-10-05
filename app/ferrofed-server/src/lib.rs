// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED server library: the run path the `ferrofed` binary and its
//! integration tests share.
//!
//! [`command`] runs the binary: [`cli`] parses the command line, [`config`]
//! reads the file and the environment into one
//! [`config::settings::Settings`], [`telemetry`] installs the subscriber,
//! [`router`] builds the HTTP surface over [`state::AppState`],
//! and [`serve`] runs it on a bound listener until the process is asked to
//! stop, while [`reload`] replaces the registry on `SIGHUP`. On a terminal,
//! `serve` prints the [`banner`] before the subscriber starts. `main.rs` only
//! hands in the arguments and returns the exit code.
//!
//! Every route sits under the configured base path ([`base_path`]; §4.1,
//! N28). The ITS-REST façade serves the federated query,
//! `POST {base}/v1/query/aql` and its `GET` form ([`facade`], §7), and routes every request to
//! an EHR resource under a path `ehr_id`, the creation of an EHR, and every
//! definition request, to one node ([`facade::route`], §7a.1, §12.4,
//! §12.6), unless the stored-query registry holds the definition
//! ([`facade::stored`], §12.7). A DEMOGRAPHIC request goes to the one
//! endpoint the deployment declared for it when it names that endpoint, and
//! answers `501` where none is declared (§7a.1, §12.6, N32); every other
//! path under `{base}/v1/` answers `501` until its issue lands.
//!
//! [`binding`] holds the regional and national bindings, each one module
//! behind one Cargo feature (`binding-ihe`, `binding-nl`) and the always-built
//! development binding, from which the server builds the roles of resolution,
//! localization, demographics and the consent pre-filter, and the processes
//! that outlive a reload.
//!
//! [`admission`] is the `admission check` job: one member exercised against
//! the identifier-integrity conditions of §12b.2 (§12b.1, N42a, CP-33a).
//! [`conformance`] is the `conformance run` job: the Connectathon tracks of
//! §16.3 driven against a configured deployment, and its report (§16.4).
//! [`healthcheck`] is the `healthcheck` job a container runtime runs beside
//! the server, and [`health`] answers liveness, readiness and the last
//! observed state of every dependency. [`jwks`] serves the gateway's public
//! signing keys, which its OAuth 2.0 client assertions to the nodes are
//! verified against (§13.1, N25).
//!
//! The server builds for Unix targets only: it drains on `SIGTERM` and
//! reloads on `SIGHUP`, and every release binary and the container image are
//! Linux.
#![doc(test(attr(deny(warnings))))]

// NOTE: no specification governs this: our own design; a non-Unix target is
// refused here, with the reason, before the Unix signal code fails to resolve.
#[cfg(not(unix))]
compile_error!(
    "ferrofed-server builds for Unix targets only: it drains on SIGTERM and reloads on SIGHUP through tokio::signal::unix, and every release binary and the container image are Linux"
);

pub mod admin;
pub mod admission;
pub mod auth;
pub mod banner;
pub mod base_path;
pub mod binding;
pub mod body;
pub mod cli;
pub mod command;
pub mod config;
pub mod conformance;
pub mod conveyed;
pub mod documents;
pub mod error;
pub mod facade;
pub mod federation;
pub mod health;
pub mod healthcheck;
pub mod jwks;
pub mod localization;
pub mod metrics;
mod onward;
pub mod panic;
pub mod reload;
pub mod request_id;
pub mod request_log;
pub mod service;
pub mod state;
pub mod stored;
pub mod telemetry;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use http::{HeaderMap, StatusCode};
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;

use crate::config::settings::ServerSettings;
use crate::health::lifecycle::{Lifecycle, drain_on};
use crate::state::AppState;

/// The exit code of a command line the binary refuses.
///
/// Two is the conventional usage exit, the code a command line uses when the
/// invocation is understood and refused.
pub const EXIT_USAGE: u8 = 2;

/// The exit code of a refused configuration.
///
/// `EX_CONFIG` from `sysexits`
/// (<https://man.freebsd.org/cgi/man.cgi?query=sysexits>), so an orchestrator
/// tells a bad configuration from a failure to serve.
pub const EXIT_CONFIG: u8 = 78;

/// The path prefix the ITS-REST surface lives under (`{base}/v1/…`).
pub const ITS_REST_PREFIX: &str = "/v1/";

/// The release of the `openehr-*` crate family this server is built on.
///
/// The family moves in lockstep, so one version names every member the
/// workspace pins (`openehr-query`, `openehr-its`, `openehr-base`,
/// `openehr-rm`, `openehr-sdt`).
pub const OPENEHR_FAMILY: &str = "0.0.82";

/// Returns `error` and every cause behind it as one line.
pub(crate) fn chain(error: &dyn std::error::Error) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}

/// Builds the HTTP application over `state`, with the shared middleware.
///
/// Every route sits under the configured base path, `{base}`
/// ([`ServerSettings::base_path`]; §4.1, N28). `GET {base}/` answers a small
/// JSON document naming the product and its version, `OPTIONS {base}/` the
/// federation's self-description ([`facade::options::options_root`]),
/// `GET {base}/health` answers `200` while the process is up,
/// `GET {base}/health/readiness` answers `200` while the process serves
/// and every registered indicator is up and `503` with the phase and each
/// indicator's state otherwise, and `GET {base}/health/dependencies`
/// answers `200` with the last observed state of each member endpoint and of
/// the resolver ([`health::dependencies`]). `GET {base}/.well-known/jwks.json`
/// answers the gateway's public signing keys with no client authentication
/// ([`jwks`]), and `404` when none are configured. A binding's public
/// document, such as a Nuts holder's DID document, is answered at the path
/// its binding names, with no client authentication ([`documents`]).
/// `POST {base}/v1/query/aql` answers the federated query when a registry is
/// configured ([`facade::query_aql`]), and so does `GET {base}/v1/query/aql`
/// from its query string ([`facade::query_aql_get`]). Every other path under
/// [`ITS_REST_PREFIX`] is routed or answers `501`, and every path outside it,
/// or outside the base, answers `404`. Under a base other than `/`, the base
/// itself and the base with a trailing `/` are both `{base}/`.
pub fn router(state: Arc<AppState>, server: &ServerSettings) -> Router {
    let surface = Router::new()
        .route("/health", get(liveness))
        .route("/health/readiness", get(readiness))
        .route("/health/dependencies", get(dependencies))
        // NOTE: RFC 7517 §5, §13.1 jwks-discovery: public keys are public material,
        // so the JWK Set stays outside every client authentication layer.
        .route(jwks::JWKS_PATH, get(jwks::jwks))
        .route(
            facade::QUERY_AQL,
            get(facade::query_aql_get)
                .post(facade::query_aql)
                .fallback(facade::route::unrouted),
        )
        .fallback(facade::route::unrouted);
    let surface = state.processes().routes(surface);
    let routes = if server.base_path.is_root() {
        surface.route("/", base_root())
    } else {
        // NOTE: no specification governs this: our own design; `{base}/` is the
        // root N28 names, and `{base}` without the slash is served the same.
        let base = server.base_path.as_str();
        Router::new()
            .route(base, base_root())
            .route(&server.base_path.join("/"), base_root())
            .nest(base, surface)
            .fallback(outside_the_base)
    };
    let guard = Arc::new(auth::Guard::new(
        auth::Gate::new(&server.auth),
        server.base_path.clone(),
        Arc::clone(&state),
    ));
    let guarded = routes
        .with_state(Arc::clone(&state))
        .layer(axum::middleware::from_fn_with_state(guard, auth::guard))
        // NOTE: RFC 7517 §5, DID 1.0 §7.1: published key material is public, so a
        // binding's documents are answered outside the client authentication gate.
        .layer(axum::middleware::from_fn_with_state(
            state,
            documents::serve,
        ));
    with_middleware(guarded, server)
}

/// `GET` and `OPTIONS` of `{base}/` (§7a.2).
fn base_root() -> axum::routing::MethodRouter<Arc<AppState>> {
    get(root).options(facade::options::options_root)
}

/// Every path outside the configured base: `404`, naming no path.
async fn outside_the_base(headers: HeaderMap) -> Response {
    error::fixed(
        error::Code::NotFound,
        request_id::of(&headers).unwrap_or_default(),
    )
}

/// Applies the middleware stack every FerroFED surface carries to `router`.
///
/// Outermost first: the request-id normalizer, the panic renderer, the layer
/// that mints the gateway's outbound id, the layer that sets the exchange id,
/// the layer that propagates the exchange id onto the response, the request
/// log, the panic catcher, the request timeout, and the body-size ceiling.
/// The renderer sits outside the outbound and propagate layers because it
/// reads both ids from the response they have just stamped, and the exchange
/// id is set inside the outbound layer so an unnamed request takes the
/// outbound id as its exchange id ([`request_id`]). The log sits outside the
/// catcher, the timeout and the ceiling, so a request one of them answers,
/// a panicking one included, still gets its line with the status it answered.
pub fn with_middleware(router: Router, server: &ServerSettings) -> Router {
    router
        .layer(RequestBodyLimitLayer::new(server.body_limit))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            server.request_timeout,
        ))
        .layer(CatchPanicLayer::custom(panic::caught))
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(server.base_path.clone()),
            request_log::log,
        ))
        .layer(PropagateRequestIdLayer::new(request_id::HEADER))
        .layer(SetRequestIdLayer::new(request_id::HEADER, request_id::Mint))
        .layer(axum::middleware::from_fn(request_id::mint_outbound))
        .layer(axum::middleware::map_response(panic::render))
        .layer(axum::middleware::map_request(request_id::strip_illegal))
}

/// `GET /`: the product and the version, as JSON.
async fn root() -> Json<body::Root> {
    Json(body::Root::default())
}

/// `GET /health`: `200` while the process is up.
async fn liveness() -> Json<body::Liveness> {
    Json(body::Liveness {
        state: health::State::Up,
    })
}

/// `GET /health/readiness`: the phase of the process and the state of every
/// registered indicator.
async fn readiness(State(state): State<Arc<AppState>>) -> Response {
    let report = state.health().evaluate().await;
    let readiness = health::Readiness::new(state.lifecycle().phase(), report);
    (readiness.status(), Json(readiness)).into_response()
}

/// `GET /health/dependencies`: the last observed state of each dependency,
/// always `200` ([`AppState::dependencies`]).
async fn dependencies(State(state): State<Arc<AppState>>) -> Json<health::dependencies::Report> {
    Json(state.dependencies())
}

/// Serves `app` on an already-bound listener until the process receives
/// `SIGTERM` or `SIGINT`, then drains.
///
/// The signal moves `lifecycle` to draining before the drain starts, so
/// readiness answers `503` from the moment the signal arrives
/// ([`drain_on`]).
///
/// # Errors
/// Returns the I/O error from accepting or serving connections.
pub async fn serve(
    listener: TcpListener,
    app: Router,
    server: &ServerSettings,
    lifecycle: Lifecycle,
) -> std::io::Result<()> {
    serve_until(
        listener,
        app,
        server.shutdown_timeout,
        drain_on(shutdown_signal(), lifecycle),
    )
    .await
}

/// Serves `app` on an already-bound listener until `shutdown` completes, then
/// finishes the requests in flight within `drain`.
///
/// A container runtime stops a container with `SIGTERM` to PID 1 and kills it
/// after a grace period, so the drain is bounded here too: a connection still
/// open when `drain` elapses is dropped and the function returns.
///
/// # Errors
/// Returns the I/O error from accepting or serving connections.
pub async fn serve_until<F>(
    listener: TcpListener,
    app: Router,
    drain: Duration,
    shutdown: F,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let signalled = Arc::new(tokio::sync::Notify::new());
    let inner = Arc::clone(&signalled);
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.await;
            inner.notify_one();
        })
        .into_future();
    let mut server = std::pin::pin!(server);
    tokio::select! {
        result = &mut server => return result,
        () = signalled.notified() => {}
    }
    if let Ok(result) = tokio::time::timeout(drain, server).await {
        return result;
    }
    tracing::warn!(
        drain_ms = drain.as_millis(),
        "the drain did not finish in time; the remaining connections are dropped"
    );
    Ok(())
}

/// Completes when the process receives `SIGTERM` or `SIGINT`.
///
/// A failure to install a handler is logged and that arm never completes, so
/// the server keeps serving and the runtime's own kill stays the backstop.
pub async fn shutdown_signal() {
    let interrupt = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGINT");
                std::future::pending::<()>().await;
            }
        }
    };
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        () = interrupt => tracing::info!("SIGINT received, draining"),
        () = terminate => tracing::info!("SIGTERM received, draining"),
    }
}
