// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `admission check`: the admission conditions run against one member.

use std::process::ExitCode;

use crate::config::settings::Settings;
use crate::federation::Federation;
use crate::{EXIT_CONFIG, EXIT_USAGE, admission, chain, config};

/// Runs the admission check against `endpoint` with `count` test EHRs, or
/// with `read_only` reading at most `count` existing EHRs and writing
/// nothing, and writes the report to standard output.
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
pub(super) fn admission_command(
    settings: &Settings,
    endpoint: &str,
    count: u8,
    read_only: bool,
) -> ExitCode {
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
    let checked = if read_only {
        runtime.block_on(admission::read_only::check(&federation, &endpoint, count))
    } else {
        runtime.block_on(admission::check(&federation, &endpoint, count))
    };
    match checked {
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
