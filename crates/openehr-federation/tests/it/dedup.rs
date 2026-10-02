// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Version-identity dedup at the Tier (§10.2, §10.3, N15, N36, CP-9, and the
//! visibility half of CP-29): one `OBJECT_VERSION_ID` held at two endpoints
//! keeps the originating copy, every suppression is recorded, two versions of
//! one object both stay, and with `ORDER BY` and `LIMIT` the answer is still
//! the Tier's top `n` of the deduplicated union (§11.6.1, §11.6.2).
#![cfg(feature = "merge")]

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use openehr_federation::dedup::DedupMode;
use openehr_federation::id::EndpointId;
use openehr_federation::merge::{Disagreement, Merged, NodeAnswer, Refused, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_its::rest::generated::query::ResultSetRow;
use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::json;

/// The `system_id` of the node behind endpoint `node-<n>`.
fn system(n: usize) -> String {
    format!("cdr{n}.example.org")
}

/// The version uid of version `tree` of object `object`, created at the
/// system of node `created`.
fn uid(object: usize, created: usize, tree: usize) -> String {
    format!(
        "8849a2f0-1d3c-4e5f-9a7b-{object:012}::{}::{tree}",
        system(created)
    )
}

/// `SELECT c/uid/value, c/name/value`: no `ORDER BY`, no `LIMIT`, the uid in
/// column 0.
fn unordered() -> ResultOrder {
    ResultOrder::new(Vec::new(), Vec::new(), None).with_version_key(0)
}

fn answer(endpoint: &str, node: usize, rows: Vec<ResultSetRow>) -> NodeAnswer {
    NodeAnswer::new(endpoint, rows).with_system_id(system(node))
}

fn accepted(nodes: Vec<NodeAnswer>, order: &ResultOrder) -> Merged {
    let merged = merge(nodes, order);
    assert!(
        merged.refused().is_empty(),
        "no node is refused: {:?}",
        merged.refused()
    );
    merged
}

/// The §10.3 scenario: a composition created at node 1 and imported into
/// node 0, where it keeps its uid, beside a composition of node 0's own.
fn imported() -> Vec<NodeAnswer> {
    vec![
        answer(
            "node-0",
            0,
            vec![
                vec![json!(uid(1, 1, 1)), json!("imported")],
                vec![json!(uid(2, 0, 1)), json!("local")],
            ],
        ),
        answer(
            "node-1",
            1,
            vec![vec![json!(uid(1, 1, 1)), json!("original")]],
        ),
    ]
}

// conformance: CP-9
#[test]
fn by_default_both_copies_of_an_imported_composition_come_back() {
    let merged = accepted(imported(), &ResultOrder::unordered());
    assert_eq!(
        merged.rows().len(),
        3,
        "§10.1, N15: the Tier does not deduplicate by default"
    );
    assert_eq!(merged.suppressed().rows(), 0);
}

/// Covers the visibility half of CP-29 (§10.2, §10.3): the suppressed
/// copies stay visible in `meta.federation.dedup`; its write-routing half is
/// #66.
// conformance: CP-9
#[test]
fn under_version_identity_the_originating_copy_is_kept_and_the_copy_recorded() {
    let merged = accepted(imported(), &unordered());
    assert_eq!(
        merged.rows(),
        [
            vec![json!(uid(2, 0, 1)), json!("local")],
            vec![json!(uid(1, 1, 1)), json!("original")]
        ],
        "§10.2: node-1 created the version, so its copy survives although node-0 sorts first"
    );
    assert_eq!(merged.suppressed().rows(), 1, "§10.2: suppressed_rows");
    assert_eq!(
        merged.suppressed().endpoints(),
        ["node-0"],
        "§10.3: the endpoint whose copy was dropped stays visible"
    );
}

/// BASE `master05-identification_package.adoc` §"Composite Identifiers and
/// Case": a `system_id` identical to the `creating_system_id` apart from case
/// is the same identifier, so its node holds the originating copy.
// conformance: CP-9
#[test]
fn a_system_id_that_differs_from_the_creating_system_id_only_in_case_keeps_its_copy() {
    let shared = uid(1, 1, 1);
    let merged = accepted(
        vec![
            answer("node-0", 0, vec![vec![json!(shared), json!("imported")]]),
            NodeAnswer::new("node-1", vec![vec![json!(shared), json!("original")]])
                .with_system_id(system(1).to_ascii_uppercase()),
        ],
        &unordered(),
    );
    assert_eq!(
        merged.rows(),
        [vec![json!(shared), json!("original")]],
        "§10.2: CDR1.EXAMPLE.ORG is cdr1.example.org, so node-1 created the version"
    );
    assert_eq!(merged.suppressed().endpoints(), ["node-0"]);
}

/// BASE `master05-identification_package.adoc` §"Composite Identifiers and
/// Case": two version ids identical apart from case name one version, and the
/// kept copy keeps the case its node sent (case-preserving).
// conformance: CP-9
#[test]
fn copies_whose_version_ids_differ_only_in_case_are_one_version_kept_as_sent() {
    let original = uid(10, 1, 1).replace("8849a2f0", "8849A2F0");
    let imported = original.to_ascii_lowercase();
    assert_ne!(original, imported, "the fixture's two spellings differ");
    let merged = accepted(
        vec![
            answer("node-0", 0, vec![vec![json!(imported), json!("imported")]]),
            NodeAnswer::new("node-1", vec![vec![json!(original), json!("original")]])
                .with_system_id(system(1).to_ascii_uppercase()),
        ],
        &unordered(),
    );
    assert_eq!(
        merged.rows(),
        [vec![json!(original), json!("original")]],
        "§10.2: one version, the originating copy, its uid text as node-1 sent it"
    );
    assert_eq!(merged.suppressed().rows(), 1, "§10.2: suppressed_rows");
    assert_eq!(merged.suppressed().endpoints(), ["node-0"]);

    let lowest = accepted(
        vec![
            NodeAnswer::new("node-0", vec![vec![json!(imported), json!("a")]]),
            NodeAnswer::new("node-1", vec![vec![json!(original), json!("b")]]),
        ],
        &unordered(),
    );
    assert_eq!(
        lowest.rows(),
        [vec![json!(imported), json!("a")]],
        "with no originating holder the lowest endpoint id keeps its copy, as it sent it"
    );
    assert_eq!(lowest.suppressed().endpoints(), ["node-1"]);
}

// conformance: CP-9
#[test]
fn with_no_originating_holder_the_lowest_endpoint_id_keeps_its_copy() {
    let shared = uid(1, 3, 1);
    let merged = accepted(
        vec![
            answer("node-2", 2, vec![vec![json!(shared), json!("c")]]),
            answer("node-1", 1, vec![vec![json!(shared), json!("b")]]),
            NodeAnswer::new("node-0", vec![vec![json!(shared), json!("a")]]),
        ],
        &unordered(),
    );
    assert_eq!(
        merged.rows(),
        [vec![json!(shared), json!("a")]],
        "§10.2: deterministic, the lowest endpoint id when no holder created the version"
    );
    assert_eq!(merged.suppressed().endpoints(), ["node-1", "node-2"]);
    assert_eq!(merged.suppressed().rows(), 2);
}

// conformance: CP-9
#[test]
fn two_versions_of_one_object_both_survive() {
    let merged = accepted(
        vec![
            answer("node-0", 0, vec![vec![json!(uid(1, 1, 1)), json!("v1")]]),
            answer("node-1", 1, vec![vec![json!(uid(1, 1, 2)), json!("v2")]]),
        ],
        &unordered(),
    );
    assert_eq!(
        merged.rows().len(),
        2,
        "§10.2: different version_tree_ids are different versions"
    );
    assert_eq!(merged.suppressed().rows(), 0);
}

// conformance: CP-9
#[test]
fn a_row_with_no_version_uid_is_never_suppressed() {
    let merged = accepted(
        vec![
            answer("node-0", 0, vec![vec![json!(null), json!("x")]]),
            answer("node-1", 1, vec![vec![json!(null), json!("x")]]),
        ],
        &unordered(),
    );
    assert_eq!(merged.rows().len(), 2, "no version, no duplicate");
    assert_eq!(merged.suppressed().rows(), 0);
}

// conformance: CP-9
#[test]
fn every_row_of_the_dropped_copy_is_suppressed_and_every_row_of_the_kept_one_stays() {
    let shared = uid(1, 1, 1);
    let merged = accepted(
        vec![
            answer(
                "node-0",
                0,
                vec![vec![json!(shared), json!(1)], vec![json!(shared), json!(2)]],
            ),
            answer(
                "node-1",
                1,
                vec![vec![json!(shared), json!(1)], vec![json!(shared), json!(2)]],
            ),
        ],
        &unordered(),
    );
    assert_eq!(
        merged.rows().len(),
        2,
        "one row per observation of the kept copy"
    );
    assert_eq!(merged.suppressed().rows(), 2, "suppressed_rows counts rows");
    assert_eq!(merged.suppressed().endpoints(), ["node-0"]);
}

// conformance: CP-9
#[test]
fn a_version_uid_of_the_wrong_form_is_a_node_defect() {
    for wrong in [
        json!("8849a2f0-1d3c-4e5f-9a7b-000000000001"),
        json!("a::1"),
        json!(7),
        json!({"value": uid(1, 1, 1)}),
    ] {
        let merged = merge(
            vec![
                answer("node-0", 0, vec![vec![wrong.clone(), json!("x")]]),
                answer("node-1", 1, vec![vec![json!(uid(1, 1, 1)), json!("y")]]),
            ],
            &unordered(),
        );
        let refused: Vec<(&str, Disagreement)> = merged
            .refused()
            .iter()
            .map(|refused| (refused.endpoint(), refused.reason()))
            .collect();
        assert_eq!(
            refused,
            [("node-0", Disagreement::VersionId)],
            "{wrong}: never silently a row without a duplicate"
        );
        assert_eq!(merged.rows(), [vec![json!(uid(1, 1, 1)), json!("y")]]);
    }
}

// conformance: CP-9
#[test]
fn a_row_too_short_for_its_version_column_is_refused() {
    let merged = merge(vec![answer("node-0", 0, vec![Vec::new()])], &unordered());
    assert_eq!(merged.refused().len(), 1);
    assert_eq!(
        merged.refused().first().map(Refused::reason),
        Some(Disagreement::ShortRow)
    );
}

/// Covers the visibility half of CP-29 (§10.2, §10.3): the suppressed
/// copies stay visible in `meta.federation.dedup`; its write-routing half is
/// #66.
// conformance: CP-9
#[test]
fn the_record_carries_the_mode_and_under_version_identity_what_was_suppressed() {
    let none = Merged::default().suppressed().record(DedupMode::None);
    let none = none.expect("a record");
    assert_eq!(none.mode.as_deref(), Some("none"));
    assert_eq!(none.suppressed_rows, None);
    assert_eq!(none.suppressed_endpoints, None);

    let nothing = Merged::default()
        .suppressed()
        .record(DedupMode::VersionIdentity)
        .expect("a record");
    assert_eq!(nothing.mode.as_deref(), Some("version-identity"));
    assert_eq!(nothing.suppressed_rows, Some(0));
    assert_eq!(nothing.suppressed_endpoints, None);

    let merged = accepted(imported(), &unordered());
    let record = merged
        .suppressed()
        .record(DedupMode::VersionIdentity)
        .expect("a record");
    assert_eq!(record.suppressed_rows, Some(1));
    let named: Vec<&str> = record
        .suppressed_endpoints
        .iter()
        .flatten()
        .map(EndpointId::as_str)
        .collect();
    assert_eq!(named, ["node-0"], "§10.3, N36");
}

// conformance: CP-9 CP-8
#[test]
fn dedup_runs_before_distinct_so_the_distinct_row_is_the_originating_copy() {
    let shared = uid(1, 1, 1);
    let order = ResultOrder::new(Vec::new(), Vec::new(), None)
        .with_distinct(vec![0])
        .with_version_key(0);
    let merged = accepted(
        vec![
            answer("node-0", 0, vec![vec![json!(shared), json!("node-0")]]),
            answer("node-1", 1, vec![vec![json!(shared), json!("node-1")]]),
        ],
        &order,
    );
    assert_eq!(
        merged.rows(),
        [vec![json!(shared), json!("node-1")]],
        "DISTINCT alone keeps node-0's copy, first by endpoint id; dedup keeps the originating one"
    );
    assert_eq!(merged.suppressed().rows(), 1);
}

// conformance: CP-9 CP-32
#[test]
fn a_suppressed_copy_does_not_take_a_slot_of_the_limit() {
    // node-0 holds an import of node-1's version `y` and its own `r`, tied on
    // the key; under LIMIT 1 it returns `y`, the lower uid.
    let (y, r) = (uid(1, 1, 1), uid(2, 0, 1));
    let order = ResultOrder::new(
        vec![SortKey::new(0, Direction::Ascending)],
        vec![1],
        Some(1),
    )
    .with_version_key(1);
    let merged = accepted(
        vec![
            answer("node-0", 0, vec![vec![json!(5), json!(y), json!("node-0")]]),
            answer("node-1", 1, vec![vec![json!(5), json!(y), json!("node-1")]]),
        ],
        &order,
    );
    assert_eq!(
        merged.rows(),
        [vec![json!(5), json!(y), json!("node-1")]],
        "§11.6.1: the global top 1 of the deduplicated union is node-1's copy of y, not r"
    );
    assert!(r > y, "the fixture's uids order y first");
}

/// One generated version: its object, the node that created it, its tree
/// number, and its `ORDER BY` key (`None` is null).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    object: usize,
    created: usize,
    tree: usize,
    key: Option<i32>,
}

