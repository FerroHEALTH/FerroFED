// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry snapshot: the federation's members as the operator admitted
//! them (§12b.1), validated once and read unchanged by every query that took
//! it (docs/architecture.md section 8).

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::Path;

use serde::Deserialize;
use url::Url;

use crate::document::{Document, EndpointDoc, NodeDoc};
use crate::error::{LoadError, Referrer, UrlFault};
use crate::id::{EndpointId, NodeId, OrganisationId, SystemId};

/// The connection type of an endpoint (N19, §15.2).
///
/// N19 requires a defined openEHR Query API code, and §15.2 forbids relying
/// on `hl7-fhir-rest` for an openEHR endpoint, so the one accepted code is
/// `openehr-rest-query`, which FerroFED defines in a code system of its own
/// (docs/architecture.md section 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub enum ConnectionType {
    /// The openEHR ITS-REST Query API (`openehr-rest-query`).
    #[serde(rename = "openehr-rest-query")]
    OpenehrRestQuery,
}

impl ConnectionType {
    /// The code as the document and the directory write it.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::OpenehrRestQuery => "openehr-rest-query",
        }
    }
}

/// Whether an endpoint is contacted.
///
/// A suspended endpoint is reported `excluded` with an operator-policy
/// reason and never contacted (§11.1); the rule for a suspended member is
/// FerroFED's own, because §12b.3 leaves revocation open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointStatus {
    /// The endpoint is in service; the default when the document is silent.
    #[default]
    Active,
    /// The operator has taken the endpoint out of service.
    Suspended,
}

/// An organisation: one that operates a node, or the managing organisation of
/// an endpoint (N20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organisation {
    id: OrganisationId,
    name: Option<String>,
}

impl Organisation {
    /// The organisation's id.
    #[must_use]
    pub fn id(&self) -> &OrganisationId {
        &self.id
    }

    /// The organisation's display name, when the document gives one.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// An identifier by which a service outside the gateway (a localizer, a
/// directory) names a node, as `system|value`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeIdentifier {
    system: String,
    value: String,
}

impl NodeIdentifier {
    /// The identifier system (a URI or an OID).
    #[must_use]
    pub fn system(&self) -> &str {
        &self.system
    }

    /// The identifier value within its system.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A member of the federation: one organisation's CDR deployment, with its
/// openEHR `system_id` and its endpoints (§12b.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    id: NodeId,
    organisation: OrganisationId,
    system_id: SystemId,
    identifiers: Vec<NodeIdentifier>,
}

impl Node {
    /// The node's `node_id`.
    #[must_use]
    pub fn id(&self) -> &NodeId {
        &self.id
    }

    /// The organisation that operates the node.
    #[must_use]
    pub fn organisation(&self) -> &OrganisationId {
        &self.organisation
    }

    /// The node's openEHR `system_id`, unique across the federation (§12b.2).
    #[must_use]
    pub fn system_id(&self) -> &SystemId {
        &self.system_id
    }

    /// The identifiers outside services name the node by, in document order.
    #[must_use]
    pub fn identifiers(&self) -> &[NodeIdentifier] {
        &self.identifiers
    }
}

/// A reachable interface of a node: one base URL with one connection type
/// and exactly one managing organisation (N19, N20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    id: EndpointId,
    node: NodeId,
    url: Url,
    connection_type: ConnectionType,
    managing_organisation: OrganisationId,
    status: EndpointStatus,
}

impl Endpoint {
    /// The endpoint's stable `endpoint_id` (N19).
    #[must_use]
    pub fn id(&self) -> &EndpointId {
        &self.id
    }

    /// The node the endpoint belongs to.
    #[must_use]
    pub fn node(&self) -> &NodeId {
        &self.node
    }

    /// The endpoint's ITS-REST base URL.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// The endpoint's connection type.
    #[must_use]
    pub fn connection_type(&self) -> ConnectionType {
        self.connection_type
    }

    /// The one organisation that manages the endpoint (N20).
    #[must_use]
    pub fn managing_organisation(&self) -> &OrganisationId {
        &self.managing_organisation
    }

    /// Whether the endpoint is in service.
    #[must_use]
    pub fn status(&self) -> EndpointStatus {
        self.status
    }
}

/// The federation's membership as one immutable value.
///
/// Built from the reviewed bootstrap document, with every reference checked,
/// so a lookup either answers or the member does not exist. A query takes the
/// snapshot once and uses it to its end; a reload builds a new one. The
/// registry has no write API: admission is a reviewed change to the document
/// (§12b.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrySnapshot {
    organisations: BTreeMap<OrganisationId, Organisation>,
    nodes: BTreeMap<NodeId, Node>,
    endpoints: BTreeMap<EndpointId, Endpoint>,
    by_system_id: BTreeMap<SystemId, NodeId>,
}

