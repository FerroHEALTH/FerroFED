// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED server library: the run path the `ferrofed` binary and its
//! integration tests share.
//!
//! [`cli`] parses the command line, [`config`] reads the file and the
//! environment into one [`config::settings::Settings`], [`telemetry`] installs the
//! subscriber, [`router`] builds the HTTP surface over [`state::AppState`],
//! and [`serve`] runs it on a bound listener until the process is asked to
//! stop, while [`reload`] replaces the registry on `SIGHUP`. `main.rs` only
//! hands in the arguments and returns the exit code.
//!
//! The ITS-REST façade serves the federated query, `POST /v1/query/aql`
//! ([`facade`], §7), and routes every request to an EHR resource under a
//! path `ehr_id`, the creation of an EHR, and every definition request, to
//! one node ([`facade::route`], §7a.1, §12.4, §12.6), unless the
//! stored-query registry holds the definition ([`facade::stored`], §12.7).
//! A DEMOGRAPHIC request goes to the one endpoint the deployment configured
//! for it, and answers `501` where none is (§7a.1, N32); every other path
//! under `/v1/` answers `501` until its issue lands.
//!
//! [`admission`] is the `admission check` job: one member exercised against
//! the identifier-integrity conditions of §12b.2 (§12b.1, N42a, CP-33a).
#![doc(test(attr(deny(warnings))))]

pub mod admission;
pub mod body;
pub mod cli;
pub mod config;
pub mod error;
pub mod facade;
pub mod federation;
pub mod health;
pub mod panic;
pub mod reload;
pub mod request_id;
pub mod request_log;
pub mod state;
pub mod stored;
pub mod telemetry;

use std::future::Future;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use clap::Parser;
use ferrofed_engine::outbound_id::OutboundId;
use http::{HeaderMap, Method, StatusCode, Uri};
use openehr_its::rest::routes::{self, Lookup};
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;

use crate::cli::{AdmissionCommand, Cli, Command, ConfigCommand};
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
        Command::Admission {
            command: AdmissionCommand::Check { endpoint, count },
        } => admission_command(&settings, &endpoint, count),
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
            panic::install_hook();
            let state = match AppState::build(&settings) {
                Ok(state) => Arc::new(state),
                Err(error) => {
                    tracing::error!(error = chain(&error), "cannot start");
                    return ExitCode::from(EXIT_CONFIG);
                }
            };
            match serve_command(settings, state, cli.config) {
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

/// Runs the admission check against `endpoint` with `count` test EHRs and
/// writes the report to standard output.
///
/// The exit code is `0` when no condition failed, `1` when one did,
/// [`EXIT_USAGE`] for an endpoint the registry does not hold, and
/// [`EXIT_CONFIG`] for a configuration that federates nothing or does not
/// load.
#[expect(
    clippy::print_stdout,
    reason = "`admission check` answers the operator who ran it"
)]
#[expect(
    clippy::print_stderr,
    reason = "a refusal is reported to the operator, with no log subscriber installed"
)]
fn admission_command(settings: &Settings, endpoint: &str, count: u8) -> ExitCode {
    let federation = match Federation::load(settings) {
        Ok(Some(federation)) => federation,
        Ok(None) => {
            eprintln!(
                "ferrofed: cannot check admission: set registry.document, whose members the check exercises"
            );
            return ExitCode::from(EXIT_CONFIG);
        }
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let endpoint = match ferrofed_registry::id::EndpointId::new(endpoint) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            eprintln!("ferrofed: cannot check admission: {}", chain(&error));
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("ferrofed: cannot check admission: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(admission::check(&federation, &endpoint, count)) {
        Ok(report) => {
            println!("{report}");
            if report.failed() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("ferrofed: cannot check admission: {}", chain(&error));
            match error {
                admission::AdmissionError::UnknownEndpoint(_) => ExitCode::from(EXIT_USAGE),
                _ => ExitCode::FAILURE,
            }
        }
    }
}

/// Builds the runtime and serves `state` until the process is asked to stop,
/// reloading the registry on `SIGHUP` from `config`, the file `settings`
/// were read from ([`reload`]).
fn serve_command(
    settings: Settings,
    state: Arc<AppState>,
    config: Option<PathBuf>,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let server = settings.server.clone();
    runtime.block_on(async move {
        use anyhow::Context;

        tracing::info!(
            version = body::VERSION,
            indicators = state.health().names().join(","),
            "ferrofed starting"
        );
        let listener = TcpListener::bind(server.listen)
            .await
            .with_context(|| format!("binding {}", server.listen))?;
        tracing::info!(listen = %server.listen, "listening");
        #[cfg(unix)]
        tokio::spawn(reload::on_hangup(Arc::new(reload::Reloader::new(
            config,
            settings,
            Arc::clone(&state),
        ))));
        #[cfg(not(unix))]
        drop((config, settings));
        serve(listener, router(Arc::clone(&state), &server), &server)
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
/// `GET /` answers a small JSON document naming the product and its version,
/// `OPTIONS /` the federation's self-description
/// ([`facade::options::options_root`]),
/// `GET /health` answers `200` while the process is up, and
/// `GET /health/readiness` answers `200` when every registered indicator is up
/// and `503` with each indicator's state otherwise. `POST /v1/query/aql` answers
/// the federated query when a registry is configured ([`facade::query_aql`]).
/// Every other path under [`ITS_REST_PREFIX`] answers `501`, because its
/// part of the façade is not built yet, and every path outside it answers
/// `404`.
pub fn router(state: Arc<AppState>, server: &ServerSettings) -> Router {
    let routes = Router::new()
        .route("/", get(root).options(facade::options::options_root))
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
/// A path under [`ITS_REST_PREFIX`] is part of the ITS-REST surface: a
/// request to an EHR resource under a path `ehr_id`, the creation of an
/// EHR, a definition request the stored-query registry does not hold, and a
/// DEMOGRAPHIC request where the deployment configured its endpoint, is
/// routed to one node ([`facade::route`]; §7a.1, §12.4, §12.6), `OPTIONS`
/// names the methods the gateway serves for the path
/// ([`facade::options::allow`]; §7a.2), and every other path answers `501`
/// (§7a.1, N32), because a `404` would claim the resource does not exist.
/// Every other path answers `404`. No answer of the gateway's own echoes the path.
///
/// A routed request reaches its node under the request's [`OutboundId`],
/// never the client's `x-request-id` (§5.4.1, N33).
async fn unrouted(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    let outbound = outbound.map_or_else(OutboundId::mint, |Extension(id)| id);
    let Some(path) = uri
        .path()
        .strip_prefix(ITS_REST_PREFIX.trim_end_matches('/'))
        .filter(|path| path.starts_with('/'))
    else {
        return error::fixed(error::Code::NotFound, request_id);
    };
    if method == Method::OPTIONS {
        return facade::options::allow(&state, path, request_id);
    }
    let mut arrived = facade::route::Arrived {
        method: &method,
        path,
        uri: &uri,
        headers: &headers,
        body,
        request_id,
        outbound,
    };
    let federation = state.federation();
    if let (Some(federation), Some(definitions)) = (federation.as_deref(), state.definitions())
        && let Lookup::Matched(matched) = routes::lookup(&method, path)
    {
        match facade::stored::serve(federation, definitions, &matched, arrived).await {
            Ok(response) => return response,
            Err(unanswered) => arrived = unanswered,
        }
    }
    facade::route::serve(federation.as_deref(), arrived).await
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
