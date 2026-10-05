// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The follow-up routing table: every observed `creating_system_id` to the
//! node that answers for it (N21, §12.2, §15.1).
//!
//! A CDR can hold versions created elsewhere, so the table maps more than the
//! members' own `system_id`s. It answers from three sources, in this order:
//!
//! 1. a member's own `system_id`, which maps to that member;
//! 2. a `[[creating_system]]` mapping in the reviewed bootstrap document;
//! 3. a mapping learned from answers, held in a [`LearnedMap`].
//!
//! A learned mapping never overrides 1 or 2. One that disagrees with them, or
//! with another sighting, raises an [`Incident`] and is not used. A
//! `creating_system_id` none of the three answers is a typed
//! [`CreatingSystemMiss`], never a default endpoint.
//!
//! ```
//! use ferrofed_registry::creating_system::{CreatingSystemRoute, LearnedMap};
//! use ferrofed_registry::id::{EndpointId, NodeId, SystemId};
//! use ferrofed_registry::snapshot::RegistrySnapshot;
//!
//! let registry = RegistrySnapshot::from_toml_str(
//!     r#"
//!     [[organisation]]
//!     id = "org-a"
//!
//!     [[node]]
//!     id = "node-a"
//!     organisation = "org-a"
//!     system_id = "cdr-a.example.org"
//!
//!     [[endpoint]]
//!     id = "node-a-pub"
//!     node = "node-a"
//!     url = "https://cdr-a.example.org/openehr"
//!     connection_type = "openehr-rest-query"
//!     managing_organisation = "org-a"
//!
//!     [[creating_system]]
//!     creating_system_id = "legacy-a.example.org"
//!     endpoint = "node-a-pub"
//!     "#,
//! )?;
//! let learned = LearnedMap::new();
//!
//! let legacy: SystemId = "legacy-a.example.org".parse()?;
//! let route = learned.route(&registry, &legacy)?;
//! assert!(matches!(route, CreatingSystemRoute::Registered { .. }));
//! assert_eq!(route.node(), &"node-a".parse::<NodeId>()?);
//! assert_eq!(route.endpoint(), Some(&"node-a-pub".parse::<EndpointId>()?));
//! assert!(learned.route(&registry, &"elsewhere.example.org".parse()?).is_err());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use openehr_base::prelude::ObjectVersionId;

use crate::error::{CreatingSystemMiss, ObserveError};
use crate::id::{EndpointId, NodeId, SystemId};
use crate::incident::Incident;
use crate::snapshot::RegistrySnapshot;

/// Where a follow-up for a version with this `creating_system_id` goes, and
/// on what ground (§12.2, §12.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatingSystemRoute {
    /// The `creating_system_id` is the member's own `system_id`: the member
    /// created the version, the controlling CDR of §12.4, and any of its
    /// endpoints reaches it.
    Member {
        /// The member.
        node: NodeId,
    },
    /// The registry document maps the `creating_system_id` to this endpoint.
    Registered {
        /// The node the endpoint belongs to.
        node: NodeId,
        /// The mapped endpoint.
        endpoint: EndpointId,
    },
    /// The gateway saw versions of the `creating_system_id` at this endpoint
    /// and nowhere else.
    ///
    /// The endpoint holds the versions; that it created them is not shown,
    /// because an import keeps the original uid (§10.2, §10.3).
    Learned {
        /// The node the endpoint belongs to.
        node: NodeId,
        /// The endpoint the versions were seen at.
        endpoint: EndpointId,
    },
}

impl CreatingSystemRoute {
    /// The node the route reaches.
    #[must_use]
    pub fn node(&self) -> &NodeId {
        match self {
            Self::Member { node } | Self::Registered { node, .. } | Self::Learned { node, .. } => {
                node
            }
        }
    }

    /// The endpoint the route names, or `None` for a member, which any of
    /// its endpoints reaches.
    #[must_use]
    pub fn endpoint(&self) -> Option<&EndpointId> {
        match self {
            Self::Member { .. } => None,
            Self::Registered { endpoint, .. } | Self::Learned { endpoint, .. } => Some(endpoint),
        }
    }
}

