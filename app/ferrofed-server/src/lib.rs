// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED server library: the run path the `ferrofed` binary and its
//! integration tests share.
//!
//! [`cli`] parses the command line, [`config`] reads the file and the
//! environment into one [`config::settings::Settings`], [`telemetry`] installs the
//! subscriber, [`router`] builds the HTTP surface over [`state::AppState`],
//! and [`serve`] runs it on a bound listener until the process is asked to
//! stop. `main.rs` only hands in the arguments and returns the exit code.
//!
//! The ITS-REST façade serves the federated query, `POST /v1/query/aql`
//! ([`facade`], §7); every other path under
//! `/v1/` answers `501` until its issue lands.
#![doc(test(attr(deny(warnings))))]

pub mod body;
pub mod cli;
pub mod config;
pub mod error;
pub mod facade;
pub mod federation;
pub mod health;
pub mod panic;
pub mod request_id;
pub mod request_log;
pub mod state;
pub mod telemetry;

use std::future::Future;
use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use http::{HeaderMap, StatusCode, Uri};
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;

use crate::cli::{Cli, Command, ConfigCommand};
use crate::config::Config;
use crate::config::settings::{ServerSettings, Settings};
use crate::federation::Federation;
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

/// Runs the binary with `args` and returns the process exit code.
///
/// `args` is the whole argument vector, the program name included, so the
/// command-line parser reports usage under the right name.
#[must_use]
#[expect(
    clippy::print_stderr,
    reason = "a refused configuration is reported before any log subscriber exists"
)]
pub fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => return ExitCode::from(clap_exit(&error)),
    };
    let settings = match Config::load(cli.config.as_deref()).and_then(|config| config.resolve()) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    match cli.command {
        Command::Config {
            command: ConfigCommand::Check,
        } => match Federation::load(&settings) {
            Ok(_) => config_checked(),
            Err(error) => {
                eprintln!("ferrofed: cannot start: {}", chain(&error));
                ExitCode::from(EXIT_CONFIG)
            }
        },
        Command::Serve => {
            let stdout_is_terminal = std::io::stdout().is_terminal();
            if let Err(error) = telemetry::init(
                settings.telemetry.format,
                &settings.telemetry.filter,
                stdout_is_terminal,
            ) {
                eprintln!("ferrofed: cannot start: {}", chain(&error));
                return ExitCode::from(EXIT_CONFIG);
            }
            let state = match AppState::build(&settings) {
                Ok(state) => Arc::new(state),
                Err(error) => {
                    tracing::error!(error = chain(&error), "cannot start");
                    return ExitCode::from(EXIT_CONFIG);
                }
            };
            match serve_command(&settings, state) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    tracing::error!(error = format!("{error:#}"), "cannot serve");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

/// Reports a configuration that resolved, and exits successfully.
#[expect(
    clippy::print_stdout,
    reason = "`config check` answers the person or pipeline that ran it"
)]
fn config_checked() -> ExitCode {
    println!("ferrofed: the configuration is valid");
    ExitCode::SUCCESS
}

/// Builds the runtime and serves `state` until the process is asked to stop.
fn serve_command(settings: &Settings, state: Arc<AppState>) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        use anyhow::Context;

        tracing::info!(
            version = body::VERSION,
            indicators = state.health().names().join(","),
            "ferrofed starting"
        );
        let listener = TcpListener::bind(settings.server.listen)
            .await
            .with_context(|| format!("binding {}", settings.server.listen))?;
        tracing::info!(listen = %settings.server.listen, "listening");
        serve(
            listener,
            router(Arc::clone(&state), &settings.server),
            &settings.server,
        )
        .await
        .context("serving HTTP")?;
        tracing::info!("ferrofed stopped");
        Ok(())
    })
}

/// Returns the exit code a command-line refusal deserves.
///
/// `--help` and `--version` are not failures: clap reports both as an error
/// whose kind says the text was printed
/// (<https://docs.rs/clap/4/clap/error/enum.ErrorKind.html>).
#[expect(
    clippy::print_stdout,
    reason = "clap renders help and version to stdout, which is where a person reads them"
)]
#[expect(
    clippy::print_stderr,
    reason = "a usage refusal is reported before any log subscriber exists"
)]
fn clap_exit(error: &clap::Error) -> u8 {
    if error.use_stderr() {
        eprint!("{error}");
        EXIT_USAGE
    } else {
        print!("{error}");
        0
    }
}

/// Returns `error` and every cause behind it as one line.
fn chain(error: &dyn std::error::Error) -> String {
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
/// `GET /` answers a small JSON document naming the product and its version,
/// `GET /health` answers `200` while the process is up, and
/// `GET /health/readiness` answers `200` when every registered indicator is up
/// and `503` with each indicator's state otherwise. `POST /v1/query/aql` answers
/// the federated query when a registry is configured ([`facade::query_aql`]).
/// Every other path under [`ITS_REST_PREFIX`] answers `501`, because its
/// part of the façade is not built yet, and every path outside it answers
/// `404`.
pub fn router(state: Arc<AppState>, server: &ServerSettings) -> Router {
    let routes = Router::new()
        .route("/", get(root))
        .route("/health", get(liveness))
        .route("/health/readiness", get(readiness))
        .route(
            facade::QUERY_AQL,
            post(facade::query_aql).fallback(unrouted),
        )
        .fallback(unrouted)
        .with_state(state);
    with_middleware(routes, server)
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
        .layer(axum::middleware::from_fn(request_log::log))
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

/// `GET /health/readiness`: the state of every registered indicator.
async fn readiness(State(state): State<Arc<AppState>>) -> Response {
    let report = state.health().evaluate().await;
    (report.status(), Json(report)).into_response()
}

/// Every path no route serves.
///
/// A path under [`ITS_REST_PREFIX`] is part of the ITS-REST surface the
/// gateway will serve, so it answers `501` (§7a.1, N32): that part of the
/// façade is not built yet, and a `404` would claim the resource does not
/// exist. Every other path answers `404`. Neither answer echoes the path.
async fn unrouted(uri: Uri, headers: HeaderMap) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    if uri.path().starts_with(ITS_REST_PREFIX) {
        error::fixed(error::Code::NotImplemented, request_id)
    } else {
        error::fixed(error::Code::NotFound, request_id)
    }
}

/// Serves `app` on an already-bound listener until the process receives
/// `SIGTERM` or `SIGINT`, then drains.
///
/// # Errors
/// Returns the I/O error from accepting or serving connections.
pub async fn serve(
    listener: TcpListener,
    app: Router,
    server: &ServerSettings,
) -> std::io::Result<()> {
    serve_until(listener, app, server.shutdown_timeout, shutdown_signal()).await
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
