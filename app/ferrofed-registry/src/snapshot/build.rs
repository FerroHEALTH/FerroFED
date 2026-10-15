// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checks a document's organisations, nodes, endpoints and
//! `creating_system_id` mappings pass on their way into a snapshot.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use url::Url;

use super::{ConnectionType, Endpoint, Node, NodeIdentifier, Organisation};
use crate::document::{CreatingSystemDoc, EndpointDoc, NodeDoc};
use crate::error::{LoadError, Referrer, UrlFault};
use crate::id::{EndpointId, NodeId, OrganisationId, SystemId};
use crate::native::{self, NativeEndpoint};

pub(super) fn organisations(
    declared: impl Iterator<Item = Organisation>,
) -> Result<BTreeMap<OrganisationId, Organisation>, LoadError> {
    let mut organisations = BTreeMap::new();
    for organisation in declared {
        match organisations.entry(organisation.id.clone()) {
            Entry::Occupied(entry) => {
                return Err(LoadError::DuplicateOrganisation(entry.key().clone()));
            }
            Entry::Vacant(entry) => {
                entry.insert(organisation);
            }
        }
    }
    Ok(organisations)
}

pub(super) type NodeMaps = (BTreeMap<NodeId, Node>, BTreeMap<SystemId, NodeId>);