/// What one sighting of a version at an endpoint did to the learned map.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Sighting {
    /// The registry document answers for the `creating_system_id`, so nothing
    /// is learned: the endpoint is its creator or holds a copy (§10.2).
    Known(CreatingSystemRoute),
    /// The first sighting of the `creating_system_id`: the map now routes it
    /// to this endpoint.
    Learned(CreatingSystemRoute),
    /// A sighting at the node the map already routes to.
    Confirmed(CreatingSystemRoute),
    /// The sighting contradicts the map or the registry document; the
    /// incident was emitted and the learned mapping is withdrawn.
    Conflict(Incident),
    /// The learned mapping was withdrawn by an earlier incident, which is not
    /// raised again.
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Learned {
    Holder(EndpointId),
    Withdrawn,
}

/// The `creating_system_id` mappings learned from answers, in memory.
///
/// A learned mapping only adds a route the registry document does not give,
/// so a stale one costs a fallback and never a wrong route: a lookup that
/// finds its endpoint gone from the snapshot is a miss. The gateway feeds it
/// the sightings of each answer once the answer is settled. The map holds
/// `system_id`s and `endpoint_id`s only, never a row or a patient identifier (no specification governs where
/// learned state lives: our own design).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LearnedMap {
    entries: BTreeMap<SystemId, Learned>,
}

impl LearnedMap {
    /// Creates an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Routes a `creating_system_id`: a member's own `system_id`, then a
    /// registered mapping, then a learned one (N21, §12.3 step 1).
    ///
    /// A versioned write takes its controlling CDR from the first two alone,
    /// [`RegistrySnapshot::registered_route`]: a learned route shows where
    /// versions are held, never who controls them (§10.3, §12a.1, N23).
    ///
    /// # Errors
    ///
    /// [`CreatingSystemMiss::Conflicted`] when only a withdrawn learned
    /// mapping names it, and [`CreatingSystemMiss::Unknown`] when nothing
    /// does, or the learned endpoint has left the snapshot.
    pub fn route(
        &self,
        snapshot: &RegistrySnapshot,
        creating_system_id: &SystemId,
    ) -> Result<CreatingSystemRoute, CreatingSystemMiss> {
        if let Some(route) = snapshot.registered_route(creating_system_id) {
            return Ok(route);
        }
        match self.entries.get(creating_system_id) {
            Some(Learned::Holder(endpoint)) => learned_route(snapshot, endpoint)
                .ok_or_else(|| CreatingSystemMiss::Unknown(creating_system_id.clone())),
            Some(Learned::Withdrawn) => {
                Err(CreatingSystemMiss::Conflicted(creating_system_id.clone()))
            }
            None => Err(CreatingSystemMiss::Unknown(creating_system_id.clone())),
        }
    }

    /// Every learned mapping, ordered by `creating_system_id`: the endpoint
    /// it routes through, or `None` for one an incident withdrew.
    pub fn entries(&self) -> impl Iterator<Item = (&SystemId, Option<&EndpointId>)> {
        self.entries
            .iter()
            .map(|(creating_system_id, learned)| match learned {
                Learned::Holder(endpoint) => (creating_system_id, Some(endpoint)),
                Learned::Withdrawn => (creating_system_id, None),
            })
    }

