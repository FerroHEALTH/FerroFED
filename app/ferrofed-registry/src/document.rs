// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bootstrap document as written, before validation.
//!
//! The native form is TOML with `deny_unknown_fields` throughout, read by
//! [`RegistrySnapshot::from_toml_str`](crate::snapshot::RegistrySnapshot::from_toml_str).
//! Another form (FHIR `Organization` and `Endpoint` resources, N19) is read
//! into the same [`Document`] by its own reader and validated by
//! [`RegistrySnapshot::from_document`](crate::snapshot::RegistrySnapshot::from_document),
//! so both forms meet one set of membership rules.

use serde::Deserialize;

use crate::id::{EndpointId, NodeId, OrganisationId, SystemId};
use crate::secret::SecretUrl;
use crate::snapshot::{ConnectionType, EndpointStatus};

/// The federation's members as a document declares them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    /// The `[[organisation]]` entries.
    #[serde(default, rename = "organisation")]
    pub organisations: Vec<OrganisationDoc>,
    /// The `[[node]]` entries.
    #[serde(default, rename = "node")]
    pub nodes: Vec<NodeDoc>,
    /// The `[[endpoint]]` entries.
    #[serde(default, rename = "endpoint")]
    pub endpoints: Vec<EndpointDoc>,
    /// The `[[creating_system]]` entries.
    #[serde(default, rename = "creating_system")]
    pub creating_systems: Vec<CreatingSystemDoc>,
}

/// One organisation as declared.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganisationDoc {
    /// The organisation's id.
    pub id: OrganisationId,
    /// The organisation's display name.
    pub name: Option<String>,
}

/// One node as declared.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeDoc {
    /// The node's `node_id`.
    pub id: NodeId,
    /// The organisation that operates the node.
    pub organisation: OrganisationId,
    /// The node's openEHR `system_id`.
    pub system_id: SystemId,
    /// The node's CDR product name (§9.5, N40).
    pub product: Option<String>,
    /// The node's CDR product version (§9.5, N40).
    pub version: Option<String>,
    /// The identifiers outside services name the node by.
    #[serde(default, rename = "identifier")]
    pub identifiers: Vec<NodeIdentifierDoc>,
}

/// One node identifier as declared, `system|value`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeIdentifierDoc {
    /// The identifier system.
    pub system: String,
    /// The identifier value.
    pub value: String,
}

/// One endpoint as declared.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointDoc {
    /// The endpoint's stable `endpoint_id` (N19).
    pub id: EndpointId,
    /// The node the endpoint belongs to.
    pub node: NodeId,
    /// The endpoint's ITS-REST base URL, as written; the snapshot refuses
    /// one that carries userinfo.
    pub url: SecretUrl,
    /// The endpoint's connection type (N19, §15.2).
    pub connection_type: ConnectionType,
    /// The one organisation that manages the endpoint (N20).
    pub managing_organisation: OrganisationId,
    /// Whether the endpoint is in service.
    #[serde(default)]
    pub status: EndpointStatus,
    /// The `code` values of an ITS-REST `Error` by which this endpoint's node
    /// marks a `403` as a consent refusal, reported `consent-denied` (§11.1,
    /// N27). Empty by default: every refusal is then `node-error`, because
    /// ITS-REST defines no consent signal (no specification governs the key:
    /// our own design).
    #[serde(default)]
    pub consent_refusal_codes: Vec<String>,
}

/// One `creating_system_id` mapped to the endpoint that answers for it (N21,
/// §12.2).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatingSystemDoc {
    /// The mapped `creating_system_id`.
    pub creating_system_id: SystemId,
    /// The endpoint that answers for it.
    pub endpoint: EndpointId,
}
