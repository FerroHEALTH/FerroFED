// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a registry document in FHIR form refuses to load.

use std::fmt;
use std::path::PathBuf;

use ferrofed_registry::error::{IdError, LoadError};
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ihe_iti::mcsd::error::DirectoryError;
use thiserror::Error;

use super::{
    CREATING_SYSTEM_ID_SYSTEM, ENDPOINT_ID_SYSTEM, NODE_ID_SYSTEM, ORGANISATION_ID_SYSTEM,
    SYSTEM_ID_SYSTEM,
};

/// A Bundle entry, named before its registry id is known: its position, and
/// its logical id when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    index: usize,
    logical_id: Option<String>,
}

impl Entry {
    pub(super) fn new(index: usize, logical_id: Option<&str>) -> Self {
        Self {
            index,
            logical_id: logical_id.map(str::to_owned),
        }
    }

    /// The entry's position in the Bundle.
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// The resource's logical id.
    #[must_use]
    pub fn logical_id(&self) -> Option<&str> {
        self.logical_id.as_deref()
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bundle.entry[{}]", self.index)?;
        match &self.logical_id {
            Some(id) => write!(f, " (id {id:?})"),
            None => Ok(()),
        }
    }
}

/// Why a resource does not carry exactly one usable identifier in a system.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum IdentifierFault {
    /// No identifier carries the system.
    #[error("no identifier carries the system")]
    Missing,
    /// More than one identifier carries the system.
    #[error("more than one identifier carries the system")]
    Repeated,
    /// An identifier carries the system and no value.
    #[error("an identifier carries the system and no value")]
    NoValue,
    /// The value does not have the form its namespace requires.
    #[error("the value is not a usable id")]
    Malformed(#[source] IdError),
}

/// Why an endpoint's `connectionType` is not an openEHR Query API code (N19,
/// §15.2).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ConnectionTypeFault {
    /// The code is `hl7-fhir-rest`, which denotes a FHIR REST endpoint and
    /// never an openEHR Query API (§15.2).
    #[error("hl7-fhir-rest denotes a FHIR REST endpoint, never an openEHR Query API (§15.2)")]
    FhirRest,
    /// The `connectionType` carries no code.
    #[error("the connectionType carries no code")]
    NoCode,
    /// The code carries no system, which makes it an informal string (N19).
    #[error("the code {code:?} carries no system, so it is an informal string (N19)")]
    NoSystem {
        /// The code as given.
        code: String,
    },
    /// The system and code are not a defined openEHR Query API code (N19).
    #[error("{system}|{code} is not a defined openEHR Query API connection type (N19)")]
    Unbound {
        /// The system as given.
        system: String,
        /// The code as given.
        code: String,
    },
}

/// Why an endpoint's `managingOrganization` names no organisation of the
/// document (N20).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ReferenceFault {
    /// The endpoint has no `managingOrganization`.
    #[error("the endpoint has no managingOrganization")]
    Missing,
    /// The reference has no literal `reference`, so no entry answers it.
    #[error("the reference carries no literal reference")]
    NotLiteral,
    /// The reference names no `Organization` of the Bundle.
    #[error("the reference {0:?} names no Organization of the Bundle")]
    Outside(String),
}

/// Why an endpoint has no single operating organisation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum OperatorFault {
    /// No `Organization` lists the endpoint in its `endpoint` references.
    #[error("no Organization lists the endpoint")]
    Unlisted,
    /// Two organisations list the endpoint.
    #[error("organisation {first} and organisation {second} both list the endpoint")]
    Several {
        /// The organisation listed first.
        first: OrganisationId,
        /// The organisation listed second.
        second: OrganisationId,
    },
}

