// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integrity incidents: a federation integrity defect the gateway detects,
//! reported to the federation operator and never resolved by a choice
//! (§12.5.2, §12b.2, N42).
//!
//! An incident is an event. It is emitted once, as a structured `tracing`
//! event at `ERROR` under [`TARGET`] with a stable kind and the routing ids
//! involved, counted per [`Kind`] for the metrics surface, and handed back to
//! its caller; it never carries a body or a patient identifier (no
//! specification governs the event's form: our own design). An `ehr_id` is node-local and names no patient (§5.2), so an
//! incident about one carries it; the event and the `Display` text name it
//! only when it is a bare UUID, because any other `HIER_OBJECT_ID` form could
//! be a patient identifier a client wrote in a path (§5.4.1, N33).
//!
//! ```
//! use ferrofed_registry::incident::{Detection, Incident};
//!
//! let ehr_id = "7d44b88c-4199-4bad-97dc-d78268e01398".parse()?;
//! let claimants = vec!["node-a-pub".parse()?, "node-b-pub".parse()?];
//! let detection = Detection::AskAll;
//! let incident = Incident::EhrIdCollision { ehr_id, detection, claimants };
//! assert_eq!("EhrIdCollision", incident.kind());
//! let shown = "ehr_id 7d44b88c-4199-4bad-97dc-d78268e01398 is claimed by endpoints \
//!     [node-a-pub, node-b-pub], found by the ask-all probe";
//! assert_eq!(shown, incident.to_string());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::id::{EhrId, EndpointId, NodeId, SystemId};

/// The `tracing` target every incident is emitted under, so an operator can
/// route and count incidents apart from the request log.
pub const TARGET: &str = "ferrofed::integrity";

/// A federation integrity defect: in the follow-up routing table (N21,
/// §12.2), or in which member holds an `ehr_id` (§12.5.2, §12b.2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Incident {
    /// A `creating_system_id` no member or registered mapping answers for was
    /// seen at two nodes, so a learned mapping cannot name one.
    LearnedCreatingSystemConflict {
        /// The `creating_system_id`.
        creating_system_id: SystemId,
        /// The endpoint the learned mapping named.
        first: EndpointId,
        /// The endpoint at another node the id was then seen at.
        second: EndpointId,
    },
    /// A learned mapping names another node than the registry document does
    /// for the same `creating_system_id`.
    RegisteredCreatingSystemConflict {
        /// The `creating_system_id`.
        creating_system_id: SystemId,
        /// The node the document routes it to.
        registered: NodeId,
        /// The endpoint the learned mapping named.
        learned: EndpointId,
    },
    /// A request addressed an `ehr_id` more than one member claims, and was
    /// refused `409` listing the claimants (§12.5.2, N42).
    EhrIdCollision {
        /// The `ehr_id` the request addressed.
        ehr_id: EhrId,
        /// The step of §12.5.1 that found the claimants.
        detection: Detection,
        /// The endpoint of each claiming member, in `node_id` order.
        claimants: Vec<EndpointId>,
    },
    /// The `ehr_id` index learned an `ehr_id` it already held for another
    /// member: the index-insert alarm of §12b.2.
    IndexInsertCollision {
        /// The `ehr_id`.
        ehr_id: EhrId,
        /// Every member the index now holds it at, in `node_id` order.
        claimants: Vec<NodeId>,
    },
}

/// The kind of an integrity incident: the closed set a log pipeline or a
/// counter keys on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// [`Incident::LearnedCreatingSystemConflict`].
    LearnedCreatingSystemConflict,
    /// [`Incident::RegisteredCreatingSystemConflict`].
    RegisteredCreatingSystemConflict,
    /// [`Incident::EhrIdCollision`].
    EhrIdCollision,
    /// [`Incident::IndexInsertCollision`].
    IndexInsertCollision,
}

/// How many [`Kind::LearnedCreatingSystemConflict`] incidents were emitted.
static LEARNED_CONFLICTS: AtomicU64 = AtomicU64::new(0);
/// How many [`Kind::RegisteredCreatingSystemConflict`] incidents were emitted.
static REGISTERED_CONFLICTS: AtomicU64 = AtomicU64::new(0);
/// How many [`Kind::EhrIdCollision`] incidents were emitted.
static EHR_ID_COLLISIONS: AtomicU64 = AtomicU64::new(0);
/// How many [`Kind::IndexInsertCollision`] incidents were emitted.
static INDEX_INSERT_COLLISIONS: AtomicU64 = AtomicU64::new(0);

