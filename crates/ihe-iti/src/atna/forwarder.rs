// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The store-and-forward sender of ITI-20 messages (ITI TF-2 §3.20.4.1.1).
//!
//! Every message is stored in the [`Spool`] first and delivered from it,
//! oldest first, so a message counts as recorded once it is stored, and a
//! repository that cannot be reached delays its delivery without losing it.
//!
//! [`Forwarder::run`] is the delivery loop: it keeps one connection open,
//! writes each stored message to it, removes the message once it is written,
//! and backs off between attempts while the repository cannot be reached.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use secrecy::SecretSlice;
use tokio::sync::Notify;

use super::repository::{Connection, Repository};
use super::spool::{Depth, Spool, SpoolError};

/// The first wait after a failed delivery.
const FIRST_RETRY: Duration = Duration::from_millis(250);

/// The longest wait between delivery attempts.
const LAST_RETRY: Duration = Duration::from_secs(30);

/// What the forwarder reports of itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// What the spool holds, awaiting delivery.
    pub depth: Depth,
    /// The messages delivered since the forwarder started.
    pub delivered: u64,
    /// Whether the last attempt to reach the repository succeeded.
    pub reachable: bool,
    /// Whether the spool is on disk.
    pub durable: bool,
}

/// The store-and-forward sender to one Audit Record Repository.
#[derive(Debug)]
pub struct Forwarder {
    spool: Spool,
    repository: Repository,
    wake: Notify,
    delivered: AtomicU64,
    reachable: AtomicBool,
}

impl Forwarder {
    /// The sender of the messages in `spool` to `repository`.
    #[must_use]
    pub fn new(spool: Spool, repository: Repository) -> Arc<Self> {
        Arc::new(Self {
            spool,
            repository,
            wake: Notify::new(),
            delivered: AtomicU64::new(0),
            reachable: AtomicBool::new(true),
        })
    }

    /// Stores `frame` for delivery, returning once it is stored.
    ///
    /// # Errors
    ///
    /// The [`SpoolError`] of a spool that is full or cannot be written: the
    /// message is then neither stored nor delivered.
    pub async fn submit(&self, frame: SecretSlice<u8>) -> Result<(), SpoolError> {
        self.spool.push(frame).await?;
        self.wake.notify_one();
        Ok(())
    }

    /// What the forwarder holds and how its deliveries went.
    #[must_use]
    pub fn status(&self) -> Status {
        Status {
            depth: self.spool.depth(),
            delivered: self.delivered.load(Ordering::Relaxed),
            reachable: self.reachable.load(Ordering::Relaxed),
            durable: self.spool.is_durable(),
        }
    }

    /// Delivers the stored messages, oldest first, for as long as it runs.
    ///
    /// The caller spawns it once and aborts the task to stop it; a message
    /// in flight then stays stored and is delivered again by the next run.
    pub async fn run(self: Arc<Self>) {
        let mut connection: Option<Connection> = None;
        let mut retry = FIRST_RETRY;
        loop {
            let stored = match self.spool.oldest().await {
                Ok(Some(stored)) => stored,
                Ok(None) => {
                    self.wake.notified().await;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(error = %chain(&error), "the audit spool could not be read");
                    retry = pause(retry).await;
                    continue;
                }
            };
            let reusable = match connection.take() {
                Some(mut open) => (!open.is_closed().await).then_some(open),
                None => None,
            };
            let open = match reusable {
                Some(open) => open,
                None => match self.repository.connect().await {
                    Ok(open) => open,
                    Err(error) => {
                        self.reachable.store(false, Ordering::Relaxed);
                        tracing::warn!(
                            error = %chain(&error),
                            "the audit repository could not be reached; its messages stay spooled"
                        );
                        retry = pause(retry).await;
                        continue;
                    }
                },
            };
            let mut open = open;
            if let Err(error) = open.send(&stored.message).await {
                self.reachable.store(false, Ordering::Relaxed);
                tracing::warn!(
                    error = %chain(&error),
                    "an audit message could not be delivered; it stays spooled"
                );
                retry = pause(retry).await;
                continue;
            }
            connection = Some(open);
            self.reachable.store(true, Ordering::Relaxed);
            retry = FIRST_RETRY;
            match self.spool.remove(stored.sequence).await {
                Ok(()) => {
                    self.delivered.fetch_add(1, Ordering::Relaxed);
                }
                Err(error) => {
                    tracing::warn!(
                        error = %chain(&error),
                        "a delivered audit message could not be removed from the spool"
                    );
                    retry = pause(retry).await;
                }
            }
        }
    }
}

/// Waits `retry`, and returns the wait after it.
async fn pause(retry: Duration) -> Duration {
    tokio::time::sleep(retry).await;
    retry.saturating_mul(2).min(LAST_RETRY)
}

/// `error` with its causes, joined.
fn chain(error: &dyn std::error::Error) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}
