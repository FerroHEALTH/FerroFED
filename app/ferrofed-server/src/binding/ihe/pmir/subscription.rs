// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-94 subscription the identity feed keeps (PMIR 1.6.0 §2:3.94.4).
//!
//! The gateway holds at most one subscription, and never leaves one it made
//! behind for the Registry to send a second feed to:
//!
//! - Before it creates one, it searches the Registry for its own, by the
//!   channel endpoint (`GET [base]/Subscription?url=<callback>`, the R4
//!   `Subscription` search parameter), and adopts one whose criteria and
//!   channel are its request's. A create the Registry answered late, or
//!   whose answer was lost, is found that way. A Registry that answers the
//!   search `400` or `404` does not support it, and the gateway creates.
//! - A create answered `201` with no `Location`, or one outside the base,
//!   leaves a subscription the gateway cannot read or delete. It never creates
//!   again until a restart, and reports the Registry `failing` with the fault
//!   `unmanageable`.
//! - A subscription the Registry reports `error` or `off` is deleted and
//!   created again (§2:3.94.4.1.3, §2:3.94.4.4), and one it no longer holds
//!   is created again.
//! - Each failed check doubles the wait before the next, from
//!   `check_interval_s` to 32 times it, and a check that succeeds resets it.
//! - At a drain the loop is stopped first, letting the check in flight end,
//!   and the subscription is deleted after it, or found by the search and
//!   deleted when the gateway never learned where it was.
//!
//! No specification governs the retry policy or the drain: our own design.

use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;
use ihe_iti::pmir::error::{SubscribeError, SubscriptionMalformation};
use ihe_iti::pmir::subscription::{Search, Subscribed, SubscriptionStatus};
use serde::Serialize;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::IdentityFeed;
use ferrofed_registry::health::Observed;

/// How many doublings the wait between two failed checks grows by at most:
/// 32 times `check_interval_s`.
const MAX_DOUBLINGS: u32 = 5;

/// How many exchanges with the Registry one check may make at most, which
/// bounds how long a drain waits for the check in flight.
const EXCHANGES_PER_CHECK: u32 = 4;

/// Why the Registry is not up, as `GET /health/dependencies` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum RegistryFault {
    /// The Registry did not answer in time, or could not be reached.
    Unreachable,
    /// The Registry refused a search, a create, a read or a delete.
    Refused,
    /// The Registry's answer does not hold to ITI-94.
    Malformed,
    /// The Registry created a subscription without a `Location` under its
    /// base, which the gateway can neither read nor delete; it creates no
    /// other until a restart.
    Unmanageable,
    /// The audit record of an exchange with the Registry could not be
    /// stored, so its answer was not used (PMIR §2:3.94.5.1).
    AuditFailed,
}

impl RegistryFault {
    /// The fault as `identity_registry_fault` names it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::Refused => "refused",
            Self::Malformed => "malformed",
            Self::Unmanageable => "unmanageable",
            Self::AuditFailed => "audit-failed",
        }
    }

    /// The state a fault shows the Registry in: no answer is down, any
    /// answer failing.
    #[must_use]
    pub const fn observed(self) -> Observed {
        match self {
            Self::Unreachable => Observed::Down,
            Self::Refused | Self::Malformed | Self::Unmanageable | Self::AuditFailed => {
                Observed::Failing
            }
        }
    }
}

/// The subscription the gateway holds.
#[derive(Debug, Clone, Default)]
pub(super) enum Held {
    /// None the gateway knows of.
    #[default]
    Nothing,
    /// One it can read and delete.
    Managed(Subscribed),
    /// One it created and cannot locate; it creates no other.
    Unmanageable,
}

/// What the feed knows of the Registry.
#[derive(Debug)]
pub(super) struct Watch {
    pub(super) held: Held,
    pub(super) observed: Observed,
    pub(super) fault: Option<RegistryFault>,
    pub(super) failures: u32,
}

impl Default for Watch {
    fn default() -> Self {
        Self {
            held: Held::Nothing,
            observed: Observed::Unknown,
            fault: None,
            failures: 0,
        }
    }
}

/// The wait before the next check after `failures` failed checks in a row:
/// `interval` doubled once per failure, at most 32 times `interval`.
#[must_use]
pub fn backoff(interval: Duration, failures: u32) -> Duration {
    // NOTE: no specification governs this: our own design; the doubling spares a
    // Registry that is down, and the cap keeps a recovery within minutes.
    interval.saturating_mul(1_u32 << failures.min(MAX_DOUBLINGS))
}

