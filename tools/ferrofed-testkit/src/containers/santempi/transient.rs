// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SanteMPI's transient fault, and the bounded retry the harness meets it
//! with.
//!
//! SanteMPI writes each change to the on-disk queue of its publish-and-
//! subscribe broker, and a write that collides with another on one queue file
//! is answered `500` with a `RestServiceFault` of type `IOException` naming a
//! sharing violation on that file. The fault says nothing about the request,
//! which succeeds when it is sent again (#1302). Every other refusal is an
//! answer, and fails at once. No specification governs the harness: our own
//! design.

use std::future::Future;
use std::time::Duration;

use http::StatusCode;

use super::SanteMpiError;

/// How many times a request that meets the transient fault is sent.
const ATTEMPTS: u32 = 5;

/// The wait before the second attempt, doubled before each later one.
const FIRST_WAIT: Duration = Duration::from_millis(500);

/// Whether `error` is SanteMPI's transient queue fault.
pub(super) fn transient(error: &SanteMpiError) -> bool {
    matches!(
        error,
        SanteMpiError::Refused { status, body, .. }
            if *status == StatusCode::INTERNAL_SERVER_ERROR
                && body.contains("<type>IOException</type>")
                && body.contains("Sharing violation")
    )
}

/// Runs `attempt` until it succeeds, fails with anything but the transient
/// fault, or has met that fault [`ATTEMPTS`] times, waiting between the
/// attempts; returns the last outcome.
///
/// `attempt` must be safe to repeat: one that creates a record first asks
/// whether the record exists.
pub(super) async fn retried<T, F, Fut>(mut attempt: F) -> Result<T, SanteMpiError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, SanteMpiError>>,
{
    let mut wait = FIRST_WAIT;
    let mut tried = 1;
    loop {
        match attempt().await {
            Err(error) if transient(&error) && tried < ATTEMPTS => {
                tokio::time::sleep(wait).await;
                wait = wait.saturating_mul(2);
                tried += 1;
            }
            outcome => return outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use http::StatusCode;

    use super::{ATTEMPTS, retried, transient};
    use crate::containers::santempi::SanteMpiError;

    /// The fault SanteMPI answered on run 40128635801.
    fn sharing_violation() -> SanteMpiError {
        SanteMpiError::Refused {
            step: "the AssigningAuthority create",
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: String::from(
                "<?xml version=\"1.0\"?>\n<RestServiceFault xmlns=\"http://santedb.org/fault\">\n  \
                 <type>IOException</type>\n  <message>Sharing violation on path \
                 /santedb/queue/sys.pubsub/00639284707253370097</message>\n  \
                 <policyOutcome>Deny</policyOutcome>\n</RestServiceFault>",
            ),
        }
    }

    #[test]
    fn the_queue_sharing_violation_is_transient() {
        assert!(transient(&sharing_violation()));
    }

    #[test]
    fn any_other_refusal_is_an_answer() {
        let refused = |status, body: &str| SanteMpiError::Refused {
            step: "a step",
            status,
            body: body.to_owned(),
        };
        assert!(!transient(&refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            "<RestServiceFault><type>DetectedIssueException</type></RestServiceFault>"
        )));
        assert!(!transient(&refused(
            StatusCode::CONFLICT,
            "<type>IOException</type> Sharing violation"
        )));
    }

    #[tokio::test]
    async fn a_transient_fault_is_tried_again_until_the_request_succeeds() {
        let calls = AtomicU32::new(0);
        let outcome = retried(|| async {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                Err(sharing_violation())
            } else {
                Ok("created")
            }
        })
        .await;
        assert_eq!("created", outcome.expect("the third attempt succeeds"));
        assert_eq!(3, calls.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn the_retry_is_bounded_and_returns_the_last_fault() {
        let calls = AtomicU32::new(0);
        let outcome: Result<(), _> = retried(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(sharing_violation())
        })
        .await;
        assert!(outcome.is_err_and(|error| transient(&error)));
        assert_eq!(ATTEMPTS, calls.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn any_other_refusal_fails_at_once() {
        let calls = AtomicU32::new(0);
        let outcome: Result<(), _> = retried(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(SanteMpiError::Refused {
                step: "a step",
                status: StatusCode::BAD_REQUEST,
                body: String::new(),
            })
        })
        .await;
        assert!(outcome.is_err());
        assert_eq!(1, calls.load(Ordering::SeqCst));
    }
}
