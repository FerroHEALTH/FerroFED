// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry's errors: an identifier that breaks its form, and a bootstrap
//! document that refuses to load.

use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

use crate::id::{EndpointId, NodeId, OrganisationId, SystemId};

/// The registry-owned namespace an [`IdError`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    /// A `node_id`.
    Node,
    /// An `endpoint_id`.
    Endpoint,
    /// An organisation id.
    Organisation,
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Node => "node_id",
            Self::Endpoint => "endpoint_id",
            Self::Organisation => "organisation id",
        })
    }
}

/// An identifier that does not have the form its namespace requires.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum IdError {
    /// A registry-owned identifier was empty.
    #[error("an empty {kind}")]
    Empty {
        /// The namespace.
        kind: IdKind,
    },
    /// A registry-owned identifier was longer than
    /// [`MAX_ID_LEN`](crate::id::MAX_ID_LEN) bytes.
    #[error("{kind} {found:?} is longer than 64 bytes")]
    TooLong {
        /// The namespace.
        kind: IdKind,
        /// The value as given.
        found: String,
    },
    /// A registry-owned identifier held a character outside ASCII letters,
    /// digits, `.`, `-` and `_`, or did not start with a letter or digit.
    #[error(
        "{kind} {found:?} must be ASCII letters, digits, '.', '-' and '_', starting with a letter or digit"
    )]
    Malformed {
        /// The namespace.
        kind: IdKind,
        /// The value as given.
        found: String,
    },
    /// A `system_id` that is not an openEHR `uid`.
    #[error("system_id {found:?} is not an openEHR uid")]
    SystemId {
        /// The value as given.
        found: String,
        /// The openEHR lexical refusal.
        #[source]
        source: openehr_base::v1_3::base_types::identification::lexical::IdError,
    },
    /// An `ehr_id` that is not an openEHR `HIER_OBJECT_ID`.
    #[error("ehr_id {found:?} is not an openEHR HIER_OBJECT_ID")]
    EhrId {
        /// The value as given.
        found: String,
        /// The openEHR lexical refusal.
        #[source]
        source: openehr_base::v1_3::base_types::identification::lexical::IdError,
    },
}

/// What refers to an organisation: a node that it operates, or an endpoint
/// that it manages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Referrer {
    /// The `organisation` of a node.
    Node(NodeId),
    /// The `managing_organisation` of an endpoint (N20).
    Endpoint(EndpointId),
}

impl fmt::Display for Referrer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(id) => write!(f, "node {id}"),
            Self::Endpoint(id) => write!(f, "endpoint {id}"),
        }
    }
}

/// Why an endpoint's base URL was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum UrlFault {
    /// The value is not a URL.
    #[error("not a URL")]
    Parse(#[source] url::ParseError),
    /// The scheme is neither `https` nor `http`.
    #[error("the scheme {0:?} is neither https nor http")]
    Scheme(String),
    /// The URL carries a user name or a password; outbound credentials are
    /// configured per endpoint as secrets, never in the URL.
    #[error("the URL carries credentials")]
    Credentials,
    /// The URL carries a query string or a fragment, which a base URL cannot.
    #[error("a base URL carries no query string or fragment")]
    NotABase,
}

/// A bootstrap document that refuses to load (docs/architecture.md section 8).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The document could not be read.
    #[error("the registry document {path} could not be read")]
    Read {
        /// The path given.
        path: PathBuf,
        /// The I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// The document is not TOML of the registry's shape: a syntax error, an
    /// unknown or missing field, a value of the wrong type, or an identifier
    /// that breaks its form.
    #[error("the registry document does not have the registry's shape")]
    Parse(#[source] Box<toml::de::Error>),
    /// The document admits no node.
    #[error("the registry document admits no node")]
    NoNode,
    /// Two organisations share an id.
    #[error("organisation {0} is declared twice")]
    DuplicateOrganisation(OrganisationId),
    /// Two nodes share a `node_id`.
    #[error("node {0} is declared twice")]
    DuplicateNode(NodeId),
    /// Two endpoints share an `endpoint_id`.
    #[error("endpoint {0} is declared twice")]
    DuplicateEndpoint(EndpointId),
    /// Two nodes share a `system_id`, which must be unique across the
    /// federation (§12b.2).
    #[error("system_id {system_id} is claimed by node {first} and node {second}")]
    DuplicateSystemId {
        /// The shared `system_id`.
        system_id: SystemId,
        /// The node declared first.
        first: NodeId,
        /// The node declared second.
        second: NodeId,
    },
    /// Two nodes, or one node twice, carry the same identifier.
    #[error("the node identifier {system}|{value} is carried by node {first} and node {second}")]
    DuplicateNodeIdentifier {
        /// The identifier system.
        system: String,
        /// The identifier value.
        value: String,
        /// The node declared first.
        first: NodeId,
        /// The node declared second (the same node for a repeat).
        second: NodeId,
    },
    /// A node identifier with an empty `system` or `value`.
    #[error("node {0} carries an identifier with an empty system or value")]
    EmptyNodeIdentifier(NodeId),
    /// A node's `product` or `version` is present and empty: an empty value
    /// describes nothing, and §9.5 reports these only as the gateway knows
    /// them.
    #[error("node {node} declares an empty {member}")]
    EmptyNodeDescription {
        /// The node.
        node: NodeId,
        /// `product` or `version`.
        member: &'static str,
    },
    /// A node or an endpoint names an organisation the document does not
    /// declare.
    #[error("{referrer} names organisation {organisation}, which is not declared")]
    UnknownOrganisation {
        /// What names the organisation.
        referrer: Referrer,
        /// The undeclared organisation.
        organisation: OrganisationId,
    },
    /// An endpoint names a node the document does not declare.
    #[error("endpoint {endpoint} names node {node}, which is not declared")]
    UnknownNode {
        /// The endpoint.
        endpoint: EndpointId,
        /// The undeclared node.
        node: NodeId,
    },
    /// An endpoint's base URL was refused.
    #[error("endpoint {endpoint} has an unusable base URL")]
    EndpointUrl {
        /// The endpoint.
        endpoint: EndpointId,
        /// Why the URL was refused.
        #[source]
        fault: UrlFault,
    },
    /// Two endpoints share a base URL, so they would be one interface under
    /// two ids.
    #[error("endpoint {first} and endpoint {second} share the base URL {url}")]
    DuplicateEndpointUrl {
        /// The shared base URL, as normalised.
        url: String,
        /// The endpoint declared first.
        first: EndpointId,
        /// The endpoint declared second.
        second: EndpointId,
    },
    /// A node with no endpoint, which nothing could reach (a node has 1..*
    /// endpoints, § The four identifiers).
    #[error("node {0} has no endpoint")]
    NodeWithoutEndpoint(NodeId),
}
