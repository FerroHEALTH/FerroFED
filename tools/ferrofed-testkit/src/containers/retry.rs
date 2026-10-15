// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Starting a container or building an image again when the image transfer
//! breaks off.
//!
//! A container start pulls its image when Docker does not hold it, and an
//! image build pulls its base image. A pull that breaks off on the network
//! (`bytes remaining on stream`, a reset connection, a timeout) is tried
//! again after a doubling pause, at most [`ATTEMPTS`] times in all. Any other
//! failure, an image that is not found, a digest that does not match, or a
//! container that does not start, returns at once, so a real failure still
//! fails the test.
//!
//! No specification governs the harness: our own design.

use std::future::Future;
use std::time::Duration;

use testcontainers::bollard::errors::Error;
use testcontainers::core::client::ClientError;
use testcontainers::core::error::TestcontainersError;
use testcontainers::runners::{AsyncBuilder, AsyncRunner};
use testcontainers::{BuildableImage, ContainerAsync, ContainerRequest, Image};

/// How many times a start or a build is tried in all.
const ATTEMPTS: u32 = 4;

/// The pause before the second try, doubled before each later one.
const FIRST_PAUSE: Duration = Duration::from_secs(3);

/// Text that marks an image transfer that broke off on the network, matched
/// case-insensitively against what Docker relays.
const TRANSPORT_FAILURES: [&str; 14] = [
    "bytes remaining on stream",
    "unexpected eof",
    "connection reset by peer",
    "connection refused",
    "broken pipe",
    "i/o timeout",
    "tls handshake timeout",
    "timeout exceeded while awaiting headers",
    "request canceled",
    "context deadline exceeded",
    "temporary failure in name resolution",
    "502 bad gateway",
    "503 service unavailable",
    "504 gateway timeout",
];

/// Text that marks a failure no second try can mend, which wins over a
/// transport marker in the same message.
const DEFINITE_FAILURES: [&str; 7] = [
    "not found",
    "manifest unknown",
    "does not match",
    "verification failed",
    "unauthorized",
    "denied",
    "toomanyrequests",
];

/// Starts the container `request` builds, building it again and retrying
/// while the image pull breaks off on the network.
///
/// # Errors
///
/// Returns the start's own error when it is not a broken-off transfer, and
/// the last one when every try broke off.
pub(crate) async fn start<I: Image>(
    mut request: impl FnMut() -> ContainerRequest<I>,
) -> Result<ContainerAsync<I>, TestcontainersError> {
    with_retries(FIRST_PAUSE, || request().start()).await
}

/// Builds the image `image` describes, describing it again and retrying
/// while the pull of its base image breaks off on the network.
///
/// # Errors
///
/// Returns the build's own error when it is not a broken-off transfer, and
/// the last one when every try broke off.
pub(crate) async fn build<B: BuildableImage + Send>(
    mut image: impl FnMut() -> B,
) -> Result<B::Built, TestcontainersError> {
    with_retries(FIRST_PAUSE, || image().build_image()).await
}

/// Runs `attempt` until it succeeds, fails for a reason other than a
/// broken-off transfer, or has run [`ATTEMPTS`] times, pausing `pause` before
/// the second try and twice as long before each later one.
async fn with_retries<T, F>(
    mut pause: Duration,
    mut attempt: impl FnMut() -> F,
) -> Result<T, TestcontainersError>
where
    F: Future<Output = Result<T, TestcontainersError>>,
{
    for _ in 1..ATTEMPTS {
        match attempt().await {
            Err(error) if is_transport_failure(&error) => {
                tokio::time::sleep(pause).await;
                pause = pause.saturating_mul(2);
            }
            outcome => return outcome,
        }
    }
    attempt().await
}

/// Returns whether `error` is an image pull or build that broke off on the
/// network.
fn is_transport_failure(error: &TestcontainersError) -> bool {
    match error {
        TestcontainersError::Client(
            ClientError::PullImage { err, .. } | ClientError::BuildImage { err, .. },
        ) => is_transport(err),
        _ => false,
    }
}

/// Returns whether Docker's `error` reports a transfer that broke off.
fn is_transport(error: &Error) -> bool {
    match error {
        Error::DockerStreamError { error } => names_transport(error),
        Error::DockerResponseServerError { message, .. } => names_transport(message),
        Error::RequestTimeoutError | Error::IOError { .. } | Error::HyperResponseError { .. } => {
            true
        }
        _ => false,
    }
}

