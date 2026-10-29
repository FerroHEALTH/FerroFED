// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `healthcheck`: the readiness of the gateway a configuration describes.
//!
//! The job sends `GET {base}/health/readiness` to the configured listen port
//! on loopback through `ferrohealth_server::healthcheck`, over the
//! listener's own certificate when `[server.tls]` is set
//! (`ferrohealth_server::tls::HealthcheckTls`, which also presents
//! `server.tls.healthcheck_identity_file`). It reports one line, never the
//! body, and exits `0` only on `200`, which a Docker `HEALTHCHECK` reads as
//! healthy (<https://docs.docker.com/reference/dockerfile/#healthcheck>). No
//! specification governs health probes: our own design.

use std::fmt;
use std::process::ExitCode;
use std::time::Duration;

use ferrohealth_server::health::READINESS;
use ferrohealth_server::healthcheck::{self, Get, GetError};
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
                eprintln!("ferrofed: not ready: {}", chain(&error));
                return ExitCode::FAILURE;
            }
        }
    }
    let line = Line(runtime.block_on(get.send()));
    if line.is_ready() {
        println!("ferrofed: {address}: {line}");
        ExitCode::SUCCESS
    } else {
        eprintln!("ferrofed: {address}: {line}");
        ExitCode::FAILURE
    }
}

/// The line the job prints for what the exchange found; the body is never
/// read, so only the status of an answer counts.
struct Line(Result<healthcheck::Answer, GetError>);

impl Line {
    /// The status readiness answered, when it answered.
    fn status(&self) -> Option<StatusCode> {
        match &self.0 {
            Ok(answer) => Some(answer.status()),
            Err(error) => error.status(),
        }
    }

    /// Whether readiness answered `200`.
    fn is_ready(&self) -> bool {
        self.status() == Some(StatusCode::OK)
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(status) = self.status() {
            return if status == StatusCode::OK {
                f.write_str("ready")
            } else {
                write!(f, "not ready: readiness answered {status}")
            };
        }
        match &self.0 {
            Ok(_) => f.write_str("ready"),
            Err(GetError::Connect { .. }) => f.write_str("not ready: no connection could be made"),
            Err(GetError::Timeout(timeout)) => {
                write!(f, "not ready: no answer within {} ms", timeout.as_millis())
            }
            Err(GetError::Handshake(error)) if other_certificate(error) => f.write_str(
                "not ready: the listener presents a certificate other than server.tls.certificate_file holds; send SIGHUP so the gateway reads it",
            ),
            Err(GetError::Handshake(error)) => {
                write!(f, "not ready: the TLS handshake failed: {}", chain(error))
            }
            Err(error) => write!(f, "not ready: the request failed: {}", chain(error)),
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