impl Kind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 4] = [
        Self::LearnedCreatingSystemConflict,
        Self::RegisteredCreatingSystemConflict,
        Self::EhrIdCollision,
        Self::IndexInsertCollision,
    ];

    /// The kind's stable name, as the event and a counter label carry it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LearnedCreatingSystemConflict => "LearnedCreatingSystemConflict",
            Self::RegisteredCreatingSystemConflict => "RegisteredCreatingSystemConflict",
            Self::EhrIdCollision => "EhrIdCollision",
            Self::IndexInsertCollision => "IndexInsertCollision",
        }
    }

    /// Returns how many incidents of this kind [`Incident::emit`] has
    /// emitted in this process since it started.
    #[must_use]
    pub fn emitted(self) -> u64 {
        self.counter().load(Ordering::Relaxed)
    }

    /// The process-wide counter of this kind.
    const fn counter(self) -> &'static AtomicU64 {
        match self {
            Self::LearnedCreatingSystemConflict => &LEARNED_CONFLICTS,
            Self::RegisteredCreatingSystemConflict => &REGISTERED_CONFLICTS,
            Self::EhrIdCollision => &EHR_ID_COLLISIONS,
            Self::IndexInsertCollision => &INDEX_INSERT_COLLISIONS,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<&str> for Kind {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Kind> for &str {
    fn eq(&self, other: &Kind) -> bool {
        *self == other.as_str()
    }
}

/// The step of §12.5.1 that found an `ehr_id` at more than one member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Detection {
    /// Step 2: the client session's resolution bindings name several members.
    Binding,
    /// Step 3: the `ehr_id` index holds several members.
    Index,
    /// Step 4: several members answered the ask-all probe holding it.
    AskAll,
}

impl Detection {
    /// The step's name as the event records it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Binding => "binding",
            Self::Index => "index",
            Self::AskAll => "ask-all",
        }
    }
}

impl fmt::Display for Detection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Binding => "the session's resolution bindings",
            Self::Index => "the ehr_id index",
            Self::AskAll => "the ask-all probe",
        })
    }
}

impl Incident {
    /// The incident's stable kind, the name a log pipeline or a counter keys
    /// on.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::LearnedCreatingSystemConflict { .. } => Kind::LearnedCreatingSystemConflict,
            Self::RegisteredCreatingSystemConflict { .. } => Kind::RegisteredCreatingSystemConflict,
            Self::EhrIdCollision { .. } => Kind::EhrIdCollision,
            Self::IndexInsertCollision { .. } => Kind::IndexInsertCollision,
        }
    }

    /// The `creating_system_id` the incident is about, when it is about one.
    #[must_use]
    pub fn creating_system_id(&self) -> Option<&SystemId> {
        match self {
            Self::LearnedCreatingSystemConflict {
                creating_system_id, ..
            }
            | Self::RegisteredCreatingSystemConflict {
                creating_system_id, ..
            } => Some(creating_system_id),
            Self::EhrIdCollision { .. } | Self::IndexInsertCollision { .. } => None,
        }
    }

    /// The `ehr_id` the incident is about, when it is about one.
    #[must_use]
    pub fn ehr_id(&self) -> Option<&EhrId> {
        match self {
            Self::EhrIdCollision { ehr_id, .. } | Self::IndexInsertCollision { ehr_id, .. } => {
                Some(ehr_id)
            }
            Self::LearnedCreatingSystemConflict { .. }
            | Self::RegisteredCreatingSystemConflict { .. } => None,
        }
    }

    /// Emits the incident as an `ERROR` event under [`TARGET`] carrying its
    /// kind and routing ids only, and counts it under its [`Kind`]
    /// ([`Kind::emitted`]).
    ///
    /// The code that detects a defect emits its incident once; a caller that
    /// is handed one back never emits it again.
    pub fn emit(&self) {
        // NOTE: no specification governs this: our own design; an operator
        // alerts on this count, and no webhook is called.
        self.kind().counter().fetch_add(1, Ordering::Relaxed);
        crate::operator::record(self);
        match self {
            Self::LearnedCreatingSystemConflict {
                creating_system_id,
                first,
                second,
            } => tracing::error!(
                target: TARGET,
                kind = self.kind().as_str(),
                creating_system_id = %creating_system_id,
                first_endpoint_id = %first,
                second_endpoint_id = %second,
                "integrity incident: a creating_system_id was seen at two nodes"
            ),
            Self::RegisteredCreatingSystemConflict {
                creating_system_id,
                registered,
                learned,
            } => tracing::error!(
                target: TARGET,
                kind = self.kind().as_str(),
                creating_system_id = %creating_system_id,
                node_id = %registered,
                endpoint_id = %learned,
                "integrity incident: a learned mapping contradicts the registry document"
            ),
            Self::EhrIdCollision {
                ehr_id,
                detection,
                claimants,
            } => tracing::error!(
                target: TARGET,
                kind = self.kind().as_str(),
                ehr_id = uuid_form(ehr_id),
                detection = detection.as_str(),
                claimants = %Listed(claimants),
                "integrity incident: an ehr_id is claimed by more than one member, and the request was refused"
            ),
            Self::IndexInsertCollision { ehr_id, claimants } => tracing::error!(
                target: TARGET,
                kind = self.kind().as_str(),
                ehr_id = uuid_form(ehr_id),
                claimants = %Listed(claimants),
                "integrity incident: the ehr_id index learned an ehr_id it holds at another member"
            ),
        }
    }
}

