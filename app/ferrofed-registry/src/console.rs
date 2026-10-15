// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources the operator console manages, and who owns each one.
//!
//! A [`ConsoleMember`] is one member of the federation as a configuration
//! revision holds it (`[[registry.members]]`): its organisation, its node and
//! its endpoints, owned together. A member the registry document or a care
//! services directory provides is the file's, read-only in the console, and
//! never split with it. [`Resources`] is the body of the gateway's
//! `GET /admin/config/resources`, naming the [`Owner`] of every member and
//! stored query. No specification governs this: our own design.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::document::{Document, EndpointDoc, NodeDoc, OrganisationDoc};
use crate::id::{EndpointId, NodeId, OrganisationId, SystemId};
use crate::secret::SecretUrl;
use crate::snapshot::{ConnectionType, EndpointStatus};

/// One member the operator console manages: an organisation, the node it
/// operates and that node's endpoints, each managed by the organisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleMember {
    /// The organisation that operates the node and manages its endpoints.
    pub organisation: ConsoleOrganisation,
    /// The node.
    pub node: ConsoleNode,
    /// The node's endpoints.
    pub endpoints: Vec<ConsoleEndpoint>,
}

/// The organisation of a [`ConsoleMember`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleOrganisation {
    /// The organisation's id.
    pub id: OrganisationId,
    /// Its display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The node of a [`ConsoleMember`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleNode {
    /// The node's `node_id`.
    pub id: NodeId,
    /// The node's openEHR `system_id`.
    pub system_id: SystemId,
    /// The node's CDR product name (§9.5, N40).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// The node's CDR product version (§9.5, N40).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// One endpoint of a [`ConsoleMember`], as the registry document's
/// `[[endpoint]]` declares one, its node and managing organisation the
/// member's own.
///
/// `Debug` shows the URL as [`SecretUrl`] redacts it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleEndpoint {
    /// The endpoint's stable `endpoint_id` (N19).
    pub id: EndpointId,
    /// The endpoint's ITS-REST base URL; one carrying userinfo is refused.
    pub url: String,
    /// The endpoint's connection type (N19, §15.2).
    pub connection_type: ConnectionType,
    /// Whether the endpoint is in service.
    #[serde(default)]
    pub status: EndpointStatus,
    /// The `code` values by which the node marks a `403` as a consent
    /// refusal (§11.1, N27).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consent_refusal_codes: Vec<String>,
    /// The categories an `hl7-fhir-rest` endpoint serves itself.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// The profiles an `hl7-fhir-rest` endpoint's documents follow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    /// The identifier system an `hl7-fhir-rest` endpoint is told the
    /// member's `ehr_id` in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ehr_id_system: Option<String>,
}

impl fmt::Debug for ConsoleEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsoleEndpoint")
            .field("id", &self.id)
            .field("url", &SecretUrl::new(self.url.as_str()))
            .field("connection_type", &self.connection_type)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl ConsoleMember {
    /// The member as a registry document of one organisation, one node and
    /// its endpoints, which
    /// [`RegistrySnapshot::with_console`](crate::snapshot::RegistrySnapshot::with_console)
    /// validates and joins.
    #[must_use]
    pub fn document(&self) -> Document {
        let organisation = self.organisation.id.clone();
        Document {
            organisations: vec![OrganisationDoc {
                id: organisation.clone(),
                name: self.organisation.name.clone(),
                identifiers: Vec::new(),
            }],
            nodes: vec![NodeDoc {
                id: self.node.id.clone(),
                organisation: organisation.clone(),
                system_id: self.node.system_id.clone(),
                product: self.node.product.clone(),
                version: self.node.version.clone(),
                identifiers: Vec::new(),
            }],
            endpoints: self
                .endpoints
                .iter()
                .map(|endpoint| EndpointDoc {
                    id: endpoint.id.clone(),
                    node: self.node.id.clone(),
                    url: SecretUrl::new(endpoint.url.as_str()),
                    connection_type: endpoint.connection_type,
                    managing_organisation: organisation.clone(),
                    status: endpoint.status,
                    consent_refusal_codes: endpoint.consent_refusal_codes.clone(),
                    categories: endpoint.categories.clone(),
                    profiles: endpoint.profiles.clone(),
                    ehr_id_system: endpoint.ehr_id_system.clone(),
                })
                .collect(),
            creating_systems: Vec::new(),
        }
    }
}

/// Who provides a configured resource, and so who may change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Owner {
    /// The configuration file: the registry document it names, or the
    /// read-only stored-query definition files. Read-only in the console.
    File,
    /// The care services directory the registry is read from. Read-only in
    /// the console.
    Directory,
    /// The operator console, through a configuration revision.
    Console,
    /// The gateway itself: the stored queries of its reserved namespace.
    Gateway,
    /// The stored-query store, written by a `PUT` (§12.7) or by the console;
    /// a stored version is immutable (N44).
    Store,
}

impl Owner {
    /// The owner as the report writes it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Console => "console",
            Self::Gateway => "gateway",
            Self::Store => "store",
        }
    }
}

/// Where a member the console holds stands against the running registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ConsoleState {
    /// The newest revision holds it as the running registry runs it.
    Running,
    /// The newest revision adds or changes it, and it is not applied yet.
    Pending,
    /// The running revision holds it and the newest one removes it.
    Removing,
    /// The registry document provides a member that shares an identity with
    /// it, so the document's runs and this one does not.
    Shadowed,
}

impl ConsoleState {
    /// The state as the report writes it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Pending => "pending",
            Self::Removing => "removing",
            Self::Shadowed => "shadowed",
        }
    }
}

/// One member and its owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberResource {
    /// The node's `node_id`.
    pub node: NodeId,
    /// The organisation that operates it.
    pub organisation: OrganisationId,
    /// The node's `system_id`.
    pub system_id: SystemId,
    /// Its endpoints, by `endpoint_id`.
    pub endpoints: Vec<EndpointId>,
    /// Who provides it.
    pub owner: Owner,
    /// Where it stands, for a member the console holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ConsoleState>,
    /// The member as the newest revision holds it, for one the console
    /// holds, which an edit starts from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<ConsoleMember>,
}

/// One stored-query version and its owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryResource {
    /// The qualified query name.
    pub name: String,
    /// Its `major.minor.patch` version.
    pub version: String,
    /// Who provides it.
    pub owner: Owner,
}

/// The body of `GET /admin/config/resources`: every member and every stored
/// query, with its owner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    /// The revision the store marks running.
    pub running: Option<u64>,
    /// The newest revision, which a console change is saved on top of.
    pub head: Option<u64>,
    /// Every member, the running registry's and the newest revision's, by
    /// `node_id`.
    pub members: Vec<MemberResource>,
    /// Every stored-query version the gateway holds, by name and version.
    pub stored_queries: Vec<QueryResource>,
}
