// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The delivery of one spool to its repository, which both recorders run:
//! the ITI-55 syslog trail ([`super::atna`]) and the BALP FHIR Feed
//! ([`super::balp`]).
//!
//! `ihe_atna`'s forwarder sends the spooled messages in order. A delivery
//! runs on the Tokio runtime that started it, is started again once that
//! runtime has shut down, and on the gateway's drain stops after the message
//! it is writing (ITI TF-2 §3.20.4.1.1; RFC 5425 §4.4).

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use ihe_atna::atna::forwarder::{Forwarder, Status};
use tokio::task::JoinHandle;

/// One spool's forwarder and the task that runs it.
pub struct Delivery {
    forwarder: Arc<Forwarder>,
    running: Mutex<Option<JoinHandle<()>>>,
}

impl fmt::Debug for Delivery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Delivery")
            .field("status", &self.forwarder.status())
            .finish_non_exhaustive()
    }
}

impl Delivery {
    /// The delivery through `forwarder`, not yet started.
    #[must_use]
    pub fn new(forwarder: Arc<Forwarder>) -> Self {
        Self {
            forwarder,
            running: Mutex::new(None),
        }
    }

    /// The forwarder a recorder submits to and reserves in.
    #[must_use]
    pub fn forwarder(&self) -> &Forwarder {
        &self.forwarder
    }

    /// Starts delivering the spooled messages on the Tokio runtime the caller
    /// runs on, unless a delivery is running already; without a runtime it
    /// does nothing, and the next recorded event starts it.
    ///
    /// A delivery started on a runtime that has since shut down has ended,
    /// so it is started again: a read on a short-lived runtime of its own
    /// never leaves the spool undelivered.
    pub fn start(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if running.as_ref().is_some_and(|task| !task.is_finished()) {
            return;
        }
        *running = Some(runtime.spawn(Arc::clone(&self.forwarder).run()));
    }

    /// What the spool holds and how the deliveries went.
    #[must_use]
    pub fn status(&self) -> Status {
        self.forwarder.status()
    }

    /// Stops delivering and waits for the delivery to end: the forwarder
    /// finishes the message it is writing, closes its repository connection
    /// with a TLS `close_notify` (RFC 5425 §4.4) and returns, so the frames
    /// it wrote last are not lost to a reset. A message not yet written
    /// stays in the spool. The gateway calls it once, on its drain.
    pub async fn stop(&self) {
        self.forwarder.stop();
        let running = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(task) = running
            && let Err(error) = task.await
        {
            tracing::warn!(%error, "an audit delivery ended without returning");
        }
    }
}