/// Returns whether `message` names a broken-off transfer and no definite
/// failure.
fn names_transport(message: &str) -> bool {
    let message = message.to_lowercase();
    let names = |markers: &[&str]| markers.iter().any(|marker| message.contains(marker));
    names(&TRANSPORT_FAILURES) && !names(&DEFINITE_FAILURES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn pull(err: Error) -> TestcontainersError {
        TestcontainersError::Client(ClientError::PullImage {
            descriptor: "postgres:15.19".to_owned(),
            err,
        })
    }

    fn stream(error: &str) -> Error {
        Error::DockerStreamError {
            error: error.to_owned(),
        }
    }

    #[test]
    fn a_pull_that_broke_off_mid_stream_is_a_transport_failure() {
        let error = pull(stream(
            "failed to register layer: unexpected EOF: 1180 bytes remaining on stream",
        ));
        assert!(is_transport_failure(&error));
    }

    #[test]
    fn a_reset_connection_to_the_registry_is_a_transport_failure() {
        let error = pull(Error::DockerResponseServerError {
            status_code: 500,
            message: "Get \"https://registry-1.docker.io/v2/\": read tcp: connection reset by peer"
                .to_owned(),
        });
        assert!(is_transport_failure(&error));
    }

    #[test]
    fn a_broken_daemon_connection_is_a_transport_failure() {
        let error = pull(Error::IOError {
            err: std::io::Error::from(std::io::ErrorKind::ConnectionReset),
        });
        assert!(is_transport_failure(&error));
    }

    #[test]
    fn an_image_that_is_not_found_fails_at_once() {
        let error = pull(Error::DockerResponseServerError {
            status_code: 404,
            message: "manifest unknown: manifest unknown".to_owned(),
        });
        assert!(!is_transport_failure(&error));
    }

    #[test]
    fn a_digest_mismatch_fails_at_once() {
        let error = pull(stream(
            "filesystem layer verification failed for digest sha256:00: digest does not match, unexpected EOF",
        ));
        assert!(!is_transport_failure(&error));
    }

    #[test]
    fn a_failed_build_step_fails_at_once() {
        let error = TestcontainersError::Client(ClientError::BuildImage {
            descriptor: "ferrofed-e2e/example:1".to_owned(),
            err: stream("process \"/bin/sh -c false\" did not complete successfully: exit code: 1"),
        });
        assert!(!is_transport_failure(&error));
    }

    #[test]
    fn a_container_that_fails_to_start_fails_at_once() {
        let error = TestcontainersError::Client(ClientError::StartContainer(stream(
            "connection reset by peer",
        )));
        assert!(!is_transport_failure(&error));
    }

    #[tokio::test]
    async fn a_transport_failure_is_tried_again_until_it_succeeds() {
        let tries = AtomicU32::new(0);
        let outcome = with_retries(Duration::ZERO, || async {
            if tries.fetch_add(1, Ordering::Relaxed) < 2 {
                Err(pull(stream("bytes remaining on stream")))
            } else {
                Ok(())
            }
        })
        .await;
        assert!(outcome.is_ok());
        assert_eq!(tries.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn a_transport_failure_is_tried_a_bounded_number_of_times() {
        let tries = AtomicU32::new(0);
        let outcome: Result<(), _> = with_retries(Duration::ZERO, || async {
            tries.fetch_add(1, Ordering::Relaxed);
            Err(pull(stream("bytes remaining on stream")))
        })
        .await;
        assert!(outcome.is_err());
        assert_eq!(tries.load(Ordering::Relaxed), ATTEMPTS);
    }

    #[tokio::test]
    async fn a_definite_failure_is_tried_once() {
        let tries = AtomicU32::new(0);
        let outcome: Result<(), _> = with_retries(Duration::ZERO, || async {
            tries.fetch_add(1, Ordering::Relaxed);
            Err(pull(Error::DockerResponseServerError {
                status_code: 404,
                message: "pull access denied, repository does not exist".to_owned(),
            }))
        })
        .await;
        assert!(outcome.is_err());
        assert_eq!(tries.load(Ordering::Relaxed), 1);
    }
}
