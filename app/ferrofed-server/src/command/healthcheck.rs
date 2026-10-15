// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `healthcheck`: the readiness of the gateway a configuration describes.

use std::process::ExitCode;

use crate::config::settings::Settings;
use crate::{chain, healthcheck};

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
pub(super) fn healthcheck_command(settings: &Settings) -> ExitCode {
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
    let tls = match settings.server.tls.as_ref().map(healthcheck::Tls::read) {
        None => None,
        Some(Ok(tls)) => Some(tls),
        Some(Err(error)) => {
            eprintln!("ferrofed: not ready: {}", chain(&error));
            return ExitCode::FAILURE;
        }
    };
    let path = settings.server.base_path.join(healthcheck::READINESS);
    let outcome = runtime.block_on(healthcheck::check(
        address,
        &path,
        healthcheck::TIMEOUT,
        tls.as_ref(),
    ));
    if outcome.is_ready() {
        println!("ferrofed: {address}: {outcome}");
        ExitCode::SUCCESS
    } else {
        eprintln!("ferrofed: {address}: {outcome}");
        ExitCode::FAILURE
    }
}
