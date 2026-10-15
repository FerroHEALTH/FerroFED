// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Keeping a replica on the revision the store marks current.
//!
//! Under `config.ui = "managed"` each gateway reads the store's current mark
//! every `config.poll_ms`. When another replica sharing the store applied or
//! rolled back a revision, the mark names a revision this replica does not
//! run, and the watch applies it through the same reload path an apply takes
//! ([`Revisions::follow`]). A revision this replica refuses is logged and
//! counted, and the replica keeps its running configuration, which
//! `GET /admin/config` then reports as out of step. Each read first rolls
//! back a running revision whose confirmation is past its deadline. Under `config.ui =
//! "off"` no watch runs. No specification governs this: our own design.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::metrics::FollowResult;
use crate::reload::Reloader;
use crate::revision::service::{Followed, RevisionError, Revisions};
use crate::state::AppState;

/// The watch of a running gateway over the store's current mark.
#[derive(Debug)]
pub struct Watch {
    stop: Arc<Notify>,
    task: JoinHandle<()>,
}

/// Starts watching the store of `revisions` every `every`.
///
/// A revision another replica marked current is applied through
/// `reloader`, and what each read found is recorded on the metrics of
/// `state`. It returns `None` when the configuration is not managed, so
/// nothing runs.
#[must_use]
pub fn start(
    revisions: Option<&Arc<Revisions>>,
    reloader: &Arc<Reloader>,
    state: &Arc<AppState>,
    every: Duration,
) -> Option<Watch> {
    let revisions = Arc::clone(revisions?);
    let stop = Arc::new(Notify::new());
    let task = tokio::spawn(watch(
        revisions,
        Arc::clone(reloader),
        Arc::clone(state),
        every,
        Arc::clone(&stop),
    ));
    Some(Watch { stop, task })
}

impl Watch {
    /// Stops the watch, letting a read in flight end within `budget`, past
    /// which it is abandoned.
    pub async fn stop(mut self, budget: Duration) {
        self.stop.notify_one();
        match tokio::time::timeout(budget, &mut self.task).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(
                panicked = error.is_panic(),
                "the configuration revision watch ended abnormally"
            ),
            Err(_elapsed) => {
                tracing::warn!(
                    "the configuration revision read in flight was abandoned at the drain"
                );
                self.task.abort();
            }
        }
    }
}

/// Reads the mark every `every` until `stop` is notified.
async fn watch(
    revisions: Arc<Revisions>,
    reloader: Arc<Reloader>,
    state: Arc<AppState>,
    every: Duration,
    stop: Arc<Notify>,
) {
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticks.tick() => {}
            () = stop.notified() => return,
        }
        let (revisions, reloader) = (Arc::clone(&revisions), Arc::clone(&reloader));
        let followed = tokio::task::spawn_blocking(move || {
            expire(&revisions, &reloader);
            revisions.follow(&reloader)
        })
        .await;
        let metrics = state.metrics();
        match followed {
            Ok(Ok(Followed::InStep)) => metrics.in_step(true),
            Ok(Ok(Followed::Applied(revision))) => {
                metrics.followed(FollowResult::Applied);
                metrics.in_step(true);
                tracing::info!(
                    revision = %revision,
                    "this replica applied the revision another replica marked running"
                );
            }
            Ok(Ok(Followed::Drifting(_))) => metrics.in_step(false),
            Ok(Err(RevisionError::Store(error))) => tracing::error!(
                error = crate::chain(&error),
                "the configuration revision store could not be read; the running configuration stays"
            ),
            Ok(Err(refusal)) => {
                metrics.followed(FollowResult::Refused);
                metrics.in_step(false);
                tracing::error!(
                    error = crate::chain(&refusal),
                    "this replica refused the revision marked running, and keeps its running configuration"
                );
            }
            Err(error) => tracing::error!(
                panicked = error.is_panic(),
                "the configuration revision read did not finish"
            ),
        }
    }
}

/// Rolls the running revision back when it awaits a confirmation past its
/// deadline ([`Revisions::expire`]), logging a rollback the store or the
/// reload refused; the next read tries again.
fn expire(revisions: &Revisions, reloader: &Reloader) {
    match revisions.expire(reloader, jiff::Timestamp::now()) {
        Ok(Some((revision, _mark))) => tracing::warn!(
            revision = %revision.id,
            "the running revision was not confirmed by its deadline and is rolled back"
        ),
        Ok(None) => {}
        Err(error) => tracing::error!(
            error = crate::chain(&error),
            "a revision past its confirmation deadline could not be rolled back; it keeps running until the next read"
        ),
    }
}
