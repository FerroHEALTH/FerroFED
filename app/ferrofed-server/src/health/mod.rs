// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's own indicators of the shared health family, and the last
//! observed state of every dependency.
//!
//! `ferrohealth-server` serves `{base}/health`, `{base}/health/liveness` and
//! `{base}/health/readiness` over the indicator registry and the lifecycle
//! of `ferrohealth-health` (#1397). Readiness answers `200` only while the
//! process serves and no indicator is down, and names each indicator and its
//! state, never its detail. An indicator checks one of the gateway's own
//! subsystems (the configuration and the registry loaded, the stored-query
//! store open, the outbound clients built). A member node or the identity
//! source never gates readiness: their last observed state is
//! [`dependencies`]'s, reported on the operator surface. No specification
//! governs health probes: our own design.

pub mod dependencies;

use ferrohealth_health::registry::{Check, HealthIndicator, Outcome};

/// A subsystem that is up once it is built: the configuration read, the
/// registry loaded, the outbound clients and the stored-query store made.
///
/// Boot refuses to start without it, so the check answers `up` and its name
/// tells readiness which subsystems this process built.
#[derive(Debug, Clone, Copy)]
pub struct Built(pub &'static str);

impl HealthIndicator for Built {
    fn name(&self) -> &'static str {
        self.0
    }

    fn check(&self) -> Check<'_> {
        Box::pin(std::future::ready(Outcome::up()))
    }
}
