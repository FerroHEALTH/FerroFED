// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: one binary, one subcommand per job.
//!
//! `serve` runs the gateway and `config check` reads and resolves the
//! configuration the same way `serve` would, then exits, so an operator or a
//! deployment pipeline tests a configuration without binding a socket.
//! `admission check` exercises one configured member against the
//! identifier-integrity conditions of §12b.2 and writes the report to
//! standard output (§12b.1, N42a, CP-33a). `conformance run` drives the
//! Connectathon tracks of §16.3 against the configured deployment and writes
//! the per-track and per-point report (§16.4). `healthcheck` asks the gateway on
//! this host for its readiness, for a container runtime with no HTTP client
//! of its own. No specification governs the command line: our own design.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::admission::DEFAULT_COUNT;

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
    /// Works on the admission of a member (§12b.1).
    Admission {
        /// The job to run.
        #[command(subcommand)]
        command: AdmissionCommand,
    },
    /// Scores a deployment against the Connectathon tracks of §16.3.
    Conformance {
        /// The job to run.
        #[command(subcommand)]
        command: ConformanceCommand,
    },
    /// Asks the gateway running on this host whether it is ready, and exits
    /// `0` only when readiness answers `200`.
    Healthcheck,
}

/// The `config` jobs.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Reads and resolves the configuration, secrets included, and reports
    /// whether the gateway would start on it.
    Check,
}

/// The `admission` jobs.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum AdmissionCommand {
    /// Creates test EHRs with synthetic subjects on one configured member and
    /// reports each identifier-integrity condition of §12b.2 as pass, fail or
    /// cannot-check, with its evidence.
    Check {
        /// The registry endpoint of the member to check.
        #[arg(long, value_name = "ENDPOINT_ID")]
        endpoint: String,
        /// How many test EHRs to create on the node, at least two so their
        /// `ehr_id`s can be compared.
        #[arg(
            long,
            value_name = "N",
            default_value_t = DEFAULT_COUNT,
            value_parser = clap::value_parser!(u8).range(2..=50)
        )]
        count: u8,
    },
}

/// The `conformance` jobs.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConformanceCommand {
    /// Drives tracks 1 to 7, 9 and 11 against the configured deployment over
    /// ITS-REST and writes the per-track and per-point report (§16.3,
    /// §16.4).
    ///
    /// The run seeds a synthetic patient at every member the cross-reference
    /// names, through each node's own ITS-REST writes, and removes nothing.
    Run(RunArgs),
}

/// The arguments of `conformance run`.
#[derive(Debug, clap::Args, PartialEq, Eq)]
pub struct RunArgs {
    /// Allows the run's writes: a synthetic patient's EHR, the vendored
    /// template and compositions, EHRs with no subject, and stored queries.
    #[arg(long)]
    pub allow_writes: bool,
    /// Allows a run against a deployment whose profile is not development.
    #[arg(long = "i-understand-this-writes-synthetic-data-to-the-nodes")]
    pub acknowledged: bool,
    /// The synthetic patient's namespace, `urn:oid:2.999` or an OID under it.
    #[arg(long, value_name = "OID")]
    pub patient_namespace: String,
    /// The synthetic patient's identifier, which the deployment's
    /// cross-reference resolves.
    #[arg(long, value_name = "VALUE")]
    pub patient_value: String,
    /// A file holding the bearer token the run presents as the caller.
    #[arg(long, value_name = "PATH")]
    pub token_file: PathBuf,
    /// The vendored template and compositions the run writes, checked by
    /// their SHA-256: the directory holding them, or the seed data file a
    /// release attaches (`ferrofed-conformance-seed-data.json`).
    #[arg(long, value_name = "PATH")]
    pub seed_data: PathBuf,
    /// The base URL of a gateway already serving the configuration; without
    /// it, the run starts the gateway in-process.
    #[arg(long, value_name = "URL")]
    pub gateway: Option<url::Url>,
    /// The directory the report is written to.
    #[arg(long, value_name = "DIR", default_value = "conformance-report")]
    pub out: PathBuf,
    /// Also runs the Federation-Node profile checks and the admission check
    /// against every active member and reports their findings as the node
    /// profile.
    #[arg(long)]
    pub node_profile: bool,
}

