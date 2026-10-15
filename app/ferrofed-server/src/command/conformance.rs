// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `conformance run`: the Connectathon scenarios against a deployment.

use std::process::ExitCode;

use crate::cli::RunArgs;
use crate::config::settings::Settings;
use crate::conformance::fixture::SyntheticPatient;
use crate::conformance::run::{RunError, RunOptions};
use crate::conformance::safety::{self, Refusal};
use crate::{EXIT_CONFIG, EXIT_USAGE, chain, config};

/// The refusals of `conformance run` that need no configuration: the writes
/// not allowed, or a patient outside the example arc.
pub(super) fn refused_before_config(args: &RunArgs) -> Result<(), Refusal> {
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
pub(super) fn conformance_command(settings: &Settings, args: RunArgs) -> ExitCode {
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
