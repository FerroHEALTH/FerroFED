// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bootstrap document as written: TOML with `deny_unknown_fields`
//! throughout, validated into a snapshot by `snapshot`.

use serde::Deserialize;

use crate::id::{EndpointId, NodeId, OrganisationId, SystemId};
use crate::snapshot::{ConnectionType, EndpointStatus};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Document {
    #[serde(default, rename = "organisation")]
    pub(crate) organisations: Vec<OrganisationDoc>,
    #[serde(default, rename = "node")]
    pub(crate) nodes: Vec<NodeDoc>,
    #[serde(default, rename = "endpoint")]
    pub(crate) endpoints: Vec<EndpointDoc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganisationDoc {
    pub(crate) id: OrganisationId,
    pub(crate) name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeDoc {
    pub(crate) id: NodeId,
    pub(crate) organisation: OrganisationId,
    pub(crate) system_id: SystemId,
    pub(crate) product: Option<String>,
    pub(crate) version: Option<String>,
    #[serde(default, rename = "identifier")]
    pub(crate) identifiers: Vec<NodeIdentifierDoc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeIdentifierDoc {
    pub(crate) system: String,
    pub(crate) value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EndpointDoc {
    pub(crate) id: EndpointId,
    pub(crate) node: NodeId,
    pub(crate) url: String,
    pub(crate) connection_type: ConnectionType,
    pub(crate) managing_organisation: OrganisationId,
    #[serde(default)]
    pub(crate) status: EndpointStatus,
}