impl Version {
    fn uid(self) -> String {
        uid(self.object, self.created, self.tree)
    }

    fn row(self, endpoint: &str) -> ResultSetRow {
        vec![
            self.key.map_or(json!(null), |key| json!(key)),
            json!(self.uid()),
            json!(endpoint),
        ]
    }
}

/// The versions of a federation: up to six, over three objects, so two
/// versions of one object occur, each created at one of five systems, of
/// which only nodes 0 to 3 exist.
fn versions() -> impl Strategy<Value = Vec<Version>> {
    vec(
        (
            0_usize..3,
            0_usize..5,
            1_usize..3,
            proptest::option::weighted(0.8, 0_i32..3),
        ),
        1..7,
    )
    .prop_map(|drawn| {
        let mut seen = BTreeSet::new();
        drawn
            .into_iter()
            .filter(|(object, created, tree, _)| seen.insert((*object, *created, *tree)))
            .map(|(object, created, tree, key)| Version {
                object,
                created,
                tree,
                key,
            })
            .collect()
    })
}

/// Up to four nodes, each holding a subset of the versions, so that one
/// version is often held at several.
fn federation() -> impl Strategy<Value = (Vec<Version>, Vec<Vec<usize>>)> {
    versions().prop_flat_map(|versions| {
        let count = versions.len();
        (
            Just(versions),
            vec(proptest::collection::btree_set(0..count, 0..=count), 1..5).prop_map(|held| {
                held.into_iter()
                    .map(|set| set.into_iter().collect())
                    .collect()
            }),
        )
    })
}