pub(super) fn nodes(
    declared: Vec<NodeDoc>,
    organisations: &BTreeMap<OrganisationId, Organisation>,
) -> Result<NodeMaps, LoadError> {
    let mut nodes = BTreeMap::new();
    let mut by_system_id: BTreeMap<SystemId, NodeId> = BTreeMap::new();
    let mut identifier_owner: BTreeMap<NodeIdentifier, NodeId> = BTreeMap::new();
    for doc in declared {
        if !organisations.contains_key(&doc.organisation) {
            return Err(LoadError::UnknownOrganisation {
                referrer: Referrer::Node(doc.id),
                organisation: doc.organisation,
            });
        }
        if nodes.contains_key(&doc.id) {
            return Err(LoadError::DuplicateNode(doc.id));
        }
        match by_system_id.entry(doc.system_id.clone()) {
            Entry::Occupied(entry) => {
                return Err(LoadError::DuplicateSystemId {
                    system_id: doc.system_id,
                    first: entry.get().clone(),
                    second: doc.id,
                });
            }
            Entry::Vacant(entry) => {
                entry.insert(doc.id.clone());
            }
        }
        let mut identifiers = Vec::with_capacity(doc.identifiers.len());
        for raw in doc.identifiers {
            if raw.system.is_empty() || raw.value.is_empty() {
                return Err(LoadError::EmptyNodeIdentifier(doc.id));
            }
            let identifier = NodeIdentifier {
                system: raw.system,
                value: raw.value,
            };
            match identifier_owner.entry(identifier.clone()) {
                Entry::Occupied(entry) => {
                    return Err(LoadError::DuplicateNodeIdentifier {
                        system: identifier.system,
                        value: identifier.value,
                        first: entry.get().clone(),
                        second: doc.id,
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(doc.id.clone());
                }
            }
            identifiers.push(identifier);
        }
        for (member, value) in [("product", &doc.product), ("version", &doc.version)] {
            if value.as_deref().is_some_and(str::is_empty) {
                return Err(LoadError::EmptyNodeDescription {
                    node: doc.id,
                    member,
                });
            }
        }
        let node = Node {
            id: doc.id.clone(),
            organisation: doc.organisation,
            system_id: doc.system_id,
            product: doc.product,
            version: doc.version,
            identifiers,
        };
        nodes.insert(doc.id, node);
    }
    Ok((nodes, by_system_id))
}

/// The Query API endpoints and the native endpoints a document declares.
pub(super) type EndpointMaps = (
    BTreeMap<EndpointId, Endpoint>,
    BTreeMap<EndpointId, NativeEndpoint>,
);

pub(super) fn endpoints(
    declared: Vec<EndpointDoc>,
    nodes: &BTreeMap<NodeId, Node>,
    organisations: &BTreeMap<OrganisationId, Organisation>,
) -> Result<EndpointMaps, LoadError> {
    let mut endpoints = BTreeMap::new();
    let mut natives = BTreeMap::new();
    let mut url_owner: BTreeMap<Url, EndpointId> = BTreeMap::new();
    for doc in declared {
        if endpoints.contains_key(&doc.id) || natives.contains_key(&doc.id) {
            return Err(LoadError::DuplicateEndpoint(doc.id));
        }
        if !nodes.contains_key(&doc.node) {
            return Err(LoadError::UnknownNode {
                endpoint: doc.id,
                node: doc.node,
            });
        }
        if !organisations.contains_key(&doc.managing_organisation) {
            return Err(LoadError::UnknownOrganisation {
                referrer: Referrer::Endpoint(doc.id),
                organisation: doc.managing_organisation,
            });
        }
        let url = match base_url(doc.url.expose()) {
            Ok(url) => url,
            Err(fault) => {
                return Err(LoadError::EndpointUrl {
                    endpoint: doc.id,
                    fault,
                });
            }
        };
        match url_owner.entry(url.clone()) {
            Entry::Occupied(entry) => {
                return Err(LoadError::DuplicateEndpointUrl {
                    url: crate::error::shown_url(&url),
                    first: entry.get().clone(),
                    second: doc.id,
                });
            }
            Entry::Vacant(entry) => {
                entry.insert(doc.id.clone());
            }
        }
        if doc.connection_type == ConnectionType::Hl7FhirRest {
            let native = native::declared(doc, url)?;
            natives.insert(native.id().clone(), native);
            continue;
        }
        native::held_to_query(&doc)?;
        if doc.consent_refusal_codes.iter().any(String::is_empty) {
            return Err(LoadError::EmptyConsentRefusalCode(doc.id));
        }
        let endpoint = Endpoint {
            id: doc.id.clone(),
            node: doc.node,
            url,
            connection_type: doc.connection_type,
            managing_organisation: doc.managing_organisation,
            status: doc.status,
            consent_refusal_codes: doc.consent_refusal_codes.into_iter().collect(),
        };
        endpoints.insert(doc.id, endpoint);
    }
    native::one_per_category(natives.values())?;
    Ok((endpoints, natives))
}

pub(super) fn creating_systems(
    declared: Vec<CreatingSystemDoc>,
    endpoints: &BTreeMap<EndpointId, Endpoint>,
    by_system_id: &BTreeMap<SystemId, NodeId>,
) -> Result<BTreeMap<SystemId, EndpointId>, LoadError> {
    let mut creating_systems: BTreeMap<SystemId, EndpointId> = BTreeMap::new();
    for doc in declared {
        if let Some(node) = by_system_id.get(&doc.creating_system_id) {
            return Err(LoadError::CreatingSystemIdOfNode {
                creating_system_id: doc.creating_system_id,
                node: node.clone(),
            });
        }
        if !endpoints.contains_key(&doc.endpoint) {
            return Err(LoadError::UnknownCreatingSystemEndpoint {
                creating_system_id: doc.creating_system_id,
                endpoint: doc.endpoint,
            });
        }
        match creating_systems.entry(doc.creating_system_id) {
            Entry::Occupied(entry) => {
                return Err(LoadError::DuplicateCreatingSystemId {
                    creating_system_id: entry.key().clone(),
                    first: entry.get().clone(),
                    second: doc.endpoint,
                });
            }
            Entry::Vacant(entry) => {
                entry.insert(doc.endpoint);
            }
        }
    }
    Ok(creating_systems)
}

// NOTE: §2.2 assumes transport security from the deployment's security profiles,
// so both `https` and `http` (a test harness behind TLS termination) are accepted.
fn base_url(raw: &str) -> Result<Url, UrlFault> {
    let url = Url::parse(raw).map_err(UrlFault::Parse)?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err(UrlFault::Scheme(url.scheme().to_owned()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(UrlFault::Credentials);
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(UrlFault::NotABase);
    }
    Ok(url)
}
