// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution bindings of §12.5.1 step 2: the `{node, ehr_id}` set a
//! client session's resolution produced, so a follow-up on a path `ehr_id`
//! can be routed to the node that holds it.
//!
//! The bindings belong to the client session (decision A20,
//! `docs/architecture.md` sections 6 and 8): held in memory, keyed by the
//! session and by the `ehr_id`, never by the patient identifier, and dropped
//! when the session's time-to-live passes. Nothing is written to disk and
//! nothing derived from a patient identifier is kept.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ferrofed_registry::id::{EhrId, NodeId};

/// The authenticated client session a binding belongs to.
///
/// It is the session the gateway's client authentication establishes (§13.1),
/// never a value derived from a patient identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionKey(String);

impl SessionKey {
    /// Names a session by the identifier client authentication gave it.
    #[must_use]
    pub fn new(session: impl Into<String>) -> Self {
        Self(session.into())
    }
}

/// What the bindings of a session say about one `ehr_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// No binding of the session names the `ehr_id`.
    None,
    /// Exactly one member holds it (§12.5.1 step 2).
    One(NodeId),
    /// Several members resolved the same `ehr_id`: step 2 does not yield an
    /// unambiguous answer, so routing moves to the next step (N41, N42).
    Several(Vec<NodeId>),
}

/// One session's bindings and when they expire.
struct Session {
    expires: Instant,
    by_ehr: BTreeMap<String, BTreeSet<NodeId>>,
}

/// The resolution bindings of every live session.
///
/// `Debug` reports how many sessions are held and none of their bindings.
pub struct ResolutionBindings {
    ttl: Duration,
    sessions: Mutex<BTreeMap<SessionKey, Session>>,
}

impl ResolutionBindings {
    /// Bindings that live `ttl` past the resolution that recorded them, a
    /// correctness bound: a binding older than it is never routed on.
    #[must_use]
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    /// Records the `{node, ehr_id}` pairs a resolution of `session` produced
    /// at `now`, and renews the session's time-to-live.
    pub fn record<'a>(
        &self,
        session: &SessionKey,
        now: Instant,
        pairs: impl IntoIterator<Item = (&'a NodeId, &'a EhrId)>,
    ) {
        let Some(expires) = now.checked_add(self.ttl) else {
            return;
        };
        let mut sessions = self.lock();
        sessions.retain(|_, held| held.expires > now);
        let held = sessions.entry(session.clone()).or_insert_with(|| Session {
            expires,
            by_ehr: BTreeMap::new(),
        });
        held.expires = expires;
        for (node, ehr_id) in pairs {
            held.by_ehr
                .entry(ehr_key(ehr_id))
                .or_default()
                .insert(node.clone());
        }
    }

    /// What the live bindings of `session` say about `ehr_id` at `now`.
    #[must_use]
    pub fn lookup(&self, session: &SessionKey, now: Instant, ehr_id: &EhrId) -> Bound {
        let sessions = self.lock();
        let Some(held) = sessions.get(session).filter(|held| held.expires > now) else {
            return Bound::None;
        };
        match held.by_ehr.get(&ehr_key(ehr_id)) {
            None => Bound::None,
            Some(nodes) => {
                let mut nodes = nodes.iter().cloned();
                match (nodes.next(), nodes.next()) {
                    (None, _) => Bound::None,
                    (Some(only), None) => Bound::One(only),
                    (Some(first), Some(second)) => {
                        Bound::Several([first, second].into_iter().chain(nodes).collect())
                    }
                }
            }
        }
    }

    /// Drops every binding of `session`, when it ends or when a consent
    /// denial voids what it resolved (decision A20).
    pub fn forget(&self, session: &SessionKey) {
        self.lock().remove(session);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<SessionKey, Session>> {
        // NOTE: a panic while the lock was held leaves bindings that may be
        // incomplete; they are still only routing hints, so they stay usable.
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl fmt::Debug for ResolutionBindings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolutionBindings")
            .field("ttl", &self.ttl)
            .field("sessions", &self.lock().len())
            .finish()
    }
}

/// The key an `ehr_id` is held under: its case-folded form, because two
/// values that differ only in ASCII case are the same identifier.
fn ehr_key(ehr_id: &EhrId) -> String {
    ehr_id.as_str().to_ascii_lowercase()
}
