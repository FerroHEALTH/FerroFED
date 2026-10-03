// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Routing a `creating_system_id` and learning one from answers (N21, §12.2,
//! §12.3 step 1): registered first, then learned, a conflict is an incident,
//! and an unknown id is a typed miss.

use ferrofed_registry::creating_system::{CreatingSystemRoute, LearnedMap, Sighting};
use ferrofed_registry::error::{CreatingSystemMiss, ObserveError};
use ferrofed_registry::incident::Incident;
use ferrofed_registry::snapshot::RegistrySnapshot;

use super::{LEGACY, TestResult, endpoint, node, registered, system, unregistered, version};
use crate::fixture::ONE_NODE;

/// An external system no member is and the document does not map.
const EXTERNAL: &str = "ext.example.org";

/// Covers the mapping half of CP-13 (N21, §12.2): a version created under a
/// registered `creating_system_id` and imported into `node-b` routes to its
/// creating node, `node-a`, and the copy teaches the learned map nothing.
// conformance: CP-13
#[test]
fn an_imported_version_resolves_to_its_creating_node_through_a_registered_mapping() -> TestResult {
    let snapshot = registered()?;
    let mut learned = LearnedMap::new();
    let expected = CreatingSystemRoute::Registered {
        node: node("node-a")?,
        endpoint: endpoint("node-a-pub")?,
    };

    let sighting = learned.observe(&snapshot, &version(LEGACY)?, &endpoint("node-b-pub")?)?;
    assert_eq!(
        sighting,
        Sighting::Known(expected.clone()),
        "a copy at node-b"
    );
    assert_eq!(learned, LearnedMap::new(), "nothing is learned from a copy");
    assert_eq!(
        learned.route(&snapshot, &system(LEGACY)?)?,
        expected,
        "the creating node, not the holder it was read from"
    );
    Ok(())
}

#[test]
fn a_members_own_system_id_routes_to_the_member_in_any_case() -> TestResult {
    let snapshot = unregistered()?;
    let learned = LearnedMap::new();
    let route = learned.route(&snapshot, &system("CDR-A.EXAMPLE.ORG")?)?;
    assert_eq!(
        route,
        CreatingSystemRoute::Member {
            node: node("node-a")?
        },
        "master05 compares the system_id without regard to ASCII case"
    );
    assert_eq!(
        route.endpoint(),
        None,
        "any endpoint of the member reaches it"
    );
    Ok(())
}

#[test]
fn an_imported_copy_of_a_members_version_teaches_nothing() -> TestResult {
    // §10.2: an import keeps the uid, so node-b holding cdr-a's version is a copy.
    let snapshot = unregistered()?;
    let mut learned = LearnedMap::new();
    let sighting = learned.observe(
        &snapshot,
        &version("cdr-a.example.org")?,
        &endpoint("node-b-pub")?,
    )?;
    assert_eq!(
        sighting,
        Sighting::Known(CreatingSystemRoute::Member {
            node: node("node-a")?
        })
    );
    assert_eq!(learned, LearnedMap::new(), "no mapping and no incident");
    Ok(())
}

#[test]
fn an_unknown_creating_system_id_is_a_typed_miss() -> TestResult {
    let snapshot = registered()?;
    let learned = LearnedMap::new();
    assert_eq!(
        learned.route(&snapshot, &system(EXTERNAL)?),
        Err(CreatingSystemMiss::Unknown(system(EXTERNAL)?)),
        "never a default endpoint"
    );
    Ok(())
}

// conformance: CP-13
#[test]
fn one_sighting_learns_a_route_to_the_endpoint_it_was_seen_at() -> TestResult {
    let snapshot = unregistered()?;
    let mut learned = LearnedMap::new();
    let expected = CreatingSystemRoute::Learned {
        node: node("node-b")?,
        endpoint: endpoint("node-b-pub")?,
    };
    let sighting = learned.observe(&snapshot, &version(EXTERNAL)?, &endpoint("node-b-pub")?)?;
    assert_eq!(sighting, Sighting::Learned(expected.clone()));
    assert_eq!(learned.route(&snapshot, &system(EXTERNAL)?)?, expected);
    Ok(())
}

#[test]
fn a_second_endpoint_of_the_same_node_confirms_the_learned_mapping() -> TestResult {
    let snapshot = unregistered()?;
    let mut learned = LearnedMap::new();
    let first = CreatingSystemRoute::Learned {
        node: node("node-a")?,
        endpoint: endpoint("node-a-pub")?,
    };
    learned.observe(&snapshot, &version(EXTERNAL)?, &endpoint("node-a-pub")?)?;
    let sighting = learned.observe(&snapshot, &version(EXTERNAL)?, &endpoint("node-a-region")?)?;
    assert_eq!(
        sighting,
        Sighting::Confirmed(first.clone()),
        "one node, one CDR"
    );
    assert_eq!(learned.route(&snapshot, &system(EXTERNAL)?)?, first);
    Ok(())
}