    /// Records that `version` was seen in an answer from `endpoint`.
    ///
    /// The first sighting of a `creating_system_id` the registry document
    /// does not answer for learns a route to that endpoint. A sighting at
    /// another node withdraws it and raises
    /// [`Incident::LearnedCreatingSystemConflict`]; a sighting of an id the
    /// document answers for, while a learned mapping names another node,
    /// raises [`Incident::RegisteredCreatingSystemConflict`]. An incident is
    /// emitted once, when the mapping is withdrawn.
    ///
    /// # Errors
    ///
    /// [`ObserveError::UnknownEndpoint`] when `endpoint` is not in the
    /// snapshot, and [`ObserveError::CreatingSystemId`] when the version's
    /// `creating_system_id` is not an openEHR `uid`.
    pub fn observe(
        &mut self,
        snapshot: &RegistrySnapshot,
        version: &ObjectVersionId,
        endpoint: &EndpointId,
    ) -> Result<Sighting, ObserveError> {
        let seen_at = snapshot
            .endpoint(endpoint)
            .ok_or_else(|| ObserveError::UnknownEndpoint(endpoint.clone()))?
            .node()
            .clone();
        let creating_system_id =
            SystemId::creating_system_id_of(version).map_err(ObserveError::CreatingSystemId)?;
        // NOTE: §10.2, an import keeps the original uid, so an id the document
        // routes, seen at another node, is a copy and teaches the map nothing.
        if let Some(route) = snapshot.registered_route(&creating_system_id) {
            return Ok(self
                .settle(snapshot, creating_system_id, &route)
                .map_or(Sighting::Known(route), Sighting::Conflict));
        }
        // NOTE: §12.2 and §10.3, a copy at a node does not prove the node created
        // it, so one sighting learns a read route only, never a write's controlling CDR.
        let sighting = match self.entries.entry(creating_system_id) {
            Entry::Vacant(entry) => {
                entry.insert(Learned::Holder(endpoint.clone()));
                Sighting::Learned(CreatingSystemRoute::Learned {
                    node: seen_at,
                    endpoint: endpoint.clone(),
                })
            }
            Entry::Occupied(mut entry) => match entry.get() {
                Learned::Withdrawn => Sighting::Withdrawn,
                Learned::Holder(first) => match learned_route(snapshot, first) {
                    Some(route) if *route.node() == seen_at => Sighting::Confirmed(route),
                    Some(_) => {
                        let incident = Incident::LearnedCreatingSystemConflict {
                            creating_system_id: entry.key().clone(),
                            first: first.clone(),
                            second: endpoint.clone(),
                        };
                        entry.insert(Learned::Withdrawn);
                        incident.emit();
                        Sighting::Conflict(incident)
                    }
                    None => {
                        entry.insert(Learned::Holder(endpoint.clone()));
                        Sighting::Learned(CreatingSystemRoute::Learned {
                            node: seen_at,
                            endpoint: endpoint.clone(),
                        })
                    }
                },
            },
        };
        Ok(sighting)
    }

    /// Checks every learned mapping against a newly loaded snapshot and
    /// returns the incidents it raises.
    ///
    /// A learned mapping the document now answers for at the same node is
    /// dropped, one it answers for at another node is withdrawn with
    /// [`Incident::RegisteredCreatingSystemConflict`], and one whose endpoint
    /// left the snapshot is dropped.
    pub fn reconcile(&mut self, snapshot: &RegistrySnapshot) -> Vec<Incident> {
        let held: Vec<(SystemId, bool)> = self
            .entries
            .iter()
            .filter_map(|(creating_system_id, learned)| match learned {
                Learned::Holder(endpoint) => Some((
                    creating_system_id.clone(),
                    snapshot.endpoint(endpoint).is_some(),
                )),
                Learned::Withdrawn => None,
            })
            .collect();
        let mut incidents = Vec::new();
        for (creating_system_id, reachable) in held {
            if let Some(route) = snapshot.registered_route(&creating_system_id) {
                incidents.extend(self.settle(snapshot, creating_system_id, &route));
            } else if !reachable {
                self.entries.remove(&creating_system_id);
            }
        }
        incidents
    }

    /// Retires a learned mapping for an id the document routes: dropped when
    /// it agrees, withdrawn with an incident when it names another node.
    fn settle(
        &mut self,
        snapshot: &RegistrySnapshot,
        creating_system_id: SystemId,
        registered: &CreatingSystemRoute,
    ) -> Option<Incident> {
        let Entry::Occupied(mut entry) = self.entries.entry(creating_system_id) else {
            return None;
        };
        let Learned::Holder(learned) = entry.get() else {
            return None;
        };
        let agrees =
            learned_route(snapshot, learned).is_none_or(|route| route.node() == registered.node());
        if agrees {
            entry.remove();
            return None;
        }
        let incident = Incident::RegisteredCreatingSystemConflict {
            creating_system_id: entry.key().clone(),
            registered: registered.node().clone(),
            learned: learned.clone(),
        };
        entry.insert(Learned::Withdrawn);
        incident.emit();
        Some(incident)
    }
}

/// The learned route to `endpoint`, or `None` once it left the snapshot.
fn learned_route(
    snapshot: &RegistrySnapshot,
    endpoint: &EndpointId,
) -> Option<CreatingSystemRoute> {
    snapshot
        .endpoint(endpoint)
        .map(|declared| CreatingSystemRoute::Learned {
            node: declared.node().clone(),
            endpoint: endpoint.clone(),
        })
}
