// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document's `[[creating_system]]` mappings load, and every mapping the
//! routing table could not answer from exactly is refused at load, naming the
//! key (N21, §12.2).
#![allow(clippy::panic, reason = "the helpers fail the test they serve")]

use ferrofed_registry::creating_system::CreatingSystemRoute;
use ferrofed_registry::error::LoadError;
use ferrofed_registry::snapshot::RegistrySnapshot;

use super::{LEGACY, TestResult, endpoint, node, registered, system};
use crate::fixture::{creating_system, two_nodes_with};

fn refusal(document: &str) -> LoadError {
    match RegistrySnapshot::from_toml_str(document) {
        Ok(_) => panic!("the document must refuse to load"),
        Err(error) => error,
    }
}

fn shape_refusal(document: &str) -> String {
    match refusal(document) {
        LoadError::Parse(source) => source.to_string(),
        other => panic!("expected a shape refusal, got {other:?}"),
    }
}

// conformance: CP-13
#[test]
fn a_registered_mapping_loads_and_is_listed() -> TestResult {
    let snapshot = registered()?;
    let listed: Vec<(&str, &str)> = snapshot
        .creating_systems()
        .map(|(id, endpoint)| (id.as_str(), endpoint.as_str()))
        .collect();
    assert_eq!(listed, [(LEGACY, "node-a-pub")]);
    assert_eq!(
        snapshot.registered_route(&system("LEGACY-A.example.org")?),
        Some(CreatingSystemRoute::Registered {
            node: node("node-a")?,
            endpoint: endpoint("node-a-pub")?,
        }),
        "compared without regard to ASCII case"
    );
    Ok(())
}

#[test]
fn a_registered_creating_system_id_is_named_as_the_document_spells_it() -> TestResult {
    let snapshot = registered()?;
    let asked = system("LEGACY-A.EXAMPLE.ORG")?;
    let (spelled, route) = snapshot
        .registered_creating_system(&asked)
        .ok_or("the mapping answers")?;
    assert_eq!(LEGACY, spelled.as_str(), "the mapping's spelling");
    assert_eq!(snapshot.registered_route(&asked), Some(route));
    let member = system("CDR-A.Example.Org")?;
    let (spelled, route) = snapshot
        .registered_creating_system(&member)
        .ok_or("the member answers")?;
    assert_eq!(
        "cdr-a.example.org",
        spelled.as_str(),
        "the member's spelling"
    );
    assert_eq!(
        CreatingSystemRoute::Member {
            node: node("node-a")?,
        },
        route
    );
    assert_eq!(
        None,
        snapshot.registered_creating_system(&system("external.example.org")?)
    );
    Ok(())
}

#[test]
fn a_document_without_mappings_registers_only_the_members() -> TestResult {
    let snapshot = super::unregistered()?;
    assert_eq!(snapshot.creating_systems().count(), 0);
    assert_eq!(snapshot.registered_route(&system(LEGACY)?), None);
    Ok(())
}

#[test]
fn a_mapping_to_an_undeclared_endpoint_is_refused() {
    let document = two_nodes_with(&creating_system(LEGACY, "node-z-pub"));
    let error = refusal(&document);
    assert!(
        matches!(
            &error,
            LoadError::UnknownCreatingSystemEndpoint { creating_system_id, endpoint }
                if creating_system_id.as_str() == LEGACY && endpoint.as_str() == "node-z-pub"
        ),
        "a dangling creating_system.endpoint: {error:?}"
    );
    assert!(
        error.to_string().contains("creating_system"),
        "the key is named: {error}"
    );
}

#[test]
fn one_creating_system_id_mapped_to_two_endpoints_is_refused() {
    let document = two_nodes_with(&format!(
        "{}\n{}",
        creating_system(LEGACY, "node-a-pub"),
        creating_system(LEGACY, "node-b-pub")
    ));
    assert!(
        matches!(
            refusal(&document),
            LoadError::DuplicateCreatingSystemId { creating_system_id, first, second }
                if creating_system_id.as_str() == LEGACY
                    && first.as_str() == "node-a-pub"
                    && second.as_str() == "node-b-pub"
        ),
        "two answers for one creating_system_id"
    );
}

#[test]
fn a_mapping_repeated_to_the_same_endpoint_is_refused() {
    let mapping = creating_system(LEGACY, "node-a-pub");
    let document = two_nodes_with(&format!("{mapping}\n{mapping}"));
    assert!(
        matches!(
            refusal(&document),
            LoadError::DuplicateCreatingSystemId { first, second, .. }
                if first == second
        ),
        "a repeat is a duplicate"
    );
}

#[test]
fn mappings_differing_only_in_case_are_one_creating_system_id() {
    // master05 §"Composite Identifiers and Case": the two spell one id.
    let document = two_nodes_with(&format!(
        "{}\n{}",
        creating_system(LEGACY, "node-a-pub"),
        creating_system("LEGACY-A.EXAMPLE.ORG", "node-b-pub")
    ));
    assert!(
        matches!(
            refusal(&document),
            LoadError::DuplicateCreatingSystemId { .. }
        ),
        "a case variant is the same id"
    );
}

#[test]
fn a_mapping_of_a_members_own_system_id_is_refused() {
    // §12.4: a member's own system_id maps to that member, and no mapping moves it.
    for (spelled, target) in [
        ("cdr-a.example.org", "node-b-pub"),
        ("CDR-A.example.org", "node-a-pub"),
    ] {
        let document = two_nodes_with(&creating_system(spelled, target));
        assert!(
            matches!(
                refusal(&document),
                LoadError::CreatingSystemIdOfNode { creating_system_id, node }
                    if creating_system_id.as_str() == spelled && node.as_str() == "node-a"
            ),
            "{spelled} to {target}"
        );
    }
}

#[test]
fn a_mapping_of_the_wrong_shape_is_refused() {
    let cases = [
        (
            creating_system("not a uid", "node-a-pub"),
            "not an openEHR uid",
        ),
        (
            "[[creating_system]]\ncreating_system_id = \"legacy-a.example.org\"\n".to_owned(),
            "endpoint",
        ),
        (
            "[[creating_system]]\nendpoint = \"node-a-pub\"\n".to_owned(),
            "creating_system_id",
        ),
        (
            format!(
                "{}node = \"node-a\"\n",
                creating_system(LEGACY, "node-a-pub")
            ),
            "unknown field",
        ),
        (creating_system(LEGACY, "node a"), "endpoint_id"),
    ];
    for (extra, named) in cases {
        let message = shape_refusal(&two_nodes_with(&extra));
        assert!(message.contains(named), "{named}: {message}");
    }
}
