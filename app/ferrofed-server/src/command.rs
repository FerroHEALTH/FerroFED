// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run path of the `ferrofed` binary.
//!
//! The command line is parsed, the configuration read, and each command run
//! to its exit code: `serve`, `config check`, `admission check`,
//! `conformance run` and `healthcheck`.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use ferrofed_identity::dev::Profile;
use tokio::net::TcpListener;

use crate::cli::{AdmissionCommand, Cli, Command, ConfigCommand, ConformanceCommand, RunArgs};
use crate::config::Config;
use crate::config::settings::Settings;
use crate::conformance::fixture::SyntheticPatient;
use crate::conformance::run::{RunError, RunOptions};
use crate::conformance::safety::{self, Refusal};
use crate::federation::Federation;
use crate::state::AppState;
use crate::{
    EXIT_CONFIG, EXIT_USAGE, admin, admission, banner, binding, body, chain, config, healthcheck,
    metrics, panic, reload, router, serve, state, telemetry,
};

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
    if let Command::Conformance {
        command: ConformanceCommand::Run(args),
    } = &cli.command
        && let Err(refusal) = refused_before_config(args)
    {
        eprintln!(
            "ferrofed: cannot run the conformance scenarios: {}",
            chain(&refusal)
        );
        return ExitCode::from(EXIT_USAGE);
    }
    let settings = match Config::load(cli.config.as_deref()).and_then(|config| config.resolve()) {
        Ok(settings) => settings,
        Err(error) if cli.command == Command::Healthcheck => {
            // NOTE: no specification governs this: our own design; a runtime
            // reads any exit but 0 and 1 as reserved, so a refusal is unhealthy.
            eprintln!("ferrofed: not ready: {}", chain(&error));
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    match cli.command {
        Command::Healthcheck => healthcheck_command(&settings),
        Command::Config {
            command: ConfigCommand::Check,
        } => match AppState::check(&settings).and_then(|cleartext| {
            state::admits_callers(&settings, settings.federates()).map(|()| cleartext)
        }) {
            Ok(cleartext) => config_checked(&cleartext),
            Err(error) => {
                eprintln!("ferrofed: cannot start: {}", chain(&error));
                ExitCode::from(EXIT_CONFIG)
            }
        },
        Command::Admission {
            command: AdmissionCommand::Check { endpoint, count },
        } => admission_command(&settings, &endpoint, count),
        Command::Conformance {
            command: ConformanceCommand::Run(args),
        } => conformance_command(&settings, args),
        Command::Serve => serve_job(settings, cli.config),
    }
}

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
fn serve_job(settings: Settings, config: Option<PathBuf>) -> ExitCode {
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
                binding::compiled()
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
        return ExitCode::from(EXIT_CONFIG);
    }
    let entered = runtime.enter();
    let state = match AppState::build_read(&settings, document) {
        Ok(state) => Arc::new(state.with_sources(sources)),
        Err(error) => {
            tracing::error!(error = chain(&error), "cannot start");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    drop(entered);
    let code = match serve_command(&runtime, settings, &state, config) {
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

/// Reports a resolved configuration and its `cleartext` credentials, and exits.
#[expect(
    clippy::print_stdout,
    reason = "`config check` answers the person or pipeline that ran it"
)]
fn config_checked(cleartext: &[config::transport::ProtectedSite]) -> ExitCode {
    config::transport::print_warnings(cleartext);
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
                "ferrofed: cannot check admission: set a registry, registry.document or [registry.mcsd], whose members the check exercises"
            );
            return ExitCode::from(EXIT_CONFIG);
        }
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    if let Err(error) = config::transport::check_and_print(settings, Some(federation.snapshot())) {
        eprintln!("ferrofed: cannot start: {}", chain(&error));
        return ExitCode::from(EXIT_CONFIG);
    }
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

/// The refusals of `conformance run` that need no configuration: the writes
/// not allowed, or a patient outside the example arc.
fn refused_before_config(args: &RunArgs) -> Result<(), Refusal> {
    safety::writes(args.allow_writes)?;
    SyntheticPatient::new(
        &args.patient_namespace,
        secrecy::SecretString::from(args.patient_value.clone()),
    )?;
    Ok(())
}

/// Runs `conformance run` against the deployment `settings` configure and
/// writes the report.
///
/// The exit code is `0` when no scenario failed, `1` when one did or the
/// run could not reach its report, [`EXIT_USAGE`] for a run the safety rules
/// refuse, and [`EXIT_CONFIG`] for a configuration that federates nothing
/// or does not build, or would send the token or the synthetic data in the
/// clear.
#[expect(
    clippy::print_stdout,
    reason = "`conformance run` answers the operator who ran it"
)]
#[expect(
    clippy::print_stderr,
    reason = "a refusal is reported to the operator, with no log subscriber installed"
)]
fn conformance_command(settings: &Settings, args: RunArgs) -> ExitCode {
    let refuse = |refusal: &Refusal| {
        eprintln!(
            "ferrofed: cannot run the conformance scenarios: {}",
            chain(refusal)
        );
        ExitCode::from(EXIT_USAGE)
    };
    if let Err(refusal) = safety::profile(settings.profile, args.acknowledged) {
        return refuse(&refusal);
    }
    let patient = match SyntheticPatient::new(
        &args.patient_namespace,
        secrecy::SecretString::from(args.patient_value),
    ) {
        Ok(patient) => patient,
        Err(error) => return refuse(&Refusal::Patient(error)),
    };
    let token = match std::fs::read_to_string(&args.token_file) {
        Ok(text) if !text.trim().is_empty() => secrecy::SecretString::from(text.trim().to_owned()),
        Ok(_) => return refuse(&Refusal::NoToken),
        Err(error) => {
            eprintln!(
                "ferrofed: cannot run the conformance scenarios: the token file {} could not be read: {error}",
                args.token_file.display()
            );
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("ferrofed: cannot run the conformance scenarios: {error}");
            return ExitCode::FAILURE;
        }
    };
    let options = RunOptions {
        gateway: args.gateway,
        token,
        patient,
        seed_data: args.seed_data,
        out: args.out,
        node_profile: args.node_profile,
    };
    match runtime.block_on(crate::conformance::run::run(settings, options)) {
        Ok(summary) => {
            let counts: Vec<String> = summary
                .report
                .counts()
                .iter()
                .map(|(result, count)| format!("{result} {count}"))
                .collect();
            println!(
                "ferrofed: conformance run against {}: {}",
                summary.base,
                counts.join("; ")
            );
            config::transport::print_warnings(&summary.cleartext);
            for path in &summary.paths {
                println!("ferrofed: wrote {}", path.display());
            }
            if summary.report.failed() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!(
                "ferrofed: cannot run the conformance scenarios: {}",
                chain(&error)
            );
            match error {
                RunError::NoRegistry
                | RunError::Federation(_)
                | RunError::State(_)
                | RunError::Cleartext(_) => ExitCode::from(EXIT_CONFIG),
                RunError::SeedData(_) => ExitCode::from(EXIT_USAGE),
                _ => ExitCode::FAILURE,
            }
        }
    }
}

