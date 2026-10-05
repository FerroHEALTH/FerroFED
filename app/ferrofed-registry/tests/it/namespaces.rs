// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `node_id`, `endpoint_id` and `system_id` are distinct namespaces: the same
//! string in all three is three identifiers, and each lookup answers only in
//! its own namespace (N32, §12a.1).

use std::error::Error;

use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ferrofed_registry::snapshot::RegistrySnapshot;

/// One string, `shared.example.org`, as an organisation id, a `node_id`, an
/// `endpoint_id` and a `system_id`, each naming a different thing.
const COINCIDING: &str = r#"
[[organisation]]
id = "shared.example.org"

[[node]]
id = "shared.example.org"
organisation = "shared.example.org"
system_id = "cdr-1.example.org"

[[node]]
id = "node-2"
organisation = "shared.example.org"
system_id = "shared.example.org"

[[endpoint]]
id = "shared.example.org"
node = "node-2"
url = "https://cdr-2.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "shared.example.org"

[[endpoint]]
id = "node-1-pub"
node = "shared.example.org"
url = "https://cdr-1.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "shared.example.org"
"#;

#[test]
fn one_string_in_every_namespace_names_different_things() -> Result<(), Box<dyn Error>> {
    let registry = RegistrySnapshot::from_toml_str(COINCIDING)?;
    let shared = "shared.example.org";

    let node: NodeId = shared.parse()?;
    let endpoint: EndpointId = shared.parse()?;
    let system_id: SystemId = shared.parse()?;
    let organisation: OrganisationId = shared.parse()?;

    let by_node_id = registry.node(&node).ok_or("a node with that node_id")?;
    assert_eq!(
        by_node_id.system_id().as_str(),
        "cdr-1.example.org",
        "the node_id lookup finds the node whose node_id it is"
    );

    let by_system_id = registry
        .node_for_system_id(&system_id)
        .ok_or("a node with that system_id")?;
    assert_eq!(
        by_system_id.id().as_str(),
        "node-2",
        "the system_id lookup finds a different node"
    );

    let by_endpoint_id = registry
        .endpoint(&endpoint)
        .ok_or("an endpoint with that id")?;
    assert_eq!(
        by_endpoint_id.node().as_str(),
        "node-2",
        "the endpoint_id lookup finds the endpoint, not the node named alike"
    );

    assert!(
        registry.organisation(&organisation).is_some(),
        "the organisation answers in its own namespace"
    );
    Ok(())
}

#[test]
fn a_node_id_finds_no_endpoint_and_an_endpoint_id_finds_no_node() -> Result<(), Box<dyn Error>> {
    let registry = RegistrySnapshot::from_toml_str(COINCIDING)?;
    let endpoint_only: EndpointId = "node-1-pub".parse()?;
    let as_node: NodeId = "node-1-pub".parse()?;
    assert!(
        registry.endpoint(&endpoint_only).is_some(),
        "it is an endpoint_id"
    );
    assert!(registry.node(&as_node).is_none(), "and no node_id");

    let node_only: NodeId = "node-2".parse()?;
    let as_endpoint: EndpointId = "node-2".parse()?;
    assert!(registry.node(&node_only).is_some(), "it is a node_id");
    assert!(
        registry.endpoint(&as_endpoint).is_none(),
        "and no endpoint_id"
    );
    Ok(())
}
