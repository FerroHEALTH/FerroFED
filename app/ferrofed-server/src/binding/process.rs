// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the bindings hold for the life of the process, beside every
//! federation a reload builds.
//!
//! A binding that records through its own instruments, runs a process of its
//! own, such as an identity feed, or reads the registry from a source of its
//! own, such as a care services directory, adds one field here, under its
//! feature; the server reaches each through the generic methods. No
//! specification governs the process model: our own design.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;

use crate::auth::trust::Trust;
use crate::binding::error::BindingError;
#[cfg(feature = "binding-ihe")]
use crate::binding::ihe;
use crate::binding::seam::{FeedHost, RegistrySink};
use crate::config::settings::Settings;
use ferrofed_operator_api::health::Indication;

/// What the bindings run for the life of the process, beside every
/// federation a reload builds: one field per binding that runs anything.
#[derive(Debug, Default)]
pub struct Processes {
    /// The IHE binding's care services directory and identity feed.
    #[cfg(feature = "binding-ihe")]
    pub(crate) ihe: ihe::process::Processes,
}

#[cfg_attr(
    not(feature = "binding-ihe"),
    expect(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        clippy::needless_pass_by_value,
        reason = "with no binding that runs a process compiled in, every method is a no-op"
    )
)]
impl Processes {
    /// Returns what `settings` describe for the bindings to run.
    ///
    /// # Errors
    ///
    /// The [`BindingError`] of a process that cannot be built.
    pub(crate) fn build(
        #[cfg_attr(
            not(feature = "binding-ihe"),
            expect(unused_variables, reason = "no compiled binding runs a process")
        )]
        settings: &Settings,
    ) -> Result<Self, BindingError> {
        Ok(Self {
            #[cfg(feature = "binding-ihe")]
            ihe: ihe::process::Processes::build(settings)?,
        })
    }

    /// Takes over the registry sources the boot opened.
    pub(crate) fn watch(
        &mut self,
        #[cfg_attr(
            not(feature = "binding-ihe"),
            expect(unused_variables, reason = "no compiled binding has a registry source")
        )]
        sources: Sources,
    ) {
        #[cfg(feature = "binding-ihe")]
        if let Some(directory) = sources.ihe {
            self.ihe.directory = Some(directory);
        }
    }

    /// Returns what each process indicates on `GET {base}/operator/dependencies`.
    pub(crate) fn indicate(&self) -> Vec<(&'static str, Indication)> {
        #[cfg(feature = "binding-ihe")]
        {
            self.ihe.indicate()
        }
        #[cfg(not(feature = "binding-ihe"))]
        {
            Vec::new()
        }
    }

    /// Returns `surface` with the routes the processes serve, each that
    /// admits its own callers verified by the gate `trust` holds.
    #[cfg_attr(
        not(feature = "binding-ihe"),
        expect(
            unused_variables,
            reason = "without the IHE binding no process serves a route of its own"
        )
    )]
    pub(crate) fn routes<H: FeedHost>(
        &self,
        surface: Router<Arc<H>>,
        trust: &Arc<Trust>,
    ) -> Router<Arc<H>> {
        #[cfg(feature = "binding-ihe")]
        {
            self.ihe.routes(surface, trust)
        }
        #[cfg(not(feature = "binding-ihe"))]
        {
            surface
        }
    }

    /// Starts every process, `reloader` replacing the registry a source
    /// changes.
    pub(crate) fn start<S: RegistrySink>(
        &self,
        #[cfg_attr(
            not(feature = "binding-ihe"),
            expect(unused_variables, reason = "no compiled binding runs a process")
        )]
        sink: &Arc<S>,
    ) -> Running {
        Running {
            #[cfg(feature = "binding-ihe")]
            ihe: self.ihe.start(sink),
        }
    }
}

/// The processes a running gateway started.
#[derive(Debug)]
pub struct Running {
    /// The IHE binding's identity feed subscription.
    #[cfg(feature = "binding-ihe")]
    ihe: ihe::process::Running,
}

impl Running {
    /// Stops every process that holds something at a remote service, within
    /// `budget`, the `gateway.bindings_drain_timeout_ms` the shipped grace
    /// periods leave room for.
    pub(crate) async fn drain(self, budget: Duration) -> Drained {
        within(budget, self.stop()).await
    }

    /// Stops every process, for as long as each one's own timeouts allow.
    #[cfg_attr(
        not(feature = "binding-ihe"),
        expect(clippy::unused_async, reason = "no compiled binding runs a process")
    )]
    async fn stop(self) {
        #[cfg(feature = "binding-ihe")]
        self.ihe.drain().await;
    }
}

/// How the bindings' drain ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Drained {
    /// Every process stopped within the budget.
    Stopped,
    /// The budget elapsed first, and the processes still stopping were
    /// abandoned, so what they hold at a remote service may be left there.
    Abandoned,
}

/// Runs `drain` until it ends or `budget` elapses, whichever comes first.
async fn within(budget: Duration, drain: impl Future<Output = ()>) -> Drained {
    match tokio::time::timeout(budget, drain).await {
        Ok(()) => Drained::Stopped,
        Err(_elapsed) => Drained::Abandoned,
    }
}

/// The registry sources a binding opened at boot, which the gateway keeps
/// its registry in step with.
#[derive(Debug, Default)]
pub struct Sources {
    /// The IHE binding's care services directory.
    #[cfg(feature = "binding-ihe")]
    pub(crate) ihe: Option<Arc<ihe::mcsd::registry::DirectoryRegistry>>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Drained, within};

    #[tokio::test(start_paused = true)]
    async fn a_drain_that_ends_within_its_budget_has_stopped() {
        let drain = tokio::time::sleep(Duration::from_secs(4));
        assert_eq!(
            Drained::Stopped,
            within(Duration::from_secs(5), drain).await
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_drain_still_running_when_its_budget_elapses_is_abandoned() {
        let started = tokio::time::Instant::now();
        let drained = within(Duration::from_secs(5), std::future::pending()).await;
        assert_eq!(Drained::Abandoned, drained);
        assert_eq!(Duration::from_secs(5), started.elapsed());
    }
}