/// The fault `error` shows, logged with its status and its error chain, which
/// carry no identifier.
fn fault_of(message: &'static str, error: &SubscribeError) -> RegistryFault {
    tracing::warn!(
        status = error.status().map(|status| status.as_u16()),
        error = crate::chain(error),
        "{message}"
    );
    match error {
        SubscribeError::Rejected { .. } => RegistryFault::Refused,
        SubscribeError::Malformed(_) => RegistryFault::Malformed,
        SubscribeError::Audit(_) => RegistryFault::AuditFailed,
        _ => RegistryFault::Unreachable,
    }
}

impl IdentityFeed {
    /// Brings the subscription to a state the Registry sends the feed in, and
    /// records what that showed of the Registry.
    ///
    /// Without a subscription it adopts its own from a search, or creates one;
    /// with one it reads its status, and replaces one in `error` or `off` or
    /// one the Registry lost. After an unmanageable create it asks nothing.
    pub async fn check(&self) -> Observed {
        let held = self.watch().held.clone();
        let outcome = match held {
            Held::Unmanageable => Err(RegistryFault::Unmanageable),
            Held::Managed(subscribed) => self.recheck(&subscribed).await,
            Held::Nothing => self.establish().await,
        };
        let mut watch = self.watch();
        match outcome {
            Ok(()) => {
                watch.observed = Observed::Up;
                watch.fault = None;
                watch.failures = 0;
            }
            Err(fault) => {
                watch.observed = fault.observed();
                watch.fault = Some(fault);
                watch.failures = watch.failures.saturating_add(1);
            }
        }
        watch.observed
    }

    /// Reads `subscribed` back, and replaces it when the Registry reports it
    /// inactive or no longer holds it.
    async fn recheck(&self, subscribed: &Subscribed) -> Result<(), RegistryFault> {
        match self.subscriber.status(subscribed, self.timeout).await {
            Ok(SubscriptionStatus::Requested | SubscriptionStatus::Active) => Ok(()),
            Ok(status) => {
                tracing::warn!(
                    status = status.as_str(),
                    "the Patient Identity Registry reports the PMIR subscription inactive; it is replaced"
                );
                self.delete(subscribed).await?;
                self.establish().await
            }
            Err(error)
                if matches!(
                    error.status(),
                    Some(StatusCode::NOT_FOUND | StatusCode::GONE)
                ) =>
            {
                tracing::warn!(
                    "the Patient Identity Registry no longer holds the PMIR subscription"
                );
                self.watch().held = Held::Nothing;
                self.establish().await
            }
            Err(error) => Err(fault_of("the PMIR subscription could not be read", &error)),
        }
    }

    /// Adopts the gateway's own subscription from a search, deleting any
    /// other of its own the Registry lists, or creates one.
    async fn establish(&self) -> Result<(), RegistryFault> {
        match self.subscriber.find(&self.request, self.timeout).await {
            Ok(Search::Found(listed)) => {
                let mut adopted = None;
                for found in listed {
                    let usable = matches!(
                        found.status,
                        SubscriptionStatus::Requested | SubscriptionStatus::Active
                    );
                    if usable && adopted.is_none() {
                        adopted = Some(found.subscribed);
                    } else {
                        self.delete(&found.subscribed).await?;
                    }
                }
                if let Some(subscribed) = adopted {
                    tracing::info!("the PMIR subscription the Registry already held was adopted");
                    self.watch().held = Held::Managed(subscribed);
                    return Ok(());
                }
            }
            Ok(_) => {
                tracing::debug!(
                    "the Patient Identity Registry does not search subscriptions by url; creating"
                );
            }
            Err(error) => {
                return Err(fault_of(
                    "the PMIR subscriptions could not be searched",
                    &error,
                ));
            }
        }
        self.create().await
    }

