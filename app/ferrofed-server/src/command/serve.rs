// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `serve`: the gateway, from its runtime to the drain.

use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;

use ferrofed_identity::profile::Profile;
use tokio::net::TcpListener;

use crate::auth::trust::Trust;
use crate::binding::process::Drained;
use crate::config::Config;
use crate::config::server::ListenerTlsSettings;
use crate::config::settings::Settings;
use crate::revision::routes::Managed;
use crate::revision::service::{Booted, Revisions};
use crate::state::AppState;
use crate::support::Support;
use crate::usage_report::Usage;
use crate::{
    EXIT_CONFIG, admin, banner, binding, body, chain, config, metrics, reload, revision, state,
    trusted_router,
};
use ferrohealth_config::loader::Loaded;
use ferrohealth_server::cli::ConfigArgs;
use ferrohealth_server::serve::Listener;
use ferrohealth_server::tls::ReloadableTls;
use ferrohealth_telemetry::section::Filter;
use ferrohealth_telemetry::telemetry::{Console, InitError, Telemetry};

/// Runs `serve`: the banner on a terminal, the runtime, the telemetry with
/// the trace export and the metrics push `[telemetry.otlp]` asks for, the
/// state, and the server, until the process is asked to stop.
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
    loaded: &Loaded<Config>,
    config: ConfigArgs,
) -> ExitCode {
    let tree = loaded.table_with_secrets();
    let (settings, booted) = match revision::service::boot(settings, tree) {
        Ok(booted) => booted,
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let console = match console(stdout_is_terminal) {
        Ok(console) => console,
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
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
            console.colour(format),
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
    let telemetry = match telemetry(&settings, &runtime) {
        Ok(telemetry) => Arc::new(telemetry),
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    if let Err(error) = telemetry.install(&settings.telemetry, &console) {
        eprintln!("ferrofed: cannot start: {}", chain(&error));
        return ExitCode::from(EXIT_CONFIG);
    }
    ferrohealth_telemetry::panic::install_hook();
    if let Err(error) = state::admits_callers(&settings, settings.federates()) {
        tracing::error!(error = chain(&error), "cannot start");
        booted.inspect(|booted| booted.refused("callers", "the configuration admits no caller"));
        return ExitCode::from(EXIT_CONFIG);
    }
    let entered = runtime.enter();
    let state = match AppState::build_telemetered(&settings, document, Arc::clone(&telemetry)) {
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
    let code = match serve_command(&runtime, settings, &state, (config, revisions, loaded)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = format!("{error:#}"), "cannot serve");
            ExitCode::FAILURE
        }
    };
    // NOTE: no specification governs this: our own design; the spans and the metrics
    // still held are flushed while the runtime the export runs on is still up.
    if let Err(error) = telemetry.shutdown() {
        tracing::warn!(error = chain(&error), "the telemetry could not be flushed");
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
/// reloading the registry on `SIGHUP` from `sources`, the file and the
/// `--set` pairs `settings` were read from ([`reload`]), and serving `GET /metrics` and the operator's
/// stored-query distribution on the admin listener when `metrics.listen` is
/// set ([`admin`]). Each binding's processes start beside the server, such as
/// a care services directory kept in step and the PMIR identity feed, which
/// deletes its subscription once the drain ends ([`crate::binding`]).
fn serve_command(
    runtime: &tokio::runtime::Runtime,
    settings: Settings,
    state: &Arc<AppState>,
    (sources, revisions, loaded): (ConfigArgs, Option<Arc<Revisions>>, &Loaded<Config>),
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
    runtime.block_on(async {
        use anyhow::Context;

        tracing::info!(
            version = body::VERSION,
            indicators = state.health().names().join(","),
            "ferrofed starting"
        );
        if let Some(notice) = Support::current().notice(body::VERSION) {
            tracing::warn!("{notice}");
        }
        let client_tls = kept(server.tls.as_ref())?;
        let listener = TcpListener::bind(server.listen)
            .await
            .with_context(|| format!("binding {}", server.listen))?;
        // NOTE: no specification governs this: our own design; the bound address, so a
        // listener configured on port 0 names the port the system gave it.
        let bound = listener
            .local_addr()
            .with_context(|| format!("reading the address {} bound", server.listen))?;
        tracing::info!(listen = %bound, tls = client_tls.is_some(), "listening");
        let mut listener_tls: Vec<Arc<ReloadableTls>> = client_tls.iter().cloned().collect();
        let admin_tls = match admin {
            Some(_) => kept(admin_tls_files.as_ref())?,
            None => None,
        };
        listener_tls.extend(admin_tls.iter().cloned());
        let reloader = Arc::new(
            reload::Reloader::new(sources.config, settings, Arc::clone(state))
                .with_sets(sources.sets)
                .with_tree(loaded.table_with_secrets().clone())
                .with_provenance(loaded.provenance().clone())
                .with_certificates(listener_tls)
                .with_revisions(revisions.clone())
                .with_trust(Arc::clone(&trust)),
        );
        let watch = revision::watch::start(revisions.as_ref(), &reloader, state, revision_poll);
        let app = trusted_router(Arc::clone(state), &server, trust);
        let mut listeners = vec![with_tls(Listener::new(listener, app), client_tls)];
        if let Some((address, access)) = admin {
            let managed =
                revisions.map(|revisions| Arc::new(Managed::new(revisions, Arc::clone(&reloader))));
            let app = admin::serving_router(
                Arc::clone(state),
                access,
                managed,
                Some(Arc::clone(&reloader)),
            );
            let bound = admin::bind(address, admin_tls.is_some())
                .await
                .with_context(|| format!("binding metrics.listen {address}"))?;
            listeners.push(with_tls(Listener::new(bound, app), admin_tls));
        }
        let running = state.processes().start(&reloader);
        tokio::spawn(reload::on_hangup(reloader));
        state.lifecycle().booted();
        let report = usage.1.start(Arc::clone(state), usage.0).await;
        let lifecycle = state.lifecycle().clone();
        let stopped = ferrohealth_server::serve::run_all(
            listeners,
            &server.section(),
            lifecycle,
            ferrohealth_server::serve::shutdown_signal(),
        )
        .await;
        if let Some(report) = report {
            report.abort();
        }
        if let Some(watch) = watch {
            watch.stop(server.bindings_drain).await;
        }
        drain_bindings(running, server.bindings_drain).await;
        stopped.context("serving HTTP")?;
        tracing::info!("ferrofed stopped");
        anyhow::Ok(())
    })
}

/// Stops the bindings `running` started, then every audit trail, each within
/// `budget`, logging the one that did not stop in time.
///
/// The trails stop last, so the records the bindings write as they stop are
/// spooled before each forwarder closes its repository connection with a TLS
/// `close_notify` (RFC 5425 §4.4).
async fn drain_bindings(running: binding::process::Running, budget: std::time::Duration) {
    if running.drain(budget).await == Drained::Abandoned {
        tracing::warn!(
            budget_ms = budget.as_millis(),
            "the bindings did not stop within server.bindings_drain_timeout_ms; what they hold at a remote service may be left there"
        );
    }
    if !crate::audit::stop_trails(budget).await {
        tracing::warn!(
            budget_ms = budget.as_millis(),
            "the audit trails did not stop within server.bindings_drain_timeout_ms; the frames a forwarder wrote last may be lost to a reset"
        );
    }
}

/// Reads the TLS `tls` of a listener, when it serves TLS, into the
/// configuration the server serves and the gateway keeps, which a reload
/// and an applied revision read again.
///
/// `config check` read the same files, so a failure here means they changed
/// between the check and the start.
fn kept(tls: Option<&ListenerTlsSettings>) -> anyhow::Result<Option<Arc<ReloadableTls>>> {
    tls.map(|tls| {
        ReloadableTls::of(&tls.section, tls.keys)
            .map(Arc::new)
            .map_err(anyhow::Error::new)
    })
    .transpose()
}

/// `listener` over `tls`, when it serves TLS.
fn with_tls(listener: Listener, tls: Option<Arc<ReloadableTls>>) -> Listener {
    match tls {
        Some(tls) => listener.with_reloadable_tls(tls),
        None => listener,
    }
}

/// The console of this process, whose security events the console writes
/// whatever `telemetry.filter` says.
///
/// # Errors
///
/// Returns the [`InitError`] of a security directive that does not parse,
/// which a build never holds.
fn console(stdout_is_terminal: bool) -> Result<Console, InitError> {
    let security = Filter::new(crate::facade::security::DIRECTIVE)?;
    Ok(Console::new(stdout_is_terminal, std::env::var_os("NO_COLOR").as_deref()).force(security))
}

/// The telemetry `settings` configure, built inside `runtime`: the
/// Prometheus reader always, and the trace export and the metrics push
/// `[telemetry.otlp]` asks for.
///
/// # Errors
///
/// Returns the [`InitError`] an exporter or a collector TLS file could not
/// be built or read with.
fn telemetry(
    settings: &Settings,
    runtime: &tokio::runtime::Runtime,
) -> Result<Telemetry, InitError> {
    // NOTE: no specification governs this: our own design; the OTLP export is a tonic
    // client, which is built inside the runtime it will run on.
    let _entered = runtime.enter();
    Telemetry::new(&settings.telemetry, metrics::service())
}
