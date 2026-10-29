// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: the family's `serve`, `config check` and `healthcheck`
//! from `ferrohealth-server`, with its `--config` and `--set`.
//!
//! `serve` runs the console, `config check` reads and resolves the
//! configuration the way `serve` would and exits, and `healthcheck` asks the
//! console on this host whether it is ready, for a container runtime with no
//! HTTP client of its own. No specification governs the command line: our own
//! design.

use std::sync::LazyLock;

use clap::Parser;
use ferrohealth_server::cli::{Common, ConfigArgs};

use ferrofed_registry::manufacturer::MANUFACTURER;

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
    /// The configuration file to read, overriding `FERROFED_CONSOLE_CONFIG`,
    /// and the `--set key=value` pairs applied over it and the environment.
    #[command(flatten)]
    pub config: ConfigArgs,
    /// The job to run.
    #[command(subcommand)]
    pub command: Common,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{CommandFactory, Parser};
    use ferrohealth_server::cli::{Common, ConfigCommand};

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_job_parses_with_and_without_a_configuration_file() {
        for (args, command) in [
            (vec!["ferrofed-console", "serve"], Common::Serve),
            (
                vec!["ferrofed-console", "config", "check"],
                Common::Config {
                    command: ConfigCommand::Check,
                },
            ),
            (vec!["ferrofed-console", "healthcheck"], Common::Healthcheck),
        ] {
            let parsed = Cli::try_parse_from(&args).expect("the job parses");
            assert_eq!(command, parsed.command, "{args:?}");
            assert_eq!(None, parsed.config.config, "{args:?}");
        }
        let parsed = Cli::try_parse_from(["ferrofed-console", "serve", "--config", "console.toml"])
            .expect("the job parses");
        assert_eq!(Some("console.toml".into()), parsed.config.config);
        let parsed = Cli::try_parse_from([
            "ferrofed-console",
            "config",
            "check",
            "--set",
            "server.locale=nl",
        ])
        .expect("the job parses");
        assert_eq!(
            vec![("server.locale".to_owned(), "nl".to_owned())],
            parsed.config.sets
        );
    }
}