// conformance: CP-13
#[test]
fn a_sighting_at_a_second_node_raises_the_incident_and_withdraws_the_mapping() -> TestResult {
    let snapshot = unregistered()?;
    let mut learned = LearnedMap::new();
    learned.observe(&snapshot, &version(EXTERNAL)?, &endpoint("node-a-pub")?)?;

    let sighting = learned.observe(&snapshot, &version(EXTERNAL)?, &endpoint("node-b-pub")?)?;
    let expected = Incident::LearnedCreatingSystemConflict {
        creating_system_id: system(EXTERNAL)?,
        first: endpoint("node-a-pub")?,
        second: endpoint("node-b-pub")?,
    };
    assert_eq!(sighting, Sighting::Conflict(expected.clone()));
    assert_eq!(expected.kind(), "LearnedCreatingSystemConflict");
    assert_eq!(
        learned.route(&snapshot, &system(EXTERNAL)?),
        Err(CreatingSystemMiss::Conflicted(system(EXTERNAL)?)),
        "a conflicted mapping is not used"
    );

    let again = learned.observe(&snapshot, &version(EXTERNAL)?, &endpoint("node-a-pub")?)?;
    assert_eq!(again, Sighting::Withdrawn, "the incident is raised once");
    Ok(())
}

#[test]
fn a_learned_mapping_never_overrides_a_registered_one() -> TestResult {
    let before = unregistered()?;
    let after = registered()?;
    let mut learned = LearnedMap::new();
    learned.observe(&before, &version(LEGACY)?, &endpoint("node-b-pub")?)?;
    assert_eq!(
        learned.route(&before, &system(LEGACY)?)?,
        CreatingSystemRoute::Learned {
            node: node("node-b")?,
            endpoint: endpoint("node-b-pub")?,
        },
        "unregistered, the id was learned at node-b"
    );
    assert_eq!(
        learned.route(&after, &system(LEGACY)?)?,
        CreatingSystemRoute::Registered {
            node: node("node-a")?,
            endpoint: endpoint("node-a-pub")?,
        },
        "registered, the document answers"
    );
    Ok(())
}

#[test]
fn a_learned_mapping_conflicting_with_a_registered_one_raises_the_incident_and_is_not_used()
-> TestResult {
    let before = unregistered()?;
    let after = registered()?;
    let mut learned = LearnedMap::new();
    learned.observe(&before, &version(LEGACY)?, &endpoint("node-b-pub")?)?;

    let sighting = learned.observe(&after, &version(LEGACY)?, &endpoint("node-b-pub")?)?;
    let expected = Incident::RegisteredCreatingSystemConflict {
        creating_system_id: system(LEGACY)?,
        registered: node("node-a")?,
        learned: endpoint("node-b-pub")?,
    };
    assert_eq!(sighting, Sighting::Conflict(expected.clone()));
    assert_eq!(expected.kind(), "RegisteredCreatingSystemConflict");
    assert_eq!(
        learned.route(&after, &system(LEGACY)?)?.node(),
        &node("node-a")?,
        "the registered mapping answers"
    );
    assert_eq!(
        learned.route(&before, &system(LEGACY)?),
        Err(CreatingSystemMiss::Conflicted(system(LEGACY)?)),
        "the learned mapping is withdrawn, under any snapshot"
    );
    Ok(())
}

#[test]
fn reconciling_a_new_document_raises_the_incident_for_a_contradicted_mapping() -> TestResult {
    let before = unregistered()?;
    let after = registered()?;
    let mut learned = LearnedMap::new();
    learned.observe(&before, &version(LEGACY)?, &endpoint("node-b-pub")?)?;

    let incidents = learned.reconcile(&after);
    assert_eq!(
        incidents,
        [Incident::RegisteredCreatingSystemConflict {
            creating_system_id: system(LEGACY)?,
            registered: node("node-a")?,
            learned: endpoint("node-b-pub")?,
        }]
    );
    assert_eq!(
        learned.route(&before, &system(LEGACY)?),
        Err(CreatingSystemMiss::Conflicted(system(LEGACY)?))
    );
    assert!(learned.reconcile(&after).is_empty(), "raised once");
    Ok(())
}

#[test]
fn reconciling_a_new_document_drops_a_mapping_it_confirms() -> TestResult {
    let before = unregistered()?;
    let after = registered()?;
    let mut learned = LearnedMap::new();
    learned.observe(&before, &version(LEGACY)?, &endpoint("node-a-region")?)?;

    assert!(
        learned.reconcile(&after).is_empty(),
        "same node, no incident"
    );
    assert_eq!(learned, LearnedMap::new(), "the document now answers");
    Ok(())
}

#[test]
fn a_learned_endpoint_that_left_the_snapshot_is_a_miss() -> TestResult {
    let before = unregistered()?;
    let after = RegistrySnapshot::from_toml_str(ONE_NODE)?;
    let mut learned = LearnedMap::new();
    learned.observe(&before, &version(EXTERNAL)?, &endpoint("node-b-pub")?)?;

    assert_eq!(
        learned.route(&after, &system(EXTERNAL)?),
        Err(CreatingSystemMiss::Unknown(system(EXTERNAL)?)),
        "a stale mapping is a miss, never a route to a non-member"
    );
    assert!(learned.reconcile(&after).is_empty());
    assert_eq!(learned, LearnedMap::new(), "the stale mapping is dropped");
    Ok(())
}

#[test]
fn a_sighting_at_an_endpoint_outside_the_snapshot_is_refused() -> TestResult {
    let snapshot = unregistered()?;
    let mut learned = LearnedMap::new();
    assert_eq!(
        learned.observe(&snapshot, &version(EXTERNAL)?, &endpoint("node-z-pub")?),
        Err(ObserveError::UnknownEndpoint(endpoint("node-z-pub")?))
    );
    assert_eq!(learned, LearnedMap::new());
    Ok(())
}
