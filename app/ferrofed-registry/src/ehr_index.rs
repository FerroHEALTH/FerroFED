// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ehr_id` to node index of §12.5.1 step 3: which member holds an
//! `ehr_id`, learned from what the gateway observed.
//!
//! An `ehr_id` carries no system component, so nothing in it names its node
//! (§12.5). The index learns an owner from a resolution that produced the
//! `ehr_id` at a member, and from a member's successful answer to a request
//! under that `ehr_id`. It holds `ehr_id`s and `node_id`s only, never a patient
//! identifier, in memory and bounded: past its capacity the least recently
//! used `ehr_id` is forgotten, which costs a later request one fallback step,
//! never a wrong route (no specification governs the index's storage: our own
//! design).
//!
//! An `ehr_id` learned at a second member is the index-insert alarm of
//! §12b.2: the index raises an [`Incident::IndexInsertCollision`] once, keeps
//! every claimant, and from then on names none of them (§12.5.2, N42). A held
//! collision has no expiry of its own. Only the operator can remedy it, at a
//! node (§12b.2), and nothing the index observes shows that remedy, so it
//! lasts until the entry is forgotten as least recently used or the process
//! restarts; a later read then asks every member again, and a collision that
//! still stands is found again (no specification governs this: our own
//! design).
//!
//! ```
//! use std::num::NonZeroUsize;
//!
//! use ferrofed_registry::ehr_index::{EhrIndex, Indexed};
//! use ferrofed_registry::id::{EhrId, NodeId};
//!
//! let index = EhrIndex::new(NonZeroUsize::MIN);
//! let ehr_id: EhrId = "7d44b88c-4199-4bad-97dc-d78268e01398".parse()?;
//! assert_eq!(Indexed::None, index.lookup(&ehr_id));
//! index.learn(&ehr_id, &"node-a".parse()?);
//! assert_eq!(Indexed::One("node-a".parse::<NodeId>()?), index.lookup(&ehr_id));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::id::{EhrId, NodeId};
use crate::incident::Incident;

/// What the index says about one `ehr_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Indexed {
    /// The index holds no owner for the `ehr_id`.
    None,
    /// Exactly one member was seen holding it (§12.5.1 step 3).
    One(NodeId),
    /// Several members were seen holding it, in `node_id` order: step 3 gives
    /// no unambiguous answer, so routing moves on (N41, N42).
    Several(Vec<NodeId>),
}

/// What learning one owner did to the index.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Learning {
    /// The index held no owner for the `ehr_id`, and now holds this one.
    New,
    /// The index already held this member, and only this member.
    Confirmed,
    /// The index held the `ehr_id` at other members only, so it now holds
    /// this one too and answers [`Indexed::Several`]: the index-insert alarm
    /// of §12b.2, raised and emitted as the incident carried here.
    Collided(Incident),
    /// The index already held this member among several, a collision whose
    /// alarm was raised when it began.
    Contested,
}

/// The owners of one `ehr_id`, and when the index last used them.
struct Entry {
    owners: BTreeSet<NodeId>,
    used: u64,
}

/// The entries and their recency order.
struct Held {
    // NOTE: BASE master05 §"Composite Identifiers and Case": `EhrId` orders by
    // its case-folded key, so an `ehr_id` written in another case finds its entry.
    entries: BTreeMap<EhrId, Entry>,
    recency: BTreeMap<u64, EhrId>,
    clock: u64,
}

impl Held {
    /// The next recency stamp.
    fn tick(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }

    /// Marks `ehr_id`'s entry as just used.
    fn touch(&mut self, ehr_id: &EhrId) {
        let now = self.tick();
        if let Some(entry) = self.entries.get_mut(ehr_id) {
            self.recency.remove(&entry.used);
            entry.used = now;
            self.recency.insert(now, ehr_id.clone());
        }
    }
}

