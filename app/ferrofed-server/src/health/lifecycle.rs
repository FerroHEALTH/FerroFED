// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the process is in its life: booting, serving or draining.
//!
//! Readiness answers `503` in every phase but [`Phase::Serving`], so an
//! orchestrator sends no request before boot completes, and stops sending
//! them from the moment the process is asked to stop, before the listener
//! closes and the drain starts. No specification governs health probes: our
//! own design.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use serde::Serialize;

/// One phase of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// The process has not finished building what it serves.
    Booting,
    /// The process serves.
    Serving,
    /// The process was asked to stop and finishes the requests in flight.
    Draining,
}

impl Phase {
    /// Returns the byte this phase is stored as.
    const fn code(self) -> u8 {
        match self {
            Self::Booting => 0,
            Self::Serving => 1,
            Self::Draining => 2,
        }
    }

    /// Returns the phase stored as `code`.
    const fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Booting,
            1 => Self::Serving,
            _ => Self::Draining,
        }
    }
}

/// A shared handle on the phase of the process.
///
/// Every clone reads and moves the same phase. The phase only moves forward:
/// booting, then serving, then draining, and draining is final.
#[derive(Debug, Clone)]
pub struct Lifecycle(Arc<AtomicU8>);

impl Default for Lifecycle {
    fn default() -> Self {
        Self(Arc::new(AtomicU8::new(Phase::Booting.code())))
    }
}

impl Lifecycle {
    /// Returns the current phase.
    #[must_use]
    pub fn phase(&self) -> Phase {
        Phase::from_code(self.0.load(Ordering::Acquire))
    }

    /// Records that boot completed, so readiness may answer `200`.
    ///
    /// A process that is already draining stays draining.
    pub fn booted(&self) {
        // NOTE: no specification governs this: our own design; a failed
        // exchange means the process already left booting, which is final here.
        let _moved: Result<u8, u8> = self.0.compare_exchange(
            Phase::Booting.code(),
            Phase::Serving.code(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Records that the process was asked to stop, so readiness answers `503`
    /// from now on.
    pub fn drain(&self) {
        self.0.store(Phase::Draining.code(), Ordering::Release);
    }
}

/// Completes `delay` after `signal` completes, moving `lifecycle` to
/// [`Phase::Draining`] the moment the signal arrives.
///
/// [`crate::serve`] hands this the process's stop signal and
/// `server.drain_delay_ms`, so readiness answers `503` while the listener
/// still accepts, and a load balancer that polls readiness, or removes a
/// terminating endpoint, stops routing to the process before it closes. The
/// Kubernetes documentation describes that removal as running alongside the
/// stop signal, not ahead of it
/// (<https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-termination>).
pub async fn drain_on<F>(signal: F, lifecycle: Lifecycle, delay: Duration)
where
    F: Future<Output = ()>,
{
    signal.await;
    lifecycle.drain();
    tracing::info!(
        drain_delay_ms = delay.as_millis(),
        "readiness withdrawn; the listener accepts until the drain delay ends"
    );
    tokio::time::sleep(delay).await;
    tracing::info!("the listener stops accepting; draining the requests in flight");
}

#[cfg(test)]
mod tests {
    use super::{Lifecycle, Phase};

    #[test]
    fn the_phase_moves_forward_and_draining_is_final() {
        let lifecycle = Lifecycle::default();
        assert_eq!(Phase::Booting, lifecycle.phase());
        let shared = lifecycle.clone();
        shared.booted();
        assert_eq!(Phase::Serving, lifecycle.phase(), "every clone moves");
        lifecycle.drain();
        assert_eq!(Phase::Draining, shared.phase());
        shared.booted();
        assert_eq!(
            Phase::Draining,
            lifecycle.phase(),
            "a draining process never serves again"
        );
    }
}
