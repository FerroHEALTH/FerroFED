// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integrity incidents: a federation integrity defect the registry detects,
//! reported to the federation operator and never resolved by a choice
//! (§12.5.2, §12b.2).
//!
//! An incident is an event. The registry emits it as a structured `tracing`
//! event at `ERROR` with a stable kind and the routing ids involved, and hands
//! it back to its caller; it never carries a body or a patient identifier
//! (no specification governs the event's form: our own design).

use std::fmt;

use crate::id::{EndpointId, NodeId, SystemId};

/// An integrity defect in the follow-up routing table (N21, §12.2).
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
}

impl Incident {
    /// The incident's stable kind, the name a log pipeline or a counter keys
    /// on.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::LearnedCreatingSystemConflict { .. } => "LearnedCreatingSystemConflict",
            Self::RegisteredCreatingSystemConflict { .. } => "RegisteredCreatingSystemConflict",
        }
    }

    /// The `creating_system_id` the incident is about.
    #[must_use]
    pub fn creating_system_id(&self) -> &SystemId {
        match self {
            Self::LearnedCreatingSystemConflict {
                creating_system_id, ..
            }
            | Self::RegisteredCreatingSystemConflict {
                creating_system_id, ..
            } => creating_system_id,
        }
    }

    /// Emits the incident as an `ERROR` event carrying routing ids only.
    pub(crate) fn emit(&self) {
        match self {
            Self::LearnedCreatingSystemConflict {
                creating_system_id,
                first,
                second,
            } => tracing::error!(
                kind = self.kind(),
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
                kind = self.kind(),
                creating_system_id = %creating_system_id,
                node_id = %registered,
                endpoint_id = %learned,
                "integrity incident: a learned mapping contradicts the registry document"
            ),
        }
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
        }
    }
}