/// The `ehr_id` to node index, shared by every request of the process.
///
/// `Debug` reports the capacity and how many `ehr_id`s are held, never one of
/// them.
pub struct EhrIndex {
    capacity: NonZeroUsize,
    held: Mutex<Held>,
}

impl EhrIndex {
    /// An empty index that holds at most `capacity` `ehr_id`s.
    #[must_use]
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            held: Mutex::new(Held {
                entries: BTreeMap::new(),
                recency: BTreeMap::new(),
                clock: 0,
            }),
        }
    }

    /// What the index holds for `ehr_id`; a hit counts as a use.
    #[must_use]
    pub fn lookup(&self, ehr_id: &EhrId) -> Indexed {
        let mut held = self.lock();
        let owners: Vec<NodeId> = match held.entries.get(ehr_id) {
            None => return Indexed::None,
            Some(entry) => entry.owners.iter().cloned().collect(),
        };
        held.touch(ehr_id);
        drop(held);
        let mut owners = owners.into_iter();
        match (owners.next(), owners.next()) {
            (None, _) => Indexed::None,
            (Some(only), None) => Indexed::One(only),
            (Some(first), Some(second)) => {
                Indexed::Several([first, second].into_iter().chain(owners).collect())
            }
        }
    }

    /// Records that `node` holds `ehr_id`, forgetting the least recently used
    /// `ehr_id` when the index is full.
    ///
    /// A member is only ever added: an `ehr_id` seen at a member it is not yet
    /// held at, while held at another, keeps every claimant, so the index never
    /// names one of them (§12.5.2, N42), and raises the index-insert alarm of
    /// §12b.2. The alarm's [`Incident::IndexInsertCollision`] is emitted once,
    /// here, for each member that joins the claimants.
    pub fn learn(&self, ehr_id: &EhrId, node: &NodeId) -> Learning {
        let mut held = self.lock();
        let learning = match held.entries.get_mut(ehr_id) {
            Some(entry) if entry.owners.contains(node) => {
                if entry.owners.len() == 1 {
                    Learning::Confirmed
                } else {
                    Learning::Contested
                }
            }
            Some(entry) => {
                entry.owners.insert(node.clone());
                Learning::Collided(Incident::IndexInsertCollision {
                    ehr_id: ehr_id.clone(),
                    claimants: entry.owners.iter().cloned().collect(),
                })
            }
            None => {
                if held.entries.len() >= self.capacity.get()
                    && let Some((_, oldest)) = held.recency.pop_first()
                {
                    held.entries.remove(&oldest);
                }
                let used = held.tick();
                held.entries.insert(
                    ehr_id.clone(),
                    Entry {
                        owners: BTreeSet::from([node.clone()]),
                        used,
                    },
                );
                held.recency.insert(used, ehr_id.clone());
                Learning::New
            }
        };
        if !matches!(learning, Learning::New) {
            held.touch(ehr_id);
        }
        drop(held);
        if let Learning::Collided(incident) = &learning {
            incident.emit();
        }
        learning
    }

    /// Forgets every `ehr_id` held at a member of `departed`, and returns how
    /// many it forgot.
    ///
    /// A registry reload calls it with the members that left the
    /// federation. An entry naming a departed member is forgotten whole,
    /// with the members it also names: a collision is never narrowed to its
    /// remaining claimant (§12.5.2, N42), so a later read asks every member
    /// again and finds a collision that still stands.
    pub fn forget_members(&self, departed: &BTreeSet<NodeId>) -> usize {
        if departed.is_empty() {
            return 0;
        }
        let mut held = self.lock();
        let gone: Vec<(EhrId, u64)> = held
            .entries
            .iter()
            .filter(|(_, entry)| !entry.owners.is_disjoint(departed))
            .map(|(ehr_id, entry)| (ehr_id.clone(), entry.used))
            .collect();
        for (ehr_id, used) in &gone {
            held.entries.remove(ehr_id);
            held.recency.remove(used);
        }
        drop(held);
        gone.len()
    }

    /// Forgets `ehr_id` when its entry names a member `present` says the
    /// registry no longer holds, and returns whether it did.
    ///
    /// A lookup that finds such an entry calls it: the entry is stale whole,
    /// so it is never narrowed to the claimants that remain (§12.5.2, N42).
    /// An entry naming only present members is kept, whatever it names.
    pub fn forget_absent(&self, ehr_id: &EhrId, present: impl Fn(&NodeId) -> bool) -> bool {
        let mut held = self.lock();
        let used = match held.entries.get(ehr_id) {
            Some(entry) if !entry.owners.iter().all(&present) => entry.used,
            Some(_) | None => return false,
        };
        held.entries.remove(ehr_id);
        held.recency.remove(&used);
        true
    }

    /// How many `ehr_id`s the index holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// Whether the index holds no `ehr_id`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> MutexGuard<'_, Held> {
        // NOTE: a panic while the lock was held leaves entries that may be
        // incomplete; each is still only a routing hint, so they stay usable.
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for EhrIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EhrIndex")
            .field("capacity", &self.capacity)
            .field("held", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::{EhrIndex, Indexed, Learning};
    use crate::id::{EhrId, NodeId};
    use crate::incident::Incident;

    const EHR_1: &str = "11111111-1111-4111-8111-111111111111";
    const EHR_2: &str = "22222222-2222-4222-8222-222222222222";
    const EHR_3: &str = "33333333-3333-4333-8333-333333333333";

    fn ehr(value: &str) -> EhrId {
        value.parse().unwrap()
    }

    fn node(value: &str) -> NodeId {
        value.parse().unwrap()
    }

    #[test]
    fn learning_an_owner_twice_confirms_it_and_another_owner_keeps_both() {
        let index = EhrIndex::new(NonZeroUsize::new(4).unwrap());
        assert_eq!(Learning::New, index.learn(&ehr(EHR_1), &node("node-a")));
        assert_eq!(
            Learning::Confirmed,
            index.learn(&ehr(EHR_1), &node("node-a"))
        );
        assert_eq!(
            Learning::Collided(Incident::IndexInsertCollision {
                ehr_id: ehr(EHR_1),
                claimants: vec![node("node-a"), node("node-b")],
            }),
            index.learn(&ehr(EHR_1), &node("node-b")),
            "the index-insert alarm (§12b.2)"
        );
        assert_eq!(
            Indexed::Several(vec![node("node-a"), node("node-b")]),
            index.lookup(&ehr(EHR_1)),
            "two claimants are never narrowed to one (N42)"
        );
        assert_eq!(
            Learning::Contested,
            index.learn(&ehr(EHR_1), &node("node-a")),
            "a held collision stays one, and its alarm is not raised again"
        );
        assert_eq!(
            Indexed::Several(vec![node("node-a"), node("node-b")]),
            index.lookup(&ehr(EHR_1)),
            "a held collision stays one"
        );
    }

    #[test]
    fn a_third_claimant_raises_the_alarm_again_naming_all_three() {
        let index = EhrIndex::new(NonZeroUsize::MIN);
        index.learn(&ehr(EHR_1), &node("node-a"));
        index.learn(&ehr(EHR_1), &node("node-b"));
        assert_eq!(
            Learning::Collided(Incident::IndexInsertCollision {
                ehr_id: ehr(EHR_1),
                claimants: vec![node("node-a"), node("node-b"), node("node-c")],
            }),
            index.learn(&ehr(EHR_1), &node("node-c"))
        );
    }

    #[test]
    fn an_ehr_id_in_another_case_finds_its_entry() {
        let index = EhrIndex::new(NonZeroUsize::MIN);
        index.learn(&ehr(EHR_1), &node("node-a"));
        assert_eq!(
            Indexed::One(node("node-a")),
            index.lookup(&ehr(&EHR_1.to_ascii_uppercase()))
        );
    }

    #[test]
    fn a_full_index_forgets_the_least_recently_used_ehr_id() {
        let index = EhrIndex::new(NonZeroUsize::new(2).unwrap());
        index.learn(&ehr(EHR_1), &node("node-a"));
        index.learn(&ehr(EHR_2), &node("node-b"));
        assert_eq!(Indexed::One(node("node-a")), index.lookup(&ehr(EHR_1)));
        index.learn(&ehr(EHR_3), &node("node-a"));
        assert_eq!(2, index.len());
        assert_eq!(
            Indexed::None,
            index.lookup(&ehr(EHR_2)),
            "the entry used longest ago went"
        );
        assert_eq!(Indexed::One(node("node-a")), index.lookup(&ehr(EHR_1)));
        assert_eq!(Indexed::One(node("node-a")), index.lookup(&ehr(EHR_3)));
    }

    #[test]
    fn a_departed_member_takes_its_entries_and_every_collision_it_claims() {
        let index = EhrIndex::new(NonZeroUsize::new(3).unwrap());
        index.learn(&ehr(EHR_1), &node("node-a"));
        index.learn(&ehr(EHR_2), &node("node-b"));
        index.learn(&ehr(EHR_3), &node("node-b"));
        index.learn(&ehr(EHR_3), &node("node-a"));
        let departed = std::collections::BTreeSet::from([node("node-b")]);
        assert_eq!(2, index.forget_members(&departed));
        assert_eq!(Indexed::One(node("node-a")), index.lookup(&ehr(EHR_1)));
        assert_eq!(Indexed::None, index.lookup(&ehr(EHR_2)));
        assert_eq!(
            Indexed::None,
            index.lookup(&ehr(EHR_3)),
            "a collision is forgotten whole, never narrowed to node-a (N42)"
        );
        assert_eq!(0, index.forget_members(&departed), "nothing is left");
    }

    #[test]
    fn a_forgotten_entry_leaves_the_recency_order_whole() {
        let index = EhrIndex::new(NonZeroUsize::new(2).unwrap());
        index.learn(&ehr(EHR_1), &node("node-b"));
        index.learn(&ehr(EHR_2), &node("node-a"));
        index.forget_members(&std::collections::BTreeSet::from([node("node-b")]));
        index.learn(&ehr(EHR_3), &node("node-a"));
        assert_eq!(2, index.len(), "the freed place is used, nothing evicted");
        assert_eq!(Indexed::One(node("node-a")), index.lookup(&ehr(EHR_2)));
        assert_eq!(Indexed::One(node("node-a")), index.lookup(&ehr(EHR_3)));
    }

    #[test]
    fn an_entry_naming_an_absent_member_is_forgotten_and_one_naming_present_members_kept() {
        let index = EhrIndex::new(NonZeroUsize::new(2).unwrap());
        index.learn(&ehr(EHR_1), &node("node-a"));
        index.learn(&ehr(EHR_1), &node("node-b"));
        index.learn(&ehr(EHR_2), &node("node-a"));
        let present = |member: &NodeId| *member == node("node-a");
        assert!(index.forget_absent(&ehr(EHR_1), present));
        assert!(!index.forget_absent(&ehr(EHR_2), present));
        assert!(!index.forget_absent(&ehr(EHR_3), present));
        assert_eq!(Indexed::None, index.lookup(&ehr(EHR_1)));
        assert_eq!(Indexed::One(node("node-a")), index.lookup(&ehr(EHR_2)));
        index.learn(&ehr(EHR_3), &node("node-a"));
        assert_eq!(2, index.len(), "the forgotten place is reused");
    }

    #[test]
    fn debug_counts_and_names_no_ehr_id() {
        let index = EhrIndex::new(NonZeroUsize::MIN);
        index.learn(&ehr(EHR_1), &node("node-a"));
        let shown = format!("{index:?}");
        assert!(shown.contains("held: 1"), "{shown}");
        assert!(!shown.contains(EHR_1), "{shown}");
    }
}
