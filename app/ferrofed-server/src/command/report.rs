// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `report`: the support archive of a deployment.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::cli::ReportArgs;
use crate::config::settings::Settings;
use crate::{EXIT_USAGE, chain, report};

/// Runs `report`: collects the report of the deployment `settings`
/// configure, from `tree`, the merged configuration, and writes the
/// archive.
///
/// The exit code is `0` when the archive is written, whatever parts the
/// gateway could not answer, each named on standard error and in the
/// manifest; [`EXIT_USAGE`] for an operator token file that does not read
/// or is empty; and `1` when the archive cannot be written.
#[expect(
    clippy::print_stdout,
    reason = "`report` answers the operator who ran it"
)]
#[expect(
    clippy::print_stderr,
    reason = "a missing part or a refusal is reported with no log subscriber installed"
)]
pub(super) fn report_command(
    settings: &Settings,
    tree: &toml::Table,
    args: &ReportArgs,
) -> ExitCode {
    let operator = match args
        .operator_token_file
        .as_deref()
        .map(std::fs::read_to_string)
    {
        None => None,
        Some(Ok(text)) if !text.trim().is_empty() => {
            Some(secrecy::SecretString::from(text.trim().to_owned()))
        }
        Some(Ok(_)) => {
            eprintln!("ferrofed: cannot write the report: the operator token file is empty");
            return ExitCode::from(EXIT_USAGE);
        }
        Some(Err(error)) => {
            eprintln!(
                "ferrofed: cannot write the report: the operator token file could not be read: {error}"
            );
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("ferrofed: cannot write the report: {error}");
            return ExitCode::FAILURE;
        }
    };
    let collected = runtime.block_on(report::Report::collect(settings, tree, operator.as_ref()));
    let written = collected.and_then(|report| {
        let out = args
            .out
            .clone()
            .unwrap_or_else(|| PathBuf::from(report.default_name()));
        report.write(&out).map(|()| (report, out))
    });
    match written {
        Ok((report, out)) => {
            for missing in report.missing() {
                eprintln!(
                    "ferrofed: note: the report has no {}: {}",
                    missing.path, missing.reason
                );
            }
            println!("ferrofed: wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ferrofed: cannot write the report: {}", chain(&error));
            ExitCode::FAILURE
        }
    }
}
