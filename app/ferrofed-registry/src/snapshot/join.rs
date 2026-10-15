// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The members the operator console holds, joined to the registry a document
//! gives.
//!
//! Each member is owned whole, its organisation, its node and its endpoints
//! together, by the document or by the console, never by both. A member the
//! console holds that shares an identity with the document's is taken by
//! the document and named as shadowed, so it is reported and never dropped
//! without a trace. No specification governs this: our own design.

use std::collections::btree_map::Entry;

use url::Url;

use super::RegistrySnapshot;
use crate::document::Document;
use crate::error::LoadError;
use crate::id::{EndpointId, NodeId};

/// A registry with the console's members joined to the document's.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Joined {
    /// The registry the gateway runs.
    pub snapshot: RegistrySnapshot,
    /// The console's members the document took, by `node_id`, in the order
    /// given: each shares an organisation id, a `node_id`, a `system_id`, an
    /// `endpoint_id` or an endpoint's base URL with the document's.
    pub shadowed: Vec<NodeId>,
}

impl RegistrySnapshot {
    /// Returns this snapshot, the document's, with each of `members` beside
    /// it, every member a document of one organisation, one node and its
    /// endpoints that the operator console holds.
    ///
    /// A member that shares an organisation id, a `node_id`, a `system_id`,
    /// an `endpoint_id` or an endpoint's base URL with the document's is left
    /// out and named in [`Joined::shadowed`]: the document's resource wins.
    ///
    /// # Errors
    ///
    /// The [`LoadError`] of [`RegistrySnapshot::from_document`] for a member
    /// that does not validate on its own, and
    /// [`LoadError::DuplicateOrganisation`] (for two different declarations
    /// of one organisation), [`LoadError::DuplicateNode`],
    /// [`LoadError::DuplicateSystemId`], [`LoadError::DuplicateEndpoint`] or
    /// [`LoadError::DuplicateEndpointUrl`] when two of `members` share one.
    pub fn with_console(
        mut self,
        members: impl IntoIterator<Item = Document>,
    ) -> Result<Joined, LoadError> {
        let document = self.clone();
        let mut shadowed = Vec::new();
        for member in members {
            let member = Self::from_document(member)?;
            if document.shares(&member) {
                shadowed.extend(member.nodes.keys().cloned());
            } else {
                self.absorb(member)?;
            }
        }
        Ok(Joined {
            snapshot: self,
            shadowed,
        })
    }

    /// Every base URL of an endpoint, a native one included, with the
    /// endpoint it addresses.
    fn addressed(&self) -> impl Iterator<Item = (&Url, &EndpointId)> {
        self.endpoints
            .values()
            .map(|endpoint| (&endpoint.url, &endpoint.id))
            .chain(
                self.natives
                    .values()
                    .map(|native| (native.url(), native.id())),
            )
    }

    /// Whether `member` shares an identity with this snapshot.
    fn shares(&self, member: &Self) -> bool {
        member
            .organisations
            .keys()
            .any(|id| self.organisations.contains_key(id))
            || member.nodes.keys().any(|id| self.nodes.contains_key(id))
            || member
                .by_system_id
                .keys()
                .any(|id| self.by_system_id.contains_key(id))
            || member
                .endpoints
                .keys()
                .chain(member.natives.keys())
                .any(|id| self.endpoints.contains_key(id) || self.natives.contains_key(id))
            || member
                .addressed()
                .any(|(url, _)| self.addressed().any(|(held, _)| held == url))
    }

    /// Adds `member`, which shares no identity with the document, as one the
    /// console provides.
    fn absorb(&mut self, member: Self) -> Result<(), LoadError> {
        for (url, second) in member.addressed() {
            if let Some((_, first)) = self.addressed().find(|(held, _)| *held == url) {
                return Err(LoadError::DuplicateEndpointUrl {
                    url: url.to_string(),
                    first: first.clone(),
                    second: second.clone(),
                });
            }
        }
        for (id, organisation) in member.organisations {
            match self.organisations.entry(id) {
                Entry::Occupied(held) if *held.get() == organisation => {}
                Entry::Occupied(held) => {
                    return Err(LoadError::DuplicateOrganisation(held.key().clone()));
                }
                Entry::Vacant(free) => {
                    free.insert(organisation);
                }
            }
        }
        for (system_id, second) in member.by_system_id {
            match self.by_system_id.entry(system_id) {
                Entry::Occupied(held) => {
                    return Err(LoadError::DuplicateSystemId {
                        system_id: held.key().clone(),
                        first: held.get().clone(),
                        second,
                    });
                }
                Entry::Vacant(free) => {
                    free.insert(second);
                }
            }
        }
        for (id, node) in member.nodes {
            match self.nodes.entry(id) {
                Entry::Occupied(held) => return Err(LoadError::DuplicateNode(held.key().clone())),
                Entry::Vacant(free) => {
                    self.console.insert(free.key().clone());
                    free.insert(node);
                }
            }
        }
        for (id, endpoint) in member.endpoints {
            if self.natives.contains_key(&id) || self.endpoints.contains_key(&id) {
                return Err(LoadError::DuplicateEndpoint(id));
            }
            self.endpoints.insert(id, endpoint);
        }
        for (id, native) in member.natives {
            if self.natives.contains_key(&id) || self.endpoints.contains_key(&id) {
                return Err(LoadError::DuplicateEndpoint(id));
            }
            self.natives.insert(id, native);
        }
        Ok(())
    }
}
