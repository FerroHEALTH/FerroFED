// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A valid document loads, and the snapshot answers every lookup (N19 to N21).

use std::error::Error;
use std::path::Path;

use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus, RegistrySnapshot};

use crate::fixture::TWO_NODES;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn every_member_is_answered_by_its_id() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;

    let node_ids: Vec<&str> = registry.nodes().map(|n| n.id().as_str()).collect();
    assert_eq!(node_ids, ["node-a", "node-b"], "nodes in node_id order");
    let endpoint_ids: Vec<&str> = registry.endpoints().map(|e| e.id().as_str()).collect();
    assert_eq!(
        endpoint_ids,
        ["node-a-pub", "node-a-region", "node-b-pub"],
        "endpoints in endpoint_id order"
    );

    let org: OrganisationId = "org-a".parse()?;
    assert_eq!(
        registry.organisation(&org).and_then(|o| o.name()),
        Some("Hospital A"),
        "the organisation name is carried"
    );

    let node_a: NodeId = "node-a".parse()?;
    let node = registry.node(&node_a).ok_or("node-a is a member")?;
    assert_eq!(node.organisation(), &org, "the operating organisation");
    assert_eq!(
        node.system_id().as_str(),
        "cdr-a.example.org",
        "the system_id"
    );
    let identifiers: Vec<(&str, &str)> = node
        .identifiers()
        .iter()
        .map(|i| (i.system(), i.value()))
        .collect();
    assert_eq!(
        identifiers,
        [("urn:oid:2.999.1", "node-a")],
        "the node identifiers"
    );
    Ok(())
}

#[test]
fn an_endpoint_carries_its_url_connection_type_and_managing_organisation() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;
    let id: EndpointId = "node-a-region".parse()?;
    let endpoint = registry
        .endpoint(&id)
        .ok_or("node-a-region is registered")?;

    assert_eq!(endpoint.node().as_str(), "node-a", "the owning node");
    assert_eq!(
        endpoint.url().as_str(),
        "https://internal.cdr-a.example.org/openehr",
        "the base URL"
    );
    assert_eq!(
        endpoint.connection_type(),
        ConnectionType::OpenehrRestQuery,
        "N19"
    );
    assert_eq!(
        endpoint.connection_type().code(),
        "openehr-rest-query",
        "the code as written"
    );
    assert_eq!(
        endpoint.managing_organisation().as_str(),
        "org-region",
        "N20: the manager need not be the operator"
    );
    assert_eq!(
        endpoint.status(),
        EndpointStatus::Suspended,
        "the declared status"
    );

    let public: EndpointId = "node-a-pub".parse()?;
    let public = registry
        .endpoint(&public)
        .ok_or("node-a-pub is registered")?;
    assert_eq!(
        public.status(),
        EndpointStatus::Active,
        "active when the document is silent"
    );
    Ok(())
}

#[test]
fn a_node_lists_its_own_endpoints() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;
    let node_a: NodeId = "node-a".parse()?;
    let ids: Vec<&str> = registry
        .endpoints_of(&node_a)
        .map(|e| e.id().as_str())
        .collect();
    assert_eq!(
        ids,
        ["node-a-pub", "node-a-region"],
        "node-a's endpoints only"
    );
    Ok(())
}

#[test]
fn a_system_id_routes_to_its_node() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;
    let oid: SystemId = "2.999.20.1".parse()?;
    assert_eq!(
        registry.node_for_system_id(&oid).map(|n| n.id().as_str()),
        Some("node-b"),
        "an ISO_OID system_id"
    );
    let unknown: SystemId = "cdr-z.example.org".parse()?;
    assert!(
        registry.node_for_system_id(&unknown).is_none(),
        "a system_id no node claims answers nothing"
    );
    Ok(())
}

#[test]
fn a_system_id_compares_without_ascii_case() -> TestResult {
    // master05 §"Composite Identifiers and Case": identical apart from case
    // is the same identifier, and the stored value keeps its case.
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;
    let upper: SystemId = "CDR-A.Example.ORG".parse()?;
    let node = registry
        .node_for_system_id(&upper)
        .ok_or("found regardless of case")?;
    assert_eq!(node.id().as_str(), "node-a", "the node with that system_id");
    assert_eq!(
        node.system_id().as_str(),
        "cdr-a.example.org",
        "stored as written"
    );
    Ok(())
}

#[test]
fn a_document_on_disk_loads() -> TestResult {
    let dir = std::env::temp_dir().join(format!("ferrofed-registry-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("registry.toml");
    std::fs::write(&path, TWO_NODES)?;
    let registry = RegistrySnapshot::read(&path)?;
    assert_eq!(registry.nodes().count(), 2, "both nodes");
    std::fs::remove_file(&path)?;
    std::fs::remove_dir(&dir)?;
    Ok(())
}

#[test]
fn a_missing_document_is_a_read_error() {
    let missing = Path::new("/nonexistent/ferrofed/registry.toml");
    assert!(
        matches!(RegistrySnapshot::read(missing), Err(LoadError::Read { .. })),
        "a missing file is LoadError::Read"
    );
}