    /// Creates the subscription, and stops creating after one the gateway
    /// cannot locate.
    async fn create(&self) -> Result<(), RegistryFault> {
        match self.subscriber.subscribe(&self.request, self.timeout).await {
            Ok(subscribed) => {
                tracing::info!("the PMIR subscription was created");
                self.watch().held = Held::Managed(subscribed);
                Ok(())
            }
            Err(SubscribeError::Malformed(
                SubscriptionMalformation::NoLocation | SubscriptionMalformation::Location,
            )) => {
                tracing::error!(
                    "the Patient Identity Registry created the PMIR subscription without a Location under its base; it may hold one subscription the gateway cannot manage, and none is created again until a restart"
                );
                self.watch().held = Held::Unmanageable;
                Err(RegistryFault::Unmanageable)
            }
            Err(error) => Err(fault_of(
                "the PMIR subscription could not be created",
                &error,
            )),
        }
    }

    /// Deletes `subscribed` (§2:3.94.4.5).
    async fn delete(&self, subscribed: &Subscribed) -> Result<(), RegistryFault> {
        self.subscriber
            .unsubscribe(subscribed, self.timeout)
            .await
            .map_err(|error| fault_of("the PMIR subscription could not be deleted", &error))?;
        tracing::info!("the PMIR subscription was deleted");
        Ok(())
    }

    /// Deletes the subscription the gateway holds, or, when it knows of none,
    /// every one of its own a search finds, so the Registry stops sending the
    /// feed (§2:3.94.4.5); a failure is logged and the subscription is left
    /// to the Registry.
    pub async fn unsubscribe(&self) {
        let held = std::mem::take(&mut self.watch().held);
        if let Held::Managed(subscribed) = held {
            // NOTE: no specification governs this: our own design; a failed delete
            // is logged in `delete`, and the drain goes on.
            if self.delete(&subscribed).await.is_ok() {
                return;
            }
        }
        let listed = match self.subscriber.find(&self.request, self.timeout).await {
            Ok(Search::Found(listed)) => listed,
            Ok(_) => return,
            Err(error) => {
                fault_of(
                    "the PMIR subscriptions could not be searched at the drain",
                    &error,
                );
                return;
            }
        };
        let mut left = 0_usize;
        for found in listed {
            if self.delete(&found.subscribed).await.is_err() {
                left = left.saturating_add(1);
            }
        }
        if left > 0 {
            tracing::warn!(
                left,
                "PMIR subscriptions the gateway could not delete are left to the Registry"
            );
        }
    }

    /// Starts checking the subscription now, and again after each
    /// [`backoff`], until the returned [`Running`] is drained.
    #[must_use]
    pub fn start(self: &Arc<Self>) -> Running {
        let stop = Arc::new(Notify::new());
        let task = tokio::spawn(Arc::clone(self).keep_subscribed(Arc::clone(&stop)));
        Running {
            feed: Arc::clone(self),
            stop,
            task,
        }
    }

    /// Checks the subscription until `stop` is notified; a check in flight
    /// always ends, so what it created is known to the drain.
    async fn keep_subscribed(self: Arc<Self>, stop: Arc<Notify>) {
        loop {
            self.check().await;
            let wait = backoff(self.check_interval, self.failures());
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = stop.notified() => return,
            }
        }
    }
}

/// The subscription loop of a running gateway.
#[derive(Debug)]
pub struct Running {
    feed: Arc<IdentityFeed>,
    stop: Arc<Notify>,
    task: JoinHandle<()>,
}

impl Running {
    /// Stops the loop, lets the check in flight end, and deletes the
    /// subscription, so none is left for the Registry to send to and none is
    /// created after the delete.
    ///
    /// A check that does not end within the time its exchanges may take is
    /// aborted, and the delete then finds the subscription by searching.
    pub async fn drain(mut self) {
        self.stop.notify_one();
        let bound = self.feed.timeout.saturating_mul(EXCHANGES_PER_CHECK);
        match tokio::time::timeout(bound, &mut self.task).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(
                    panicked = error.is_panic(),
                    "the PMIR subscription loop ended abnormally"
                );
            }
            Err(_elapsed) => {
                tracing::warn!("the PMIR subscription check in flight was abandoned at the drain");
                self.task.abort();
            }
        }
        self.feed.unsubscribe().await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::backoff;

    #[test]
    fn the_wait_doubles_per_failure_up_to_32_times_the_interval() {
        let interval = Duration::from_secs(60);
        let waits: Vec<u64> = (0..8)
            .map(|failures| backoff(interval, failures).as_secs())
            .collect();
        assert_eq!(vec![60, 120, 240, 480, 960, 1920, 1920, 1920], waits);
        assert_eq!(
            Duration::MAX,
            backoff(Duration::MAX, 3),
            "the wait saturates rather than wrapping"
        );
    }
}