/// A registry document in FHIR form that refuses to load.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FhirFormError {
    /// The document could not be read.
    #[error("the registry document {path} could not be read")]
    Read {
        /// The path given.
        path: PathBuf,
        /// The I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// The document is not a Bundle of `Organization` and `Endpoint`
    /// resources.
    #[error("the registry document is not a Bundle of Organization and Endpoint resources")]
    Directory(#[source] DirectoryError),
    /// An organisation carries no single usable id.
    #[error("the Organization at {organisation} has no single organisation id in {system}", system = ORGANISATION_ID_SYSTEM)]
    OrganisationId {
        /// The organisation's entry.
        organisation: Entry,
        /// What is wrong with its identifiers.
        #[source]
        fault: IdentifierFault,
    },
    /// An organisation is marked inactive, so it can neither operate nor
    /// manage a member.
    #[error("organisation {0} is marked inactive")]
    OrganisationInactive(OrganisationId),
    /// An organisation's `endpoint` list names no `Endpoint` of the Bundle.
    #[error("organisation {organisation} lists an endpoint that is not in the Bundle")]
    OrganisationEndpoint {
        /// The organisation.
        organisation: OrganisationId,
        /// Why the reference names nothing.
        #[source]
        fault: ReferenceFault,
    },
    /// An endpoint carries no single usable `endpoint_id` (N19).
    #[error("the Endpoint at {endpoint} has no single endpoint id in {system} (N19)", system = ENDPOINT_ID_SYSTEM)]
    EndpointId {
        /// The endpoint's entry.
        endpoint: Entry,
        /// What is wrong with its identifiers.
        #[source]
        fault: IdentifierFault,
    },
    /// An endpoint's `connectionType` is not an openEHR Query API code (N19,
    /// §15.2, CP-20).
    #[error("endpoint {endpoint} does not carry an openEHR Query API connectionType")]
    ConnectionType {
        /// The endpoint.
        endpoint: EndpointId,
        /// Why the code is refused.
        #[source]
        fault: ConnectionTypeFault,
    },
    /// An endpoint's `status` is neither `active` nor `suspended`.
    #[error("endpoint {endpoint} has the status {found:?}, which is neither active nor suspended")]
    Status {
        /// The endpoint.
        endpoint: EndpointId,
        /// The status code as given.
        found: Option<String>,
    },
    /// An endpoint's `address` carries no value.
    #[error("endpoint {0} has no address")]
    Address(EndpointId),
    /// An endpoint's `managingOrganization` names no organisation of the
    /// document (N20).
    #[error("endpoint {endpoint} has no managing organisation in the Bundle (N20)")]
    ManagingOrganisation {
        /// The endpoint.
        endpoint: EndpointId,
        /// Why the reference names nothing.
        #[source]
        fault: ReferenceFault,
    },
    /// An endpoint carries no single usable `node_id`.
    #[error("endpoint {endpoint} has no single node id in {system}", system = NODE_ID_SYSTEM)]
    NodeId {
        /// The endpoint.
        endpoint: EndpointId,
        /// What is wrong with its identifiers.
        #[source]
        fault: IdentifierFault,
    },
    /// An endpoint carries no single usable `system_id`.
    #[error("endpoint {endpoint} has no single system_id in {system}", system = SYSTEM_ID_SYSTEM)]
    SystemId {
        /// The endpoint.
        endpoint: EndpointId,
        /// What is wrong with its identifiers.
        #[source]
        fault: IdentifierFault,
    },
    /// An endpoint carries a `creating_system_id` that is not usable.
    #[error("endpoint {endpoint} has an unusable creating_system_id in {system}", system = CREATING_SYSTEM_ID_SYSTEM)]
    CreatingSystemId {
        /// The endpoint.
        endpoint: EndpointId,
        /// What is wrong with the identifier.
        #[source]
        fault: IdentifierFault,
    },
    /// An endpoint has no single operating organisation.
    #[error("endpoint {endpoint} has no single operating organisation")]
    Operator {
        /// The endpoint.
        endpoint: EndpointId,
        /// Why.
        #[source]
        fault: OperatorFault,
    },
    /// Two endpoints of one node are operated by two organisations.
    #[error("node {node} is operated by organisation {first} and organisation {second}")]
    NodeOperator {
        /// The node.
        node: NodeId,
        /// The organisation of the node's first endpoint.
        first: OrganisationId,
        /// The organisation of a later endpoint.
        second: OrganisationId,
    },
    /// Two endpoints of one node carry two `system_id`s.
    #[error("node {node} carries system_id {first} and system_id {second}")]
    NodeSystemId {
        /// The node.
        node: NodeId,
        /// The `system_id` of the node's first endpoint.
        first: SystemId,
        /// The `system_id` of a later endpoint.
        second: SystemId,
    },
    /// The members the document declares break a membership rule every form
    /// of the document meets.
    #[error("the registry document breaks a membership rule")]
    Registry(#[source] LoadError),
}