/// Asks the gateway this configuration describes for its readiness and
/// prints one line.
///
/// The exit code is `0` when readiness answered `200` and `1` otherwise,
/// the two codes a container runtime's health check reads.
#[expect(
    clippy::print_stdout,
    reason = "`healthcheck` answers the runtime or operator that ran it"
)]
#[expect(
    clippy::print_stderr,
    reason = "an unready gateway is reported with no log subscriber installed"
)]
fn healthcheck_command(settings: &Settings) -> ExitCode {
    let address = healthcheck::target(settings.server.listen);
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("ferrofed: not ready: {error}");
            return ExitCode::FAILURE;
        }
    };
    let path = settings.server.base_path.join(healthcheck::READINESS);
    let outcome = runtime.block_on(healthcheck::check(address, &path, healthcheck::TIMEOUT));
    if outcome.is_ready() {
        println!("ferrofed: {address}: {outcome}");
        ExitCode::SUCCESS
    } else {
        eprintln!("ferrofed: {address}: {outcome}");
        ExitCode::FAILURE
    }
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
    config: Option<PathBuf>,
) -> anyhow::Result<()> {
    let server = settings.server.clone();
    let admin = admin::listener(&settings.metrics, state);
    let outcome = runtime.block_on(async {
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
        if let Some((address, app)) = admin {
            let metrics = TcpListener::bind(address)
                .await
                .with_context(|| format!("binding metrics.listen {address}"))?;
            tracing::info!(
                listen = %address,
                path = metrics::PATH,
                distribute = admin::DISTRIBUTE,
                "serving the admin listener"
            );
            tokio::spawn(async move {
                if let Err(error) = axum::serve(metrics, app).await {
                    tracing::error!(%error, "the metrics listener stopped");
                }
            });
        }
        let reloader = Arc::new(reload::Reloader::new(config, settings, Arc::clone(state)));
        let running = state.processes().start(&reloader);
        tokio::spawn(reload::on_hangup(reloader));
        let app = router(Arc::clone(state), &server);
        state.lifecycle().booted();
        let stopped = serve(listener, app, &server, state.lifecycle().clone()).await;
        running.drain().await;
        stopped.context("serving HTTP")?;
        tracing::info!("ferrofed stopped");
        anyhow::Ok(())
    });
    if let Err(error) = state.metrics().shutdown() {
        tracing::warn!(error = chain(&error), "the metrics could not be flushed");
    }
    outcome
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
