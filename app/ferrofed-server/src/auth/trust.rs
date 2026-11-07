// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client authentication in place: the [`Gate`] `[auth]` describes,
//! which a reload replaces whole.
//!
//! A request takes the gate in place once, when it reaches client
//! authentication, and verifies its caller against that gate to the end,
//! so a reload never changes the issuers under a request that is being
//! admitted. Every request that arrives after the reload is verified by the
//! new gate. The client listener and the admin listener share one [`Trust`],
//! so a reload changes both at once. No specification governs reloading:
//! our own design.

use std::sync::{Arc, PoisonError, RwLock};

use ferrohealth_smart::sender::replay::{MemoryReplayStore, ReplayStore};

use crate::auth::Gate;
use crate::config::auth::AuthSettings;
use crate::state::AppState;
use crate::store::replay::Proofs;

/// The gate in place, shared by every surface that admits callers.
#[derive(Debug)]
pub struct Trust {
    /// The gate every request that arrives now takes.
    current: RwLock<Arc<Gate>>,
}

impl Trust {
    /// Returns the trust `settings` describe, its `DPoP` proofs recorded in
    /// the memory of this process alone; no key set is read yet.
    #[must_use]
    pub fn new(settings: &AuthSettings) -> Self {
        Self::replaying(settings, Arc::new(MemoryReplayStore::new()))
    }

    /// Returns the trust `settings` describe, its `DPoP` proofs recorded in
    /// `replay`, which every gate that succeeds the first keeps; no key set
    /// is read yet.
    #[must_use]
    pub fn replaying(settings: &AuthSettings, replay: Arc<dyn ReplayStore>) -> Self {
        Self {
            current: RwLock::new(Arc::new(Gate::replaying(settings, replay))),
        }
    }

    /// Returns the trust `settings` describe over `state`: its `DPoP` proofs
    /// recorded in FerroFED's own store when `state` has one, which every
    /// replica shares (RFC 9449 §11.1), and in the memory of this process
    /// otherwise.
    #[must_use]
    pub fn of(settings: &AuthSettings, state: &AppState) -> Self {
        match state.store() {
            Some(store) => Self::replaying(settings, Arc::new(Proofs::new(Arc::clone(store)))),
            None => Self::new(settings),
        }
    }

    /// Returns the gate in place, which the caller keeps for the whole of
    /// one request.
    #[must_use]
    pub fn gate(&self) -> Arc<Gate> {
        // NOTE: no specification governs this: our own design; the lock guards
        // one `Arc` swap or clone, so a poisoned lock still holds a whole gate.
        Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Puts the gate `settings` describe in place of the running one, for
    /// every request that arrives afterwards; a request that already took
    /// the running one keeps it.
    pub fn replace(&self, settings: &AuthSettings) {
        let next = Arc::new(self.gate().succeeding(settings));
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = next;
    }
}
