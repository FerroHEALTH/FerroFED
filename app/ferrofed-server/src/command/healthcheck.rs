// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `healthcheck`: the readiness of the gateway a configuration describes.
//!
//! The job sends `GET {base}/health/readiness` to the configured listen port
//! on loopback through `ferrohealth_server::healthcheck`, over the
//! listener's own certificate when `[server.tls]` is set
//! (`ferrohealth_server::tls::HealthcheckTls`, which also presents
//! `server.tls.healthcheck_identity_file`). It reports one line, never the
//! body: the `healthcheck::Outcome` the family prints, or the replaced
//! listener certificate, which it has no variant for. It exits `0` only on
//! `200`, which a Docker `HEALTHCHECK` reads as
//! healthy (<https://docs.docker.com/reference/dockerfile/#healthcheck>). No
//! specification governs health probes: our own design.

use std::fmt;
use std::process::ExitCode;
use std::time::Duration;

use ferrohealth_server::health::READINESS;
use ferrohealth_server::healthcheck::{self, Get, GetError, Outcome};
use ferrohealth_server::tls::HealthcheckTls;
use http::StatusCode;

use crate::chain;
use crate::config::settings::Settings;

/// How long the job waits for the whole exchange.
///
/// Shorter than the five seconds the image's `HEALTHCHECK` allows, so the
/// job reports its own line before the runtime gives up on it.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// Asks the gateway this configuration describes for its readiness and
/// prints one line.
///
/// The line is the family's (`healthy`, or `unhealthy: ...`) and the exit
/// code is the outcome's: `0` when readiness answered `200` and `1`
/// otherwise, the two codes a container runtime's health check reads.
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
            eprintln!("ferrofed: {}", Outcome::Failed(chain(&error)));
            return ExitCode::FAILURE;
        }
    };
    let mut get = Get::new(
        address,
        settings.server.base_path.join(READINESS),
        TIMEOUT,
        0,
    );
    if let Some(tls) = &settings.server.tls {
        match HealthcheckTls::read_of(&tls.section, tls.keys) {
            Ok(tls) => get = get.with_tls(tls),
            Err(error) => {
                eprintln!("ferrofed: {}", Outcome::Failed(chain(&error)));
                return ExitCode::FAILURE;
            }
        }
    }
    let line = Line::of(runtime.block_on(get.send()));
    if line.is_healthy() {
        println!("ferrofed: {address}: {line}");
        ExitCode::SUCCESS
    } else {
        eprintln!("ferrofed: {address}: {line}");
        ExitCode::FAILURE
    }
}

/// The line the job prints for what the exchange found; the body is never
/// read, so only the status of an answer counts.
enum Line {
    /// One of the family's outcomes, printed as the crate prints it.
    Outcome(Outcome),
    /// The listener presents a certificate other than the one
    /// `server.tls.certificate_file` holds, which `Outcome` has no variant
    /// for.
    OtherCertificate,
}

impl Line {
    /// The line for the result of the exchange.
    fn of(result: Result<healthcheck::Answer, GetError>) -> Self {
        let status = match result {
            Ok(answer) => answer.status(),
            Err(error) => match error.status() {
                Some(status) => status,
                None => return Self::failure(&error),
            },
        };
        Self::Outcome(if status == StatusCode::OK {
            Outcome::Healthy
        } else {
            Outcome::Unhealthy(status.as_u16())
        })
    }

    /// The line for an exchange that read no status.
    fn failure(error: &GetError) -> Self {
        Self::Outcome(match error {
            GetError::Timeout(_) => Outcome::Timeout,
            GetError::Handshake(cause) if other_certificate(cause) => {
                return Self::OtherCertificate;
            }
            GetError::Exchange(cause) if cause.is_parse() => {
                Outcome::Failed("the answer is no HTTP status line".to_owned())
            }
            error => Outcome::Failed(chain(error)),
        })
    }

    /// Whether readiness answered `200`.
    const fn is_healthy(&self) -> bool {
        matches!(self, Self::Outcome(outcome) if outcome.exit_code() == 0)
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Outcome(outcome) => outcome.fmt(f),
            Self::OtherCertificate => f.write_str(
                "unhealthy: the listener presents a certificate other than server.tls.certificate_file holds; send SIGHUP so the gateway reads it",
            ),
        }
    }
}

/// Whether the handshake failed because the listener presented a
/// certificate other than the one its file holds, the one failure the
/// pinned verifier reports as an application verification failure.
fn other_certificate(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|inner| {
            matches!(
                inner,
                rustls::Error::InvalidCertificate(
                    rustls::CertificateError::ApplicationVerificationFailure
                )
            )
        })
}
