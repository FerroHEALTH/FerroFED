// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `serve`: the gateway, from its runtime to the drain.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use ferrofed_identity::dev::Profile;
use tokio::net::TcpListener;

use crate::auth::trust::Trust;
use crate::binding::process::Drained;
use crate::config::settings::Settings;
use crate::listener::TlsListener;
use crate::listener::certificates::{Certificates, TlsFiles};
use crate::revision::routes::Managed;
use crate::revision::service::{Booted, Revisions};
use crate::state::AppState;
use crate::support::Support;
use crate::usage_report::reporter::Usage;
use crate::{
    EXIT_CONFIG, admin, banner, binding, body, chain, config, panic, reload, revision, serve,
    state, telemetry, trusted_router,
};

/// Runs `serve`: the banner on a terminal, the runtime, the subscriber with
/// the trace export when `telemetry.otlp_endpoint` is set, the state, and
/// the server, until the process is asked to stop.
///
/// The registry document is read once, before the banner, and the state is
/// built over that read after the subscriber starts, so the banner describes
/// the registry the gateway serves and the build still logs. The spans still
/// held are flushed once the server has stopped.
#[expect(
    clippy::print_stderr,
    reason = "a refused log filter, runtime or trace exporter is reported before any log subscriber exists"
)]
pub(super) fn serve_job(
    settings: Settings,
    tree: &toml::Table,
    config: Option<PathBuf>,
) -> ExitCode {
    let (settings, booted) = match revision::service::boot(settings, tree) {
        Ok(booted) => booted,
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let no_color = std::env::var_os("NO_COLOR");
    let format = settings.telemetry.format;
    let (document, sources) = binding::process::read_source(&settings);
    if banner::prints(format, stdout_is_terminal) {
        let described = document.as_ref().map(Result::as_ref);
        // NOTE: no specification governs this: our own design; a document that
        // does not read has no endpoint URLs, and the build stops on its error.
        let cleartext = config::transport::check(&settings, described.and_then(Result::ok));
        banner::print(
            &banner::Deployment::of(
                settings.server.base_path.clone(),
                settings.server.listen,
                described,
                settings
                    .stored_queries
                    .as_ref()
                    .map(config::stored_queries::Store::backend),
                settings.profile == Profile::Development,
            )
            .with_cleartext(cleartext.as_deref())
            .with_audit_spool_in_memory(
                crate::audit::spools_in_memory(&settings.audit)
                    || binding::compiled()
                        .iter()
                        .any(|binding| binding.spools_in_memory(&settings)),
            ),
            format.colour(stdout_is_terminal, no_color.as_deref()),
        );
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("ferrofed: cannot start the runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    // NOTE: no specification governs this: our own design; the OTLP trace export
    // is a tonic client, which is built inside the runtime it will run on.
    let traces = match settings.telemetry.otlp_endpoint.as_ref().map(|endpoint| {
        let _entered = runtime.enter();
        telemetry::Traces::new(endpoint, settings.telemetry.trace_sample_ratio)
    }) {
        None => None,
        Some(Ok(traces)) => Some(traces),
        Some(Err(error)) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    if let Err(error) = telemetry::init(
        format,
        &settings.telemetry.filter,
        stdout_is_terminal,
        no_color.as_deref(),
        traces.as_ref().map(telemetry::Traces::tracer),
    ) {
        eprintln!("ferrofed: cannot start: {}", chain(&error));
        return ExitCode::from(EXIT_CONFIG);
    }
    panic::install_hook();
    // NOTE: no specification governs this: our own design; the OTLP push is a
    // tonic client, which is built inside the runtime it will run on.
    if let Err(error) = state::admits_callers(&settings, settings.federates()) {
        tracing::error!(error = chain(&error), "cannot start");
        booted.inspect(|booted| booted.refused("callers", "the configuration admits no caller"));
        return ExitCode::from(EXIT_CONFIG);
    }
    let entered = runtime.enter();
    let state = match AppState::build_read(&settings, document) {
        Ok(state) => Arc::new(state.with_sources(sources)),
        Err(error) => {
            tracing::error!(error = chain(&error), "cannot start");
            booted.inspect(|booted| {
                booted.refused(
                    "state",
                    "what the configuration describes could not be built",
                );
            });
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    drop(entered);
    let revisions = booted.map(started);
    let code = match serve_command(&runtime, settings, &state, (config, revisions)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = format!("{error:#}"), "cannot serve");
            ExitCode::FAILURE
        }
    };
    // NOTE: no specification governs this: our own design; the spans still held
    // are flushed while the runtime the export runs on is still up.
    if let Some(Err(error)) = traces.as_ref().map(telemetry::Traces::shutdown) {
        tracing::warn!(error = chain(&error), "the traces could not be flushed");
    }
    code
}

/// Marks the revision the gateway started on running, logging a store that
/// refuses it, and returns the revisions.
fn started(booted: Booted) -> Arc<Revisions> {
    if let Err(error) = booted.started() {
        tracing::error!(
            revision = %booted.file.revision,
            error = chain(&error),
            "the file's revision could not be marked running"
        );
    }
    booted.revisions
}

/// Serves `state` on `runtime` until the process is asked to stop,
/// reloading the registry on `SIGHUP` from `config`, the file `settings`
/// were read from ([`reload`]), and serving `GET /metrics` and the operator's
/// stored-query distribution on the admin listener when `metrics.listen` is
/// set ([`admin`]). Each binding's processes start beside the server, such as
/// a care services directory kept in step and the PMIR identity feed, which
/// deletes its subscription once the drain ends ([`crate::binding`]).
///
/// The metrics are flushed once the gateway has stopped, while the runtime
/// an OTLP push runs on is still up.
fn serve_command(
    runtime: &tokio::runtime::Runtime,
    settings: Settings,
    state: &Arc<AppState>,
    (config, revisions): (Option<PathBuf>, Option<Arc<Revisions>>),
) -> anyhow::Result<()> {
    let server = settings.server.clone();
    let revision_poll = settings.revision_poll;
    let usage = (std::time::Instant::now(), Usage::of(&settings));
    // NOTE: no specification governs this: our own design; the client and the admin
    // listener share one gate, so a reload of `[auth]` reaches both at once.
    let trust = Arc::new(Trust::new(&server.auth));
    let admin = settings.metrics.listen.map(|address| {
        let access = admin::Access::sharing(
            Arc::clone(&trust),
            settings.profile,
            settings.metrics.scrape_users.clone(),
        );
        (address, access)
    });
    let admin_tls_files = settings.metrics.tls.clone();
    let outcome = runtime.block_on(async {
        use anyhow::Context;

        tracing::info!(
            version = body::VERSION,
            indicators = state.health().names().join(","),
            "ferrofed starting"
        );
        if let Some(notice) = Support::current().notice(body::VERSION) {
            tracing::warn!("{notice}");
        }
        let client_tls = certificates(server.tls.as_ref())?;
        let listener = TcpListener::bind(server.listen)
            .await
            .with_context(|| format!("binding {}", server.listen))?;
        // NOTE: no specification governs this: our own design; the bound address, so a
        // listener configured on port 0 names the port the system gave it.
        let bound = listener
            .local_addr()
            .with_context(|| format!("reading the address {} bound", server.listen))?;
        tracing::info!(listen = %bound, tls = client_tls.is_some(), "listening");
        let mut listener_certificates: Vec<Arc<Certificates>> =
            client_tls.iter().cloned().collect();
        let admin_tls = match admin {
            Some(_) => certificates(admin_tls_files.as_ref())?,
            None => None,
        };
        listener_certificates.extend(admin_tls.iter().cloned());
        let reloader = Arc::new(
            reload::Reloader::new(config, settings, Arc::clone(state))
                .with_certificates(listener_certificates)
                .with_revisions(revisions.clone())
                .with_trust(Arc::clone(&trust)),
        );
        let watch = revision::watch::start(revisions.as_ref(), &reloader, state, revision_poll);
        if let Some((address, access)) = admin {
            let managed = revisions
                .map(|revisions| Arc::new(Managed::new(revisions, Arc::clone(&reloader))));
            let app = admin::serving_router(
                Arc::clone(state),
                access,
                managed,
                Some(Arc::clone(&reloader)),
            );
            admin::start(address, app, admin_tls)
                .await
                .with_context(|| format!("binding metrics.listen {address}"))?;
        }
        let running = state.processes().start(&reloader);
        tokio::spawn(reload::on_hangup(reloader));
        let app = trusted_router(Arc::clone(state), &server, trust);
        state.lifecycle().booted();
        let report = usage.1.start(Arc::clone(state), usage.0).await;
        let lifecycle = state.lifecycle().clone();
        let stopped = match client_tls {
            None => serve(listener, app, &server, lifecycle).await,
            Some(certificates) => {
                serve(
                    TlsListener::new(listener, certificates)?,
                    app,
                    &server,
                    lifecycle,
                )
                .await
            }
        };
        if let Some(report) = report {
            report.abort();
        }
        if let Some(watch) = watch {
            watch.stop(server.bindings_drain).await;
        }
        if running.drain(server.bindings_drain).await == Drained::Abandoned {
            tracing::warn!(
                budget_ms = server.bindings_drain.as_millis(),
                "the bindings did not stop within server.bindings_drain_timeout_ms; what they hold at a remote service may be left there"
            );
        }
        stopped.context("serving HTTP")?;
        tracing::info!("ferrofed stopped");
        anyhow::Ok(())
    });
    if let Err(error) = state.metrics().shutdown() {
        tracing::warn!(error = chain(&error), "the metrics could not be flushed");
    }
    outcome
}

/// Reads the TLS `files` of a listener, when it serves TLS.
///
/// `config check` read the same files, so a failure here means they changed
/// between the check and the start.
fn certificates(files: Option<&TlsFiles>) -> anyhow::Result<Option<Arc<Certificates>>> {
    files
        .map(|files| {
            Certificates::load(files.clone())
                .map(Arc::new)
                .map_err(anyhow::Error::new)
        })
        .transpose()
}