fn endpoint(index: usize) -> String {
    format!("node-{index}")
}

fn key_order(a: Option<i32>, b: Option<i32>, direction: Direction) -> Ordering {
    let ascending = match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    match direction {
        Direction::Ascending => ascending,
        Direction::Descending => ascending.reverse(),
    }
}

/// The order a node is sent: the key, then the uid ascending; with no key,
/// the uid alone, as the rewrite orders a query with a `LIMIT` and no
/// `ORDER BY`.
fn dispatched(a: &Version, b: &Version, direction: Option<Direction>) -> Ordering {
    direction
        .map_or(Ordering::Equal, |direction| {
            key_order(a.key, b.key, direction)
        })
        .then_with(|| a.uid().cmp(&b.uid()))
}

/// What a node that orders as the Tier does answers: its versions in the
/// dispatched order, cut at the window it was sent.
fn answered(
    versions: &[Version],
    held: &[usize],
    direction: Option<Direction>,
    window: Option<u64>,
) -> Vec<Version> {
    let mut rows: Vec<Version> = held
        .iter()
        .filter_map(|index| versions.get(*index).copied())
        .collect();
    rows.sort_by(|a, b| dispatched(a, b, direction));
    if let Some(window) = window {
        rows.truncate(usize::try_from(window).expect("a small window"));
    }
    rows
}

