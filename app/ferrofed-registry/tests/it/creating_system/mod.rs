// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The follow-up routing table maps every observed `creating_system_id`
//! (N21, §12.2): the members' own `system_id`s, the document's
//! `[[creating_system]]` mappings, and mappings learned from answers. These
//! tests cover the mapping half of CP-13; CP-13 stays planned in the matrix
//! while its other work is open, so they carry no conformance marker.

use std::error::Error;

use ferrofed_registry::id::{EndpointId, NodeId, SystemId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_base::prelude::ObjectVersionId;

use crate::fixture::{TWO_NODES, creating_system, two_nodes_with};

mod learned;
mod load;
mod props;

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic `object_id`, invented for the tests.
const OBJECT_ID: &str = "8849a7c2-1f0e-4b5a-9c3d-2e6f8a1b4c70";

/// A `creating_system_id` no node holds as its own, registered to `node-a`
/// in [`registered`].
const LEGACY: &str = "legacy-a.example.org";

/// The two-node federation without any `[[creating_system]]` mapping.
fn unregistered() -> Result<RegistrySnapshot, Box<dyn Error>> {
    Ok(RegistrySnapshot::from_toml_str(TWO_NODES)?)
}

/// The two-node federation with [`LEGACY`] registered to `node-a-pub`.
fn registered() -> Result<RegistrySnapshot, Box<dyn Error>> {
    Ok(RegistrySnapshot::from_toml_str(&two_nodes_with(
        &creating_system(LEGACY, "node-a-pub"),
    ))?)
}

/// Version 1 of the test object, created by `creating_system_id`.
fn version(creating_system_id: &str) -> Result<ObjectVersionId, Box<dyn Error>> {
    Ok(ObjectVersionId::new(format!(
        "{OBJECT_ID}::{creating_system_id}::1"
    ))?)
}

fn endpoint(id: &str) -> Result<EndpointId, Box<dyn Error>> {
    Ok(id.parse()?)
}

fn node(id: &str) -> Result<NodeId, Box<dyn Error>> {
    Ok(id.parse()?)
}

fn system(id: &str) -> Result<SystemId, Box<dyn Error>> {
    Ok(id.parse()?)
}
