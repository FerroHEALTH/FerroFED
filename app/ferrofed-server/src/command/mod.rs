// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run path of the `ferrofed` binary.
//!
//! The command line is parsed, the configuration read, and each command run
//! to its exit code: `serve`, `config check`, `admission check`,
//! `conformance run`, `healthcheck` and `report`.

use std::process::ExitCode;

use clap::Parser;
use ferrohealth_server::healthcheck::Outcome;

use crate::cli::{AdmissionCommand, Cli, Command, ConfigCommand, ConformanceCommand};
use crate::config::Config;
use crate::state::AppState;
use crate::{EXIT_CONFIG, EXIT_USAGE, chain, config, state};

mod admission;
mod config_check;
mod conformance;
pub mod healthcheck;
mod report;
mod serve;

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
        && let Err(refusal) = conformance::refused_before_config(args)
    {
        eprintln!(
            "ferrofed: cannot run the conformance scenarios: {}",
            chain(&refusal)
        );
        return ExitCode::from(EXIT_USAGE);
    }
    let loaded = Config::load(&cli.sources).and_then(|loaded| {
        let settings = Config::resolve_loaded(&loaded)?;
        config::revisions::check_tree(settings.revisions.as_ref(), loaded.table_with_secrets())?;
        Ok((settings, loaded))
    });
    let (settings, loaded) = match loaded {
        Ok(loaded) => loaded,
        Err(error) if cli.command == Command::Healthcheck => {
            // NOTE: no specification governs this: our own design; a runtime
            // reads any exit but 0 and 1 as reserved, so a refusal is unhealthy.
            eprintln!("ferrofed: {}", Outcome::Failed(chain(&error)));
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("ferrofed: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    match cli.command {
        Command::Healthcheck => healthcheck::healthcheck_command(&settings),
        Command::Config {
            command: ConfigCommand::Check,
        } => match AppState::check(&settings).and_then(|cleartext| {
            state::admits_callers(&settings, settings.federates()).map(|()| cleartext)
        }) {
            Ok(cleartext) => config_check::config_checked(&cleartext, &settings),
            Err(error) => {
                eprintln!("ferrofed: cannot start: {}", chain(&error));
                ExitCode::from(EXIT_CONFIG)
            }
        },
        Command::Admission {
            command:
                AdmissionCommand::Check {
                    endpoint,
                    count,
                    read_only,
                },
        } => admission::admission_command(&settings, &endpoint, count, read_only),
        Command::Conformance {
            command: ConformanceCommand::Run(args),
        } => conformance::conformance_command(&settings, args),
        Command::Report(args) => {
            report::report_command(&settings, loaded.table_with_secrets(), &args)
        }
        Command::Serve => serve::serve_job(settings, &loaded, cli.sources),
    }
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
