// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run path of the `ferrofed-console` binary: the command line parsed, the
//! configuration read, and each job run to its exit code.

use std::error::Error;
use std::fmt::Write as _;
use std::process::ExitCode;

use clap::Parser;
use ferrohealth_server::cli::{Common, ConfigCommand};
use ferrohealth_telemetry::telemetry::{Console, Service, Telemetry};

use crate::cli::Cli;
use crate::config::Config;
use crate::config::settings::Settings;
use crate::server::ConsoleState;

/// How long `healthcheck` waits for the console's answer, shorter than the
/// five seconds the image's `HEALTHCHECK` allows.
pub const HEALTHCHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// The exit code of a configuration the console will not start on, from the
/// BSD `sysexits.h` set (`EX_CONFIG`).
pub const EXIT_CONFIG: u8 = 78;

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
        Err(error) => {
            // NOTE: no specification governs this: our own design; clap prints
            // the usage or the version itself and names the exit code.
            let code = error.exit_code();
            if error.print().is_err() {
                return ExitCode::FAILURE;
            }
            return u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from);
        }
    };
    let loaded = match Config::load(cli.config.config.as_deref(), &cli.config.sets) {
        Ok(loaded) => loaded,
        Err(error) if cli.command == Common::Healthcheck => {
            eprintln!("ferrofed-console: not up: {}", chain(&error));
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("ferrofed-console: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let report = (cli.command
        == Common::Config {
            command: ConfigCommand::Check,
        })
    .then(|| ferrohealth_config::report::check_report(&loaded));
    let settings = match loaded.into_parts().0.resolve() {
        Ok(settings) => settings,
        Err(error) if cli.command == Common::Healthcheck => {
            eprintln!("ferrofed-console: not up: {}", chain(&error));
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("ferrofed-console: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    match cli.command {
        Common::Healthcheck => runtime_then(|| healthcheck(&settings)),
        Common::Config {
            command: ConfigCommand::Check,
        } => config_check(settings, report),
        Common::Serve => runtime_then(|| serve(settings)),
    }
}

/// Runs `job` on a multi-threaded runtime.
#[expect(
    clippy::print_stderr,
    reason = "a runtime that cannot start is reported before any log subscriber exists"
)]
fn runtime_then<F, J>(job: J) -> ExitCode
where
    J: FnOnce() -> F,
    F: Future<Output = ExitCode>,
{
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(job()),
        Err(error) => {
            eprintln!("ferrofed-console: cannot start the runtime: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Runs `config check`: the running tree with every secret redacted, then the
/// state the server would start with is built, and nothing is bound.
#[expect(
    clippy::print_stdout,
    reason = "the job's one answer goes to standard output"
)]
#[expect(
    clippy::print_stderr,
    reason = "the job's refusal goes to standard error"
)]
fn config_check(
    settings: Settings,
    report: Option<Result<String, ferrohealth_config::error::ConfigError>>,
) -> ExitCode {
    match report {
        Some(Ok(report)) => println!("{report}"),
        Some(Err(error)) => {
            eprintln!("ferrofed-console: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
        None => {}
    }
    for key in &settings.deprecated {
        eprintln!(
            "ferrofed-console: warning: `{key}` is deprecated and does nothing; release {} refuses it",
            crate::config::SITE_ROOT_REFUSED_IN
        );
    }
    match ConsoleState::new(settings) {
        Ok(_state) => {
            println!("ferrofed-console: the configuration is valid");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ferrofed-console: cannot start: {}", chain(&error));
            ExitCode::from(EXIT_CONFIG)
        }
    }
}

/// Runs `healthcheck` against the console this configuration describes.
#[expect(
    clippy::print_stdout,
    reason = "the job's one-line answer goes to standard output"
)]
#[expect(
    clippy::print_stderr,
    reason = "the job's one-line refusal goes to standard error"
)]
async fn healthcheck(settings: &Settings) -> ExitCode {
    let target = ferrohealth_server::healthcheck::target(settings.listen);
    let outcome = ferrohealth_server::healthcheck::check(target, "", HEALTHCHECK_TIMEOUT).await;
    if outcome.exit_code() == 0 {
        println!("ferrofed-console: {outcome}");
        ExitCode::SUCCESS
    } else {
        eprintln!("ferrofed-console: {outcome}");
        ExitCode::FAILURE
    }
}

/// Runs `serve`: the subscriber, the state, and the server until `SIGTERM`
/// or `SIGINT`.
#[expect(
    clippy::print_stderr,
    reason = "a refusal before the subscriber starts has no other place to go"
)]
async fn serve(settings: Settings) -> ExitCode {
    let service = Service::new("ferrofed-console", env!("CARGO_PKG_VERSION"));
    let installed = Telemetry::new(&settings.telemetry, service).and_then(|telemetry| {
        telemetry
            .install(&settings.telemetry, &Console::detect())
            .map(|_filter| telemetry)
    });
    let telemetry = match installed {
        Ok(telemetry) => telemetry,
        Err(error) => {
            eprintln!("ferrofed-console: cannot start the log: {error}");
            return ExitCode::FAILURE;
        }
    };
    ferrohealth_telemetry::panic::install_hook();
    for key in &settings.deprecated {
        tracing::warn!(
            key,
            refused_in = crate::config::SITE_ROOT_REFUSED_IN,
            "a deprecated configuration key is set"
        );
    }
    let listen = settings.listen;
    let state = match ConsoleState::new(settings) {
        Ok(state) => state,
        Err(error) => {
            tracing::error!(error = %chain(&error), "cannot start");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let listener = match tokio::net::TcpListener::bind(listen).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%listen, %error, "cannot bind the listen address");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(%listen, "the operator console is listening");
    let lifecycle = state.lifecycle().clone();
    let served = crate::server::serve(
        listener,
        crate::server::router(state),
        lifecycle,
        ferrohealth_server::serve::shutdown_signal(),
    );
    let code = match served.await {
        Ok(_drained) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the listener failed");
            ExitCode::FAILURE
        }
    };
    if let Err(error) = telemetry.shutdown() {
        tracing::warn!(%error, "the telemetry did not flush");
    }
    code
}

/// Renders `error` with every cause in its chain, colon-separated.
#[must_use]
pub fn chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        // NOTE: no specification governs this: our own design; writing to a
        // String cannot fail, so the result carries nothing.
        if write!(text, ": {cause}").is_err() {
            break;
        }
        source = cause.source();
    }
    text
}
