// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: one binary, one subcommand per job.
//!
//! `serve` runs the console, `config check` reads and resolves the
//! configuration the way `serve` would and exits, and `healthcheck` asks the
//! console on this host whether it answers, for a container runtime with no
//! HTTP client of its own. No specification governs the command line: our own
//! design.

use std::path::PathBuf;
use std::sync::LazyLock;

use clap::{Parser, Subcommand};

use crate::manufacturer::MANUFACTURER;

/// What `ferrofed-console --version` prints after the binary's name.
///
/// The version, then the manufacturer with its postal address, its single
/// point of contact and its website (Regulation (EU) 2025/327 Art 30(1)(g)).
pub static LONG_VERSION: LazyLock<String> =
    LazyLock::new(|| MANUFACTURER.version_text(env!("CARGO_PKG_VERSION")));

/// The `ferrofed-console` command line.
#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "ferrofed-console",
    version,
    long_version = LONG_VERSION.as_str(),
    about = "The FerroFED Console"
)]
pub struct Cli {
    /// The configuration file to read, overriding `FERROFED_CONSOLE_CONFIG`.
    #[arg(long, value_name = "PATH", global = true)]
    pub config: Option<PathBuf>,
    /// The job to run.
    #[command(subcommand)]
    pub command: Command,
}

/// What the binary does.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Serves the console until the process is asked to stop.
    Serve,
    /// Works on the configuration.
    Config {
        /// The job to run.
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Asks the console running on this host whether it answers, and exits
    /// `0` only when its health route answers `200`.
    Healthcheck,
}

/// The `config` jobs.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Reads and resolves the configuration, secrets included, and reports
    /// whether the console would start on it.
    Check,
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, ConfigCommand};
    use clap::{CommandFactory, Parser};

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_job_parses_with_and_without_a_configuration_file() {
        for (args, command) in [
            (vec!["ferrofed-console", "serve"], Command::Serve),
            (
                vec!["ferrofed-console", "config", "check"],
                Command::Config {
                    command: ConfigCommand::Check,
                },
            ),
            (vec!["ferrofed-console", "healthcheck"], Command::Healthcheck),
        ] {
            let parsed = Cli::try_parse_from(&args).expect("the job parses");
            assert_eq!(command, parsed.command, "{args:?}");
            assert_eq!(None, parsed.config, "{args:?}");
        }
        let parsed = Cli::try_parse_from(["ferrofed-console", "serve", "--config", "console.toml"])
            .expect("the job parses");
        assert_eq!(Some("console.toml".into()), parsed.config);
    }
}
