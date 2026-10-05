// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Over any sequence of sightings, the learned map holds to its rules
//! (N21, §12.2): the document always answers first, an id seen at one node
//! routes to its first endpoint, an id seen at two nodes raises one incident
//! and is never routed on, and an id never seen is a miss.

use std::collections::BTreeMap;

use ferrofed_registry::creating_system::{CreatingSystemRoute, LearnedMap, Sighting};
use ferrofed_registry::error::CreatingSystemMiss;
use ferrofed_registry::id::SystemId;
use proptest::prelude::*;

use super::{LEGACY, endpoint, registered, system, version};

/// The ids sighted: a member's own, the registered one, and three no
/// document names.
const SYSTEMS: [&str; 5] = [
    "cdr-a.example.org",
    LEGACY,
    "ext-1.example.org",
    "ext-2.example.org",
    "ext-3.example.org",
];

/// The endpoints sighted at, with their nodes.
const ENDPOINTS: [(&str, &str); 3] = [
    ("node-a-pub", "node-a"),
    ("node-a-region", "node-a"),
    ("node-b-pub", "node-b"),
];

proptest! {
    #[test]
    fn the_learned_map_holds_to_its_rules(
        sightings in prop::collection::vec((0..SYSTEMS.len(), 0..ENDPOINTS.len()), 0..40),
    ) {
        let snapshot = registered().map_err(fail)?;
        let mut learned = LearnedMap::new();
        let mut seen: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
        let mut incidents: BTreeMap<&str, usize> = BTreeMap::new();
        for (s, e) in sightings {
            let (Some(id), Some(&(at, at_node))) = (SYSTEMS.get(s), ENDPOINTS.get(e)) else {
                return Err(TestCaseError::fail("an index outside the pools"));
            };
            let uid = version(id).map_err(fail)?;
            let at_id = endpoint(at).map_err(fail)?;
            let sighting = learned
                .observe(&snapshot, &uid, &at_id)
                .map_err(fail)?;
            if let Sighting::Conflict(incident) = &sighting {
                *incidents.entry(id).or_default() += 1;
                prop_assert_eq!(
                    incident.creating_system_id().map(SystemId::as_str),
                    Some(*id)
                );
            }
            if snapshot.registered_route(&system(id).map_err(fail)?).is_some() {
                prop_assert!(matches!(sighting, Sighting::Known(_)), "{id}: {sighting:?}");
            } else {
                seen.entry(id).or_default().push((at, at_node));
            }
        }
        for id in SYSTEMS {
            let key = system(id).map_err(fail)?;
            let route = learned.route(&snapshot, &key);
            if let Some(registered_route) = snapshot.registered_route(&key) {
                prop_assert_eq!(route, Ok(registered_route), "the document answers {}", id);
                prop_assert_eq!(incidents.get(id), None);
                continue;
            }
            match seen.get(id).map(Vec::as_slice) {
                None | Some([]) => {
                    prop_assert_eq!(route, Err(CreatingSystemMiss::Unknown(key)));
                }
                Some(all @ [(first, first_node), ..]) => {
                    if all.iter().all(|(_, n)| n == first_node) {
                        let expected = CreatingSystemRoute::Learned {
                            node: first_node.parse().map_err(fail)?,
                            endpoint: endpoint(first).map_err(fail)?,
                        };
                        prop_assert_eq!(route, Ok(expected));
                        prop_assert_eq!(incidents.get(id), None);
                    } else {
                        prop_assert_eq!(route, Err(CreatingSystemMiss::Conflicted(key)));
                        prop_assert_eq!(incidents.get(id), Some(&1), "one incident for {}", id);
                    }
                }
            }
        }
    }
}

/// A setup failure inside a property, as the case's failure.
fn fail(error: impl std::fmt::Display) -> TestCaseError {
    TestCaseError::fail(error.to_string())
}
