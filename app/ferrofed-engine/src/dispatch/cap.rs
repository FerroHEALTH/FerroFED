// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-endpoint in-flight cap.
//!
//! At most so many requests to one endpoint leave the gateway at once, so
//! one slow member cannot hold every worker and one caller cannot load a
//! member past what the deployment gave it.
//!
//! A request waits for a slot until its own deadline, the per-node deadline
//! of §11.5. A request still waiting when it passes was abandoned at the
//! per-node timeout and is `time-out` (§11.5, N38), the status a request
//! whose deadline passed before it left already gets, with nothing sent;
//! §11.1 needs no new status for it. Its [`Contact`] is
//! [`Contact::Capped`], so the gateway's own surfaces count it as a
//! refusal of the cap and never as a request the node did not answer. No
//! specification governs the cap itself: our own design.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use ferrofed_registry::id::EndpointId;
use openehr_federation::outcome::{ErrorDetail, Outcome};
use openehr_its::rest::client::Transport;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::{Contact, NodeClient, NodeClients, NodeReply};

/// The `error` an endpoint whose cap stayed full is reported with.
pub const CAPPED: &str = "no answer before the deadline, which passed while the gateway's in-flight cap for this endpoint was full, so the request was not sent";

/// The cap of one endpoint: the slots its requests share.
#[derive(Debug, Clone)]
pub(crate) struct InFlight {
    slots: Arc<Semaphore>,
}

impl InFlight {
    /// A cap of `limit` requests in flight at once.
    fn new(limit: NonZeroU32) -> Self {
        // NOTE: no specification governs this: our own design; a limit past
        // what a semaphore holds is held to that maximum, which bounds nothing less.
        let permits = usize::try_from(limit.get())
            .unwrap_or(Semaphore::MAX_PERMITS)
            .min(Semaphore::MAX_PERMITS);
        Self {
            slots: Arc::new(Semaphore::new(permits)),
        }
    }
}

/// A slot of an endpoint's cap, held while its request is in flight, or
/// nothing where the endpoint has no cap.
#[derive(Debug)]
#[must_use = "the slot is released when it is dropped, so it lives as long as the request"]
pub(crate) struct Slot {
    _permit: Option<OwnedSemaphorePermit>,
}

/// The endpoint's cap stayed full until the request's deadline, so nothing
/// was sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the in-flight cap of endpoint {endpoint} stayed full until the deadline, so nothing was sent"
)]
pub struct Capped {
    /// The endpoint.
    pub endpoint: EndpointId,
}

impl<T: Transport + Clone> NodeClient<T> {
    /// Waits for a slot of this endpoint's cap until `deadline`.
    ///
    /// # Errors
    ///
    /// Returns [`Capped`] when no slot came free before `deadline`.
    pub(crate) async fn slot(&self, deadline: Instant) -> Result<Slot, Capped> {
        let Some(cap) = &self.in_flight else {
            return Ok(Slot { _permit: None });
        };
        let capped = || Capped {
            endpoint: self.endpoint.clone(),
        };
        if let Ok(permit) = Arc::clone(&cap.slots).try_acquire_owned() {
            return Ok(Slot {
                _permit: Some(permit),
            });
        }
        let waited = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            Arc::clone(&cap.slots).acquire_owned(),
        )
        .await;
        match waited {
            Ok(Ok(permit)) => Ok(Slot {
                _permit: Some(permit),
            }),
            // NOTE: no specification governs this: our own design; the cap is
            // never closed, so a closed one is read as a cap that stayed full.
            Ok(Err(_)) | Err(_) => Err(capped()),
        }
    }
}

impl<T: Transport + Clone> NodeClients<T> {
    /// These clients, each sending at most `limit` requests to its endpoint
    /// at once; a request past that waits for a slot until its deadline.
    ///
    /// Every clone of these clients shares the caps, and a set of clients
    /// built again, as a registry reload builds it, starts with caps of its
    /// own.
    #[must_use]
    pub fn with_in_flight_cap(mut self, limit: NonZeroU32) -> Self {
        for client in self.clients.values_mut() {
            client.in_flight = Some(InFlight::new(limit));
        }
        self
    }
}

/// The reply of a request whose endpoint's cap stayed full for
/// `latency_ms`: `time-out`, with nothing sent (§11.5, N38).
pub(crate) fn capped_reply(latency_ms: u64) -> NodeReply {
    NodeReply::Failed {
        outcome: capped_outcome(latency_ms),
        contact: Contact::Capped,
    }
}

/// The `time-out` outcome of a request whose endpoint's cap stayed full for
/// `latency_ms` (§11.5, N38).
#[must_use]
pub fn capped_outcome(latency_ms: u64) -> Outcome {
    Outcome::TimeOut {
        latency_ms,
        error: ErrorDetail::Text(CAPPED.to_owned()),
    }
}
