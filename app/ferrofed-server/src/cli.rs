// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: one binary, one subcommand per job.
//!
//! `serve` runs the gateway and `config check` reads and resolves the
//! configuration the same way `serve` would, then exits, so an operator or a
//! deployment pipeline tests a configuration without binding a socket. No
//! specification governs the command line: our own design.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// The `ferrofed` command line.
#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "ferrofed",
    version,
    about = "The FerroFED openEHR federation gateway"
)]
pub struct Cli {
    /// The configuration file to read, overriding `FERROFED_CONFIG`.
    #[arg(long, value_name = "PATH", global = true)]
    pub config: Option<PathBuf>,
    /// The job to run.
    #[command(subcommand)]
    pub command: Command,
}

/// What the binary does.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Serves the gateway until the process is asked to stop.
    Serve,
    /// Works on the configuration.
    Config {
        /// The job to run.
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

/// The `config` jobs.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Reads and resolves the configuration, secrets included, and reports
    /// whether the gateway would start on it.
    Check,
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, ConfigCommand};
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn every_documented_subcommand_parses() {
        let cases: [(&[&str], Command); 2] = [
            (&["ferrofed", "serve"], Command::Serve),
            (
                &["ferrofed", "config", "check"],
                Command::Config {
                    command: ConfigCommand::Check,
                },
            ),
        ];
        for (argv, expected) in cases {
            let cli = Cli::try_parse_from(argv).expect("the subcommand parses");
            assert_eq!(expected, cli.command, "{argv:?}");
            assert_eq!(None, cli.config, "no --config was given");
        }
    }

    #[test]
    fn the_config_flag_is_global_and_reads_a_path() {
        let cli = Cli::try_parse_from(["ferrofed", "serve", "--config", "/etc/ferrofed.toml"])
            .expect("--config parses after the subcommand");
        assert_eq!(Some(PathBuf::from("/etc/ferrofed.toml")), cli.config);
    }

    #[test]
    fn a_subcommand_is_required_and_an_unknown_one_is_refused() {
        assert!(
            Cli::try_parse_from(["ferrofed"]).is_err(),
            "the binary does nothing without a job"
        );
        assert!(
            Cli::try_parse_from(["ferrofed", "run"]).is_err(),
            "an unknown word must not fall through to serving"
        );
        assert!(
            Cli::try_parse_from(["ferrofed", "config"]).is_err(),
            "a job group needs its job"
        );
    }
}