/// The keeper of a version among `holders`, node indices: the lowest one that
/// created it, else the lowest.
fn keeper(version: &Version, holders: &BTreeSet<usize>) -> Option<usize> {
    holders
        .iter()
        .find(|node| **node == version.created)
        .or_else(|| holders.first())
        .copied()
}

/// Dedup over `rows`, `(node, version)` pairs: the rows of every version's
/// keeper, and the suppressed count and endpoints.
fn deduplicated(rows: &[(usize, Version)]) -> (Vec<(usize, Version)>, u64, Vec<String>) {
    let mut holders: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for (node, version) in rows {
        holders.entry(version.uid()).or_default().insert(*node);
    }
    let mut kept = Vec::new();
    let mut suppressed = 0;
    let mut dropped = BTreeSet::new();
    for (node, version) in rows {
        if keeper(version, &holders[&version.uid()]) == Some(*node) {
            kept.push((*node, *version));
        } else {
            suppressed += 1;
            dropped.insert(endpoint(*node));
        }
    }
    (kept, suppressed, dropped.into_iter().collect())
}

/// The page `[k, k + n)` of the deduplicated union of every version every
/// node holds, uncut: ordered by the key, the uid, then the endpoint id.
fn oracle(
    versions: &[Version],
    held: &[Vec<usize>],
    direction: Option<Direction>,
    k: u64,
    n: Option<u64>,
) -> Vec<ResultSetRow> {
    let all: Vec<(usize, Version)> = held
        .iter()
        .enumerate()
        .flat_map(|(node, indices)| {
            indices
                .iter()
                .filter_map(move |index| versions.get(*index).map(|version| (node, *version)))
        })
        .collect();
    let (mut kept, _, _) = deduplicated(&all);
    kept.sort_by(|(na, a), (nb, b)| {
        dispatched(a, b, direction).then_with(|| endpoint(*na).cmp(&endpoint(*nb)))
    });
    let skip = usize::try_from(k).expect("a small offset");
    let take = n.map_or(usize::MAX, |n| usize::try_from(n).expect("a small limit"));
    kept.into_iter()
        .skip(skip)
        .take(take)
        .map(|(node, version)| version.row(&endpoint(node)))
        .collect()
}