#[cfg(test)]
mod tests {
    use super::{AdmissionCommand, Cli, Command, ConfigCommand, ConformanceCommand};
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn every_documented_subcommand_parses() {
        let cases: [(&[&str], Command); 5] = [
            (&["ferrofed", "serve"], Command::Serve),
            (&["ferrofed", "healthcheck"], Command::Healthcheck),
            (
                &["ferrofed", "config", "check"],
                Command::Config {
                    command: ConfigCommand::Check,
                },
            ),
            (
                &["ferrofed", "admission", "check", "--endpoint", "node-a-pub"],
                Command::Admission {
                    command: AdmissionCommand::Check {
                        endpoint: "node-a-pub".to_owned(),
                        count: 3,
                    },
                },
            ),
            (
                &[
                    "ferrofed",
                    "admission",
                    "check",
                    "--endpoint",
                    "node-a-pub",
                    "--count",
                    "5",
                ],
                Command::Admission {
                    command: AdmissionCommand::Check {
                        endpoint: "node-a-pub".to_owned(),
                        count: 5,
                    },
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

    #[test]
    fn a_conformance_run_parses_and_defaults_to_no_writes() {
        let base = [
            "ferrofed",
            "conformance",
            "run",
            "--patient-namespace",
            "urn:oid:2.999.1.1",
            "--patient-value",
            "ffd-test-0038",
            "--token-file",
            "/run/token",
            "--seed-data",
            "/data",
        ];
        let cli = Cli::try_parse_from(base).expect("the run parses");
        let Command::Conformance {
            command: ConformanceCommand::Run(args),
        } = cli.command
        else {
            panic!("a conformance run");
        };
        assert!(!args.allow_writes, "writes are never allowed by default");
        assert!(!args.acknowledged, "nor a non-development deployment");
        assert_eq!(None, args.gateway, "the gateway starts in-process");
        assert_eq!(PathBuf::from("conformance-report"), args.out);
        let mut flagged = base.to_vec();
        flagged.extend([
            "--allow-writes",
            "--i-understand-this-writes-synthetic-data-to-the-nodes",
            "--gateway",
            "https://gw.example.org/fed/",
            "--node-profile",
        ]);
        let Command::Conformance {
            command: ConformanceCommand::Run(args),
        } = Cli::try_parse_from(flagged)
            .expect("the flags parse")
            .command
        else {
            panic!("a conformance run");
        };
        assert!(args.allow_writes && args.acknowledged && args.node_profile);
        assert_eq!(
            Some("https://gw.example.org/fed/"),
            args.gateway.as_ref().map(url::Url::as_str)
        );
        for missing in ["--patient-value", "--token-file", "--seed-data"] {
            let without: Vec<&str> = base
                .iter()
                .copied()
                .scan(false, |skip, arg| {
                    let keep = !*skip && arg != missing;
                    *skip = arg == missing;
                    Some(keep.then_some(arg))
                })
                .flatten()
                .collect();
            assert!(
                Cli::try_parse_from(without).is_err(),
                "{missing} is required"
            );
        }
    }

    #[test]
    fn an_admission_check_needs_an_endpoint_and_at_least_two_ehrs() {
        assert!(
            Cli::try_parse_from(["ferrofed", "admission", "check"]).is_err(),
            "the member to check is named, never guessed"
        );
        for count in ["0", "1", "51", "many"] {
            assert!(
                Cli::try_parse_from([
                    "ferrofed",
                    "admission",
                    "check",
                    "--endpoint",
                    "node-a-pub",
                    "--count",
                    count,
                ])
                .is_err(),
                "--count {count} is refused"
            );
        }
    }
}
