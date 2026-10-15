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

use crate::auth::Gate;
use crate::config::auth::AuthSettings;

/// The gate in place, shared by every surface that admits callers.
#[derive(Debug)]
pub struct Trust {
    /// The gate every request that arrives now takes.
    current: RwLock<Arc<Gate>>,
}

impl Trust {
    /// Returns the trust `settings` describe; no key set is read yet.
    #[must_use]
    pub fn new(settings: &AuthSettings) -> Self {
        Self {
            current: RwLock::new(Arc::new(Gate::new(settings))),
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
