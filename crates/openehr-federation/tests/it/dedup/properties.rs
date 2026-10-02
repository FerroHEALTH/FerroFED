// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dedup properties (§10.2, §10.3, §11.6.1, §11.6.2, CP-9, CP-32): over
//! generated federations whose nodes spell a uid in any case, the merged page
//! is the page of the deduplicated union, and the record names exactly what
//! was suppressed.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use openehr_federation::merge::{NodeAnswer, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_its::rest::generated::query::ResultSetRow;
use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::json;

use super::{spelled, system, uid};

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

    /// The row of this version at `endpoint`, its uid in the spelling `case`
    /// of [`spelled`].
    fn row(self, endpoint: &str, case: u8) -> ResultSetRow {
        vec![
            self.key.map_or(json!(null), |key| json!(key)),
            json!(spelled(&self.uid(), case)),
            json!(endpoint),
        ]
    }
}

/// The spelling node `node` gives the uid of `version`, one of `versions`:
/// `cases[node][index]`, or as written past the drawn cases.
fn case(cases: &[Vec<u8>], versions: &[Version], node: usize, version: Version) -> u8 {
    versions
        .iter()
        .position(|candidate| *candidate == version)
        .and_then(|index| cases.get(node)?.get(index).copied())
        .unwrap_or(0)
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
/// `ORDER BY`. [`Version::uid`] is all in lower case, so it orders every
/// spelling of [`spelled`] without regard to case.
fn dispatched(a: &Version, b: &Version, direction: Option<Direction>) -> Ordering {
    direction
        .map_or(Ordering::Equal, |direction| {
            key_order(a.key, b.key, direction)
        })
        .then_with(|| a.uid().cmp(&b.uid()))
}

/// What a node that orders as the Tier does answers: its versions in the
/// dispatched order, uids compared without regard to case, cut at the window
/// it was sent.
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
/// node holds, uncut: ordered by the key, the uid without regard to case, then
/// the endpoint id, every row spelled as its node spells it.
fn oracle(
    versions: &[Version],
    held: &[Vec<usize>],
    cases: &[Vec<u8>],
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
        .map(|(node, version)| version.row(&endpoint(node), case(cases, versions, node, version)))
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
    /// Every node spells each uid it holds in a case of [`spelled`], so two
    /// copies of one version can differ in case.
    // conformance: CP-9 CP-32
    #[test]
    fn dedup_with_order_by_limit_and_a_bounded_page_is_the_page_of_the_deduplicated_union(
        (versions, held) in federation(),
        direction in direction(),
        k in 0_u64..4,
        n in proptest::option::of(0_u64..4),
        distinct in any::<bool>(),
        cases in vec(vec(0_u8..3, 6), 4),
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
                    .map(|version| {
                        version.row(&endpoint(node), case(&cases, &versions, node, version))
                    })
                    .collect();
                NodeAnswer::new(endpoint(node), rows).with_system_id(system(node))
            })
            .collect();
        let merged = merge(answers, &order(direction, window, k, distinct));
        let expected = oracle(&versions, &held, &cases, direction, k, n);
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
                    .map(|version| version.row(&endpoint(node), 0))
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
