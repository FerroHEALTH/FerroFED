// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A valid document loads, and the snapshot answers every lookup (N19 to N21).

use std::error::Error;
use std::path::Path;

use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ferrofed_registry::snapshot::{ConnectionType, Endpoint, EndpointStatus, RegistrySnapshot};

use crate::fixture::TWO_NODES;

type TestResult = Result<(), Box<dyn Error>>;

// conformance: CP-13
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

// conformance: CP-13
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

// conformance: CP-13
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

/// One node recording its CDR product and version.
const DESCRIBED: &str = r#"
[[organisation]]
id = "org-a"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"
product = "FerroEHR"
version = "4.3.1"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "https://cdr-a.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"
"#;

#[test]
fn a_node_carries_the_product_and_version_the_registry_records() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(DESCRIBED)?;
    let node = registry
        .node(&NodeId::new("node-a")?)
        .ok_or("node-a is declared")?;
    assert_eq!(Some("FerroEHR"), node.product());
    assert_eq!(Some("4.3.1"), node.version());
    Ok(())
}

#[test]
fn a_node_that_records_neither_has_no_product_or_version() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;
    for node in registry.nodes() {
        assert_eq!(None, node.product(), "{} records no product", node.id());
        assert_eq!(None, node.version(), "{} records no version", node.id());
    }
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

#[test]
fn a_member_is_asked_through_its_first_active_endpoint() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;
    let node_a: NodeId = "node-a".parse()?;
    let node_b: NodeId = "node-b".parse()?;
    let asked = |node: &NodeId| registry.asked_through(node).map(|e| e.id().as_str());
    assert_eq!(
        asked(&node_a),
        Some("node-a-pub"),
        "node-a-region is suspended"
    );
    assert_eq!(asked(&node_b), Some("node-b-pub"), "node-b's only endpoint");
    Ok(())
}

#[test]
fn a_member_whose_admitted_endpoints_are_all_suspended_is_asked_through_none() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;
    let node_a: NodeId = "node-a".parse()?;
    let region: EndpointId = "node-a-region".parse()?;
    let public: EndpointId = "node-a-pub".parse()?;
    let suspended = registry.asked_through_among(&node_a, |endpoint| *endpoint == region);
    assert!(
        suspended.is_none(),
        "the one admitted endpoint is suspended"
    );
    let active = registry.asked_through_among(&node_a, |endpoint| *endpoint == public);
    assert_eq!(
        active.map(Endpoint::id),
        Some(&public),
        "the admitted active endpoint"
    );
    Ok(())
}

#[test]
fn an_endpoint_lists_no_consent_refusal_code_unless_the_document_names_one() -> TestResult {
    let registry = RegistrySnapshot::from_toml_str(TWO_NODES)?;
    for endpoint in registry.endpoints() {
        assert!(
            endpoint.consent_refusal_codes().is_empty(),
            "§11.1: empty by default, so every refusal is node-error"
        );
    }
    let named = TWO_NODES.replacen(
        "connection_type = \"openehr-rest-query\"",
        "connection_type = \"openehr-rest-query\"\nconsent_refusal_codes = [\"consent-refused\"]",
        1,
    );
    let registry = RegistrySnapshot::from_toml_str(&named)?;
    let listed: usize = registry
        .endpoints()
        .map(|endpoint| endpoint.consent_refusal_codes().len())
        .sum();
    assert_eq!(1, listed, "the one endpoint that names a code lists it");
    Ok(())
}

#[test]
fn an_empty_consent_refusal_code_is_refused() {
    let document = TWO_NODES.replacen(
        "connection_type = \"openehr-rest-query\"",
        "connection_type = \"openehr-rest-query\"\nconsent_refusal_codes = [\"\"]",
        1,
    );
    assert!(matches!(
        RegistrySnapshot::from_toml_str(&document),
        Err(LoadError::EmptyConsentRefusalCode(_))
    ));
}
