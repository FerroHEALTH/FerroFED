// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution bindings of §12.5.1 step 2: the `{node, ehr_id}` set a
//! client session's resolution produced, so a follow-up on a path `ehr_id`
//! can be routed to the node that holds it.
//!
//! The bindings belong to the client session (§12.5.1 step 2; their storage
//! is our own design, since no specification governs it): held in memory,
//! keyed by the session and by the `ehr_id`, never by the patient identifier,
//! and dropped when the session's time-to-live passes. Nothing is written to disk and
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
    /// Several members resolved the same `ehr_id`, in `node_id` order: they
    /// are claimants of one `ehr_id`, a collision no later step may settle by
    /// picking one of them (§12.5.2, N42).
    Several(Vec<NodeId>),
}

/// What an identity change at the identity source touches, as the PMIR hook
/// reports it (ITI-93 merge or split, track 8 of §16.3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdentityChange {
    /// The change touches the patients these `ehr_id`s belong to: a binding
    /// that names one of them is dropped, in every session.
    Ehrs(Vec<EhrId>),
    /// The change is known, but not which `ehr_id`s it touches: every binding
    /// of every session is dropped.
    Unscoped,
}

/// One session's bindings and when they expire.
struct Session {
    expires: Instant,
    // NOTE: BASE master05 §"Composite Identifiers and Case": `EhrId` orders by the
    // openehr-base case-folded key, so an `ehr_id` in another case finds its binding.
    by_ehr: BTreeMap<EhrId, BTreeSet<NodeId>>,
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
                .entry(ehr_id.clone())
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
        match held.by_ehr.get(ehr_id) {
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
    /// denial voids what it resolved (§12.5.1 step 2).
    pub fn forget(&self, session: &SessionKey) {
        self.lock().remove(session);
    }

    /// The PMIR hook: drops every binding an identity change at the identity
    /// source could have made stale, in every session, and returns how many
    /// `ehr_id` bindings it dropped.
    ///
    /// Track 8 (§16.3) is provisional and full propagation MAY be deferred
    /// (§18); what FerroFED owes is that no binding outlives a change it could
    /// have learned of. A PMIR subscription (ITI-94) that receives a merge or
    /// split notification (ITI-93) calls this; the time-to-live bounds every
    /// binding the subscription never hears about. A dropped binding costs one
    /// re-resolution, never a misrouted follow-up.
    pub fn identity_changed(&self, change: &IdentityChange) -> usize {
        let mut sessions = self.lock();
        let dropped = match change {
            IdentityChange::Unscoped => {
                let dropped = sessions.values().map(|held| held.by_ehr.len()).sum();
                sessions.clear();
                dropped
            }
            IdentityChange::Ehrs(ehr_ids) => {
                let keys: BTreeSet<&EhrId> = ehr_ids.iter().collect();
                let mut dropped = 0_usize;
                for held in sessions.values_mut() {
                    let before = held.by_ehr.len();
                    held.by_ehr.retain(|ehr, _| !keys.contains(ehr));
                    dropped = dropped.saturating_add(before.saturating_sub(held.by_ehr.len()));
                }
                sessions.retain(|_, held| !held.by_ehr.is_empty());
                dropped
            }
        };
        drop(sessions);
        dropped
    }

    /// Drops every binding that names a member of `departed`, in every
    /// session, and returns how many `ehr_id` bindings it dropped.
    ///
    /// A registry reload calls it with the members that left the federation.
    /// A binding naming a departed member among others is dropped whole, so
    /// a collision is never narrowed to its remaining claimant (§12.5.2,
    /// N42); the cost is one re-resolution.
    pub fn forget_members(&self, departed: &BTreeSet<NodeId>) -> usize {
        if departed.is_empty() {
            return 0;
        }
        let mut sessions = self.lock();
        let mut dropped = 0_usize;
        for held in sessions.values_mut() {
            let before = held.by_ehr.len();
            held.by_ehr.retain(|_, nodes| nodes.is_disjoint(departed));
            dropped = dropped.saturating_add(before.saturating_sub(held.by_ehr.len()));
        }
        sessions.retain(|_, held| !held.by_ehr.is_empty());
        drop(sessions);
        dropped
    }

    /// Drops `session`'s binding of `ehr_id` when it names a member `present`
    /// says the registry no longer holds, and returns whether it did.
    ///
    /// A lookup that finds such a binding calls it: the binding is stale
    /// whole, so it is never narrowed to the members that remain (§12.5.2,
    /// N42). A binding naming only present members is kept.
    pub fn forget_absent(
        &self,
        session: &SessionKey,
        ehr_id: &EhrId,
        present: impl Fn(&NodeId) -> bool,
    ) -> bool {
        let mut sessions = self.lock();
        let Some(held) = sessions.get_mut(session) else {
            return false;
        };
        let stale = held
            .by_ehr
            .get(ehr_id)
            .is_some_and(|nodes| !nodes.iter().all(&present));
        if stale {
            held.by_ehr.remove(ehr_id);
            if held.by_ehr.is_empty() {
                sessions.remove(session);
            }
        }
        drop(sessions);
        stale
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