fn order(direction: Option<Direction>, window: Option<u64>, k: u64, distinct: bool) -> ResultOrder {
    let keys = match direction {
        Some(direction) => vec![SortKey::new(0, direction)],
        None => vec![SortKey::new(1, Direction::Ascending)],
    };
    let order = ResultOrder::new(keys, vec![1], window)
        .with_version_key(1)
        .with_offset(k);
    if distinct {
        order.with_distinct(vec![0, 1])
    } else {
        order
    }
}

fn direction() -> impl Strategy<Value = Option<Direction>> {
    prop_oneof![
        Just(Some(Direction::Ascending)),
        Just(Some(Direction::Descending)),
        Just(None)
    ]
}

proptest! {
    // conformance: CP-9 CP-32
    #[test]
    fn dedup_with_order_by_limit_and_a_bounded_page_is_the_page_of_the_deduplicated_union(
        (versions, held) in federation(),
        direction in direction(),
        k in 0_u64..4,
        n in proptest::option::of(0_u64..4),
        distinct in any::<bool>(),
    ) {
        let (k, window) = match n {
            Some(n) => (k, Some(k + n)),
            None => (0, None),
        };
        let answers: Vec<NodeAnswer> = held
            .iter()
            .enumerate()
            .map(|(node, indices)| {
                let rows = answered(&versions, indices, direction, window)
                    .into_iter()
                    .map(|version| version.row(&endpoint(node)))
                    .collect();
                NodeAnswer::new(endpoint(node), rows).with_system_id(system(node))
            })
            .collect();
        let merged = merge(answers, &order(direction, window, k, distinct));
        let expected = oracle(&versions, &held, direction, k, n);
        prop_assert!(merged.refused().is_empty(), "a conforming node is never refused: {:?}", merged.refused());
        prop_assert_eq!(
            merged.rows(),
            expected.as_slice(),
            "§10.2, §11.6.1, §11.6.2: per-node k + n rows give the page of the deduplicated union"
        );
    }

    /// Covers the visibility half of CP-29 (§10.2, §10.3): the suppressed
    /// copies stay visible in `meta.federation.dedup`; its write-routing half is
    /// #66.
    // conformance: CP-9
    #[test]
    fn dedup_keeps_one_endpoint_per_version_and_records_exactly_what_it_dropped(
        (versions, held) in federation(),
    ) {
        let visible: Vec<(usize, Version)> = held
            .iter()
            .enumerate()
            .flat_map(|(node, indices)| {
                answered(&versions, indices, Some(Direction::Ascending), None)
                    .into_iter()
                    .map(move |version| (node, version))
            })
            .collect();
        let answers: Vec<NodeAnswer> = held
            .iter()
            .enumerate()
            .map(|(node, indices)| {
                let rows = answered(&versions, indices, Some(Direction::Ascending), None)
                    .into_iter()
                    .map(|version| version.row(&endpoint(node)))
                    .collect();
                NodeAnswer::new(endpoint(node), rows).with_system_id(system(node))
            })
            .collect();
        let merged = merge(answers, &order(Some(Direction::Ascending), None, 0, false));
        let (kept, suppressed, dropped) = deduplicated(&visible);
        prop_assert_eq!(merged.rows().len(), kept.len());
        prop_assert_eq!(merged.suppressed().rows(), suppressed, "suppressed_rows counts exactly");
        prop_assert_eq!(merged.suppressed().endpoints(), dropped.as_slice());
        let uids: BTreeSet<String> = merged
            .rows()
            .iter()
            .filter_map(|row| row.get(1).and_then(|uid| uid.as_str()).map(str::to_owned))
            .collect();
        prop_assert_eq!(uids.len(), merged.rows().len(), "at most one copy of every version id");
        let held_uids: BTreeSet<String> = visible.iter().map(|(_, version)| version.uid()).collect();
        prop_assert_eq!(uids, held_uids, "every version survives once, two versions of one object included");
    }
}