impl RegistrySnapshot {
    /// Reads and validates the bootstrap document at `path`.
    ///
    /// # Errors
    ///
    /// [`LoadError::Read`] when the file cannot be read, and every error of
    /// [`RegistrySnapshot::from_toml_str`].
    pub fn read(path: &Path) -> Result<Self, LoadError> {
        let text = std::fs::read_to_string(path).map_err(|source| LoadError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml_str(&text)
    }

    /// Parses and validates a bootstrap document.
    ///
    /// # Errors
    ///
    /// [`LoadError::Parse`] when the text is not a document of the registry's
    /// shape, and the other [`LoadError`] variants when it is but its content
    /// breaks a membership rule: a duplicate id or `system_id`, a dangling
    /// reference, an unusable base URL, a node with no endpoint, or no node
    /// at all.
    pub fn from_toml_str(text: &str) -> Result<Self, LoadError> {
        let document: Document = toml::from_str(text).map_err(|e| LoadError::Parse(Box::new(e)))?;
        Self::from_document(document)
    }

    fn from_document(document: Document) -> Result<Self, LoadError> {
        if document.nodes.is_empty() {
            return Err(LoadError::NoNode);
        }
        let organisations =
            organisations(document.organisations.into_iter().map(|o| Organisation {
                id: o.id,
                name: o.name,
            }))?;
        let (nodes, by_system_id) = nodes(document.nodes, &organisations)?;
        let endpoints = endpoints(document.endpoints, &nodes, &organisations)?;
        if let Some(node) = nodes
            .keys()
            .find(|node| !endpoints.values().any(|endpoint| endpoint.node == **node))
        {
            return Err(LoadError::NodeWithoutEndpoint(node.clone()));
        }
        Ok(Self {
            organisations,
            nodes,
            endpoints,
            by_system_id,
        })
    }

    /// The organisation with this id.
    #[must_use]
    pub fn organisation(&self, id: &OrganisationId) -> Option<&Organisation> {
        self.organisations.get(id)
    }

    /// The node with this `node_id`.
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// The endpoint with this `endpoint_id`.
    #[must_use]
    pub fn endpoint(&self, id: &EndpointId) -> Option<&Endpoint> {
        self.endpoints.get(id)
    }

    /// The node whose openEHR `system_id` this is, compared as master05
    /// §"Composite Identifiers and Case" requires.
    #[must_use]
    pub fn node_for_system_id(&self, system_id: &SystemId) -> Option<&Node> {
        self.by_system_id
            .get(system_id)
            .and_then(|node| self.nodes.get(node))
    }

    /// Every organisation, ordered by id.
    pub fn organisations(&self) -> impl Iterator<Item = &Organisation> {
        self.organisations.values()
    }

    /// Every node, ordered by `node_id`.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Every endpoint, ordered by `endpoint_id`.
    pub fn endpoints(&self) -> impl Iterator<Item = &Endpoint> {
        self.endpoints.values()
    }

    /// The endpoints of one node, ordered by `endpoint_id`.
    pub fn endpoints_of<'a>(&'a self, node: &'a NodeId) -> impl Iterator<Item = &'a Endpoint> {
        self.endpoints
            .values()
            .filter(move |endpoint| endpoint.node == *node)
    }
}

fn organisations(
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

type NodeMaps = (BTreeMap<NodeId, Node>, BTreeMap<SystemId, NodeId>);

fn nodes(
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
        let node = Node {
            id: doc.id.clone(),
            organisation: doc.organisation,
            system_id: doc.system_id,
            identifiers,
        };
        nodes.insert(doc.id, node);
    }
    Ok((nodes, by_system_id))
}

fn endpoints(
    declared: Vec<EndpointDoc>,
    nodes: &BTreeMap<NodeId, Node>,
    organisations: &BTreeMap<OrganisationId, Organisation>,
) -> Result<BTreeMap<EndpointId, Endpoint>, LoadError> {
    let mut endpoints = BTreeMap::new();
    let mut url_owner: BTreeMap<Url, EndpointId> = BTreeMap::new();
    for doc in declared {
        if endpoints.contains_key(&doc.id) {
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
        let url = match base_url(&doc.url) {
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
                    url: url.into(),
                    first: entry.get().clone(),
                    second: doc.id,
                });
            }
            Entry::Vacant(entry) => {
                entry.insert(doc.id.clone());
            }
        }
        let endpoint = Endpoint {
            id: doc.id.clone(),
            node: doc.node,
            url,
            connection_type: doc.connection_type,
            managing_organisation: doc.managing_organisation,
            status: doc.status,
        };
        endpoints.insert(doc.id, endpoint);
    }
    Ok(endpoints)
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