/// The `ehr_id` as an event or a message may name it: only a bare UUID, which
/// cannot spell a patient identifier (§5.4.1, N33).
fn uuid_form(ehr_id: &EhrId) -> Option<&str> {
    ehr_id.is_uuid().then(|| ehr_id.as_str())
}

/// An `ehr_id` as the `Display` text names it.
struct Shown<'a>(&'a EhrId);

impl fmt::Display for Shown<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match uuid_form(self.0) {
            Some(uuid) => write!(f, "ehr_id {uuid}"),
            None => f.write_str("an ehr_id that is no UUID"),
        }
    }
}

/// Ids as a message lists them: `[a, b]`.
struct Listed<'a, T>(&'a [T]);

impl<T: fmt::Display> fmt::Display for Listed<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (index, id) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{id}")?;
        }
        f.write_str("]")
    }
}

impl fmt::Display for Incident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LearnedCreatingSystemConflict {
                creating_system_id,
                first,
                second,
            } => write!(
                f,
                "creating_system_id {creating_system_id} was learned at endpoint {first} and seen at endpoint {second} of another node"
            ),
            Self::RegisteredCreatingSystemConflict {
                creating_system_id,
                registered,
                learned,
            } => write!(
                f,
                "creating_system_id {creating_system_id} is registered to node {registered} and was learned at endpoint {learned}"
            ),
            Self::EhrIdCollision {
                ehr_id,
                detection,
                claimants,
            } => write!(
                f,
                "{} is claimed by endpoints {}, found by {detection}",
                Shown(ehr_id),
                Listed(claimants)
            ),
            Self::IndexInsertCollision { ehr_id, claimants } => write!(
                f,
                "the ehr_id index holds {} at nodes {}",
                Shown(ehr_id),
                Listed(claimants)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Detection, Incident, Kind};

    #[test]
    fn an_emitted_incident_is_counted_under_its_kind_alone() {
        let incident = Incident::IndexInsertCollision {
            ehr_id: "7d44b88c-4199-4bad-97dc-d78268e01398".parse().unwrap(),
            claimants: vec!["node-a".parse().unwrap(), "node-b".parse().unwrap()],
        };
        let before = Kind::ALL.map(Kind::emitted);
        incident.emit();
        let after = Kind::ALL.map(Kind::emitted);
        for ((kind, was), now) in Kind::ALL.into_iter().zip(before).zip(after) {
            let expected = if kind == Kind::IndexInsertCollision {
                was + 1
            } else {
                was
            };
            assert_eq!(expected, now, "{kind}");
        }
        assert_eq!("IndexInsertCollision", incident.kind());
    }

    #[test]
    fn an_ehr_id_that_is_no_uuid_is_never_shown() {
        let incident = Incident::IndexInsertCollision {
            ehr_id: "2.999.1.12345".parse().unwrap(),
            claimants: vec!["node-a".parse().unwrap(), "node-b".parse().unwrap()],
        };
        assert_eq!(
            "the ehr_id index holds an ehr_id that is no UUID at nodes [node-a, node-b]",
            incident.to_string()
        );
        let collision = Incident::EhrIdCollision {
            ehr_id: "2.999.1.12345".parse().unwrap(),
            detection: Detection::Binding,
            claimants: vec!["node-a-pub".parse().unwrap()],
        };
        let shown = collision.to_string();
        assert!(!shown.contains("12345"), "{shown}");
    }
}
