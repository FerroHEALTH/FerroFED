// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated merge (feature `merge`): `ORDER BY` with `LIMIT` re-applied
//! at the Tier with a deterministic tie-break (§11.6.1, N13, N39), the page
//! of `LIMIT n OFFSET k` sliced from `k + n` rows per node (§11.6.2), and the
//! check of a node's visible order (FerroFED's own, within §11.6.1).
#![cfg(feature = "merge")]

use std::cmp::Ordering;

use openehr_federation::merge::{Disagreement, NodeAnswer, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_its::rest::generated::query::ResultSetRow;
use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::json;

/// One generated row: the key (`None` is null) and its node-unique uid.
#[derive(Debug, Clone)]
struct Row {
    key: Option<i64>,
    uid: String,
}

impl Row {
    fn cells(&self) -> ResultSetRow {
        vec![
            self.key.map_or(json!(null), |key| json!(key)),
            json!(self.uid),
        ]
    }
}

/// The order the Tier defines for the generated keys: null greatest.
fn key_order(a: Option<i64>, b: Option<i64>, direction: Direction) -> Ordering {
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

fn endpoint(index: usize) -> String {
    format!("node-{index}")
}

/// What a node that orders as the Tier does answers to the pushed query:
/// its rows by the key and then the uid, cut at `n` (§11.6.1).
fn conforming(mut rows: Vec<Row>, direction: Direction, limit: Option<u64>) -> Vec<Row> {
    rows.sort_by(|a, b| key_order(a.key, b.key, direction).then_with(|| a.uid.cmp(&b.uid)));
    if let Some(limit) = limit {
        rows.truncate(usize::try_from(limit).expect("a small limit"));
    }
    rows
}

/// The federated answer computed from every row of every node, uncut.
fn oracle(nodes: &[Vec<Row>], direction: Direction, limit: Option<u64>) -> Vec<ResultSetRow> {
    let mut all: Vec<(String, Row)> = nodes
        .iter()
        .enumerate()
        .flat_map(|(index, rows)| rows.iter().map(move |row| (endpoint(index), row.clone())))
        .collect();
    all.sort_by(|(ea, a), (eb, b)| {
        key_order(a.key, b.key, direction)
            .then_with(|| ea.cmp(eb))
            .then_with(|| a.uid.cmp(&b.uid))
    });
    let mut rows: Vec<ResultSetRow> = all.into_iter().map(|(_, row)| row.cells()).collect();
    if let Some(limit) = limit {
        rows.truncate(usize::try_from(limit).expect("a small limit"));
    }
    rows
}

fn order(direction: Direction, limit: Option<u64>) -> ResultOrder {
    ResultOrder::new(vec![SortKey::new(0, direction)], vec![1], limit)
}

fn answers(nodes: &[Vec<Row>]) -> Vec<NodeAnswer> {
    nodes
        .iter()
        .enumerate()
        .map(|(index, rows)| {
            NodeAnswer::new(endpoint(index), rows.iter().map(Row::cells).collect())
        })
        .collect()
}

/// Up to four nodes of up to eight rows, keys drawn from a small range so
/// that ties within and across nodes are common.
fn nodes() -> impl Strategy<Value = Vec<Vec<Row>>> {
    vec(vec(proptest::option::weighted(0.8, -3_i64..3), 0..8), 1..5).prop_map(|nodes| {
        nodes
            .into_iter()
            .enumerate()
            .map(|(node, keys)| {
                keys.into_iter()
                    .enumerate()
                    .map(|(index, key)| Row {
                        key,
                        uid: format!("{}::{index:02}", endpoint(node)),
                    })
                    .collect()
            })
            .collect()
    })
}

fn direction() -> impl Strategy<Value = Direction> {
    prop_oneof![Just(Direction::Ascending), Just(Direction::Descending)]
}

proptest! {
    // conformance: CP-8 CP-32
    #[test]
    fn the_pushed_down_merge_is_the_order_and_limit_of_the_union(
        nodes in nodes(),
        direction in direction(),
        limit in proptest::option::of(0_u64..7),
    ) {
        let dispatched: Vec<Vec<Row>> = nodes
            .iter()
            .map(|rows| conforming(rows.clone(), direction, limit))
            .collect();
        let merged = merge(answers(&dispatched), &order(direction, limit));
        prop_assert!(merged.refused().is_empty(), "a conforming node is never refused: {:?}", merged.refused());
        let expected = oracle(&nodes, direction, limit);
        prop_assert_eq!(merged.rows(), expected.as_slice(), "N39, §11.6.1");
    }

    // conformance: CP-32
    #[test]
    fn the_merge_does_not_depend_on_the_order_the_nodes_answered_in(
        (nodes, arrival) in nodes().prop_flat_map(|nodes| {
            let indices: Vec<usize> = (0..nodes.len()).collect();
            (Just(nodes), Just(indices).prop_shuffle())
        }),
        direction in direction(),
        limit in proptest::option::of(0_u64..7),
    ) {
        let dispatched: Vec<Vec<Row>> = nodes
            .iter()
            .map(|rows| conforming(rows.clone(), direction, limit))
            .collect();
        let forward = merge(answers(&dispatched), &order(direction, limit));
        let answered = answers(&dispatched);
        let reordered: Vec<NodeAnswer> = arrival.iter().map(|index| answered[*index].clone()).collect();
        prop_assert_eq!(forward, merge(reordered, &order(direction, limit)), "§11.6.1: deterministic");
    }

    // conformance: CP-32
    #[test]
    fn an_uncut_node_may_answer_in_any_order(
        rows in vec(proptest::option::weighted(0.8, -3_i64..3), 0..6)
            .prop_map(|keys| keys.into_iter().enumerate().map(|(index, key)| Row { key, uid: format!("u{index:02}") }).collect::<Vec<_>>())
            .prop_flat_map(|rows| (Just(rows.clone()), Just(rows).prop_shuffle())),
        direction in direction(),
    ) {
        let (sorted, shuffled) = rows;
        let limit = Some(6);
        let expected = merge(vec![NodeAnswer::new("node-0", sorted.iter().map(Row::cells).collect())], &order(direction, limit));
        let actual = merge(vec![NodeAnswer::new("node-0", shuffled.iter().map(Row::cells).collect())], &order(direction, limit));
        prop_assert!(actual.refused().is_empty(), "an uncut node holds all its rows, so the Tier orders them");
        prop_assert_eq!(expected, actual);
    }

    // conformance: CP-32
    #[test]
    fn a_cut_node_out_of_the_federation_order_is_refused(
        keys in vec(proptest::option::weighted(0.8, -3_i64..3), 2..9),
        direction in direction(),
        swap in any::<proptest::sample::Index>(),
    ) {
        let rows: Vec<Row> = keys
            .into_iter()
            .enumerate()
            .map(|(index, key)| Row { key, uid: format!("u{index:02}") })
            .collect();
        let mut answered = conforming(rows.clone(), direction, None);
        let limit = u64::try_from(answered.len()).expect("a small count");
        let at = swap.index(answered.len() - 1);
        answered.swap(at, at + 1);
        let merged = merge(vec![NodeAnswer::new("node-0", answered.iter().map(Row::cells).collect())], &order(direction, Some(limit)));
        prop_assert!(merged.rows().is_empty(), "none of a refused node's rows is merged");
        prop_assert_eq!(merged.refused().len(), 1);
        prop_assert_eq!(merged.refused()[0].reason(), Disagreement::Order, "§11.6.1, N39");
    }
}

/// The page `[k, k + n)` of the federated answer computed from every row of
/// every node.
fn oracle_page(nodes: &[Vec<Row>], direction: Direction, k: u64, n: u64) -> Vec<ResultSetRow> {
    let skip = usize::try_from(k).expect("a small offset");
    let take = usize::try_from(n).expect("a small limit");
    oracle(nodes, direction, None)
        .into_iter()
        .skip(skip)
        .take(take)
        .collect()
}

/// The bounded `OFFSET` merge: every node sent `LIMIT k + n` and the Tier
/// keeping rows `[k, k + n)` (§11.6.2).
fn bounded_page(nodes: &[Vec<Row>], direction: Direction, k: u64, n: u64) -> Vec<ResultSetRow> {
    let window = k + n;
    let dispatched: Vec<Vec<Row>> = nodes
        .iter()
        .map(|rows| conforming(rows.clone(), direction, Some(window)))
        .collect();
    let merged = merge(
        answers(&dispatched),
        &order(direction, Some(window)).with_offset(k),
    );
    assert!(
        merged.refused().is_empty(),
        "a conforming node is never refused: {:?}",
        merged.refused()
    );
    merged.rows().to_vec()
}

proptest! {
    // conformance: CP-32
    #[test]
    fn the_bounded_offset_page_is_the_page_of_the_full_merged_set(
        nodes in nodes(),
        direction in direction(),
        k in 0_u64..9,
        n in 0_u64..7,
    ) {
        prop_assert_eq!(
            bounded_page(&nodes, direction, k, n),
            oracle_page(&nodes, direction, k, n),
            "§11.6.2, N39: k + n rows per node, merged, ordered and sliced"
        );
    }
}

/// Node splits for the table-driven page check: ties at both slice edges,
/// across and within nodes, and nodes holding fewer than `k + n` rows.
fn splits() -> Vec<(&'static str, Vec<Vec<Row>>)> {
    let rows = |node: usize, keys: &[Option<i64>]| -> Vec<Row> {
        keys.iter()
            .enumerate()
            .map(|(index, key)| Row {
                key: *key,
                uid: format!("{}::{index:02}", endpoint(node)),
            })
            .collect()
    };
    vec![
        (
            "every row on one node",
            vec![rows(
                0,
                &[Some(1), Some(2), Some(2), Some(3), Some(4), Some(5)],
            )],
        ),
        (
            "ties straddling both edges across nodes",
            vec![
                rows(0, &[Some(2), Some(2), Some(4)]),
                rows(1, &[Some(2), Some(4), Some(4)]),
                rows(2, &[Some(1), Some(2), Some(4), Some(9)]),
            ],
        ),
        (
            "nodes with fewer rows than the window",
            vec![rows(0, &[Some(7)]), rows(1, &[]), rows(2, &[Some(3), None])],
        ),
        (
            "nulls at the edge",
            vec![rows(0, &[None, None, Some(1)]), rows(1, &[None, Some(0)])],
        ),
        (
            "one node holding the whole page past the others",
            vec![
                rows(0, &[Some(0), Some(0), Some(0), Some(0), Some(0)]),
                rows(1, &[Some(5), Some(6)]),
            ],
        ),
    ]
}

// conformance: CP-32
#[test]
fn the_bounded_offset_page_matches_the_full_merge_for_every_split() {
    for (split, nodes) in splits() {
        for direction in [Direction::Ascending, Direction::Descending] {
            for k in 0..8 {
                for n in 0..5 {
                    assert_eq!(
                        bounded_page(&nodes, direction, k, n),
                        oracle_page(&nodes, direction, k, n),
                        "{split}, {direction:?}, OFFSET {k} LIMIT {n} (§11.6.2)"
                    );
                }
            }
        }
    }
}

// conformance: CP-32
#[test]
fn a_node_whose_k_plus_n_rows_disagree_with_the_federation_order_is_refused() {
    let merged = merge(
        vec![
            NodeAnswer::new(
                "node-a",
                vec![
                    vec![json!(1), json!("a1")],
                    vec![json!(3), json!("a2")],
                    vec![json!(2), json!("a3")],
                ],
            ),
            NodeAnswer::new("node-b", vec![vec![json!(0), json!("b1")]]),
        ],
        &order(Direction::Ascending, Some(3)).with_offset(1),
    );
    assert_eq!(merged.refused().len(), 1);
    assert_eq!(merged.refused()[0].endpoint(), "node-a");
    assert_eq!(
        merged.refused()[0].reason(),
        Disagreement::Order,
        "the order check runs on the k + n rows the node was sent (§11.6.2)"
    );
    assert!(merged.rows().is_empty(), "OFFSET 1 skips node-b's one row");
}

#[test]
fn an_offset_past_every_row_leaves_an_empty_page() {
    let merged = merge(
        vec![NodeAnswer::new("node-a", vec![vec![json!(1), json!("a1")]])],
        &order(Direction::Ascending, Some(12)).with_offset(10),
    );
    assert!(merged.refused().is_empty());
    assert!(
        merged.rows().is_empty(),
        "AQL: OFFSET skips rows that exist"
    );
}

#[test]
fn a_node_tied_at_its_cut_is_accepted() {
    let order = ResultOrder::new(
        vec![SortKey::new(0, Direction::Ascending)],
        Vec::new(),
        Some(2),
    );
    let merged = merge(
        vec![NodeAnswer::new(
            "node-0",
            vec![vec![json!(1), json!("y")], vec![json!(1), json!("x")]],
        )],
        &order,
    );
    assert!(
        merged.refused().is_empty(),
        "§11.6.1: rows tied on every key the node was sent are in its order"
    );
    assert_eq!(
        merged.rows(),
        [vec![json!(1), json!("x")], vec![json!(1), json!("y")]],
        "the Tier breaks the tie on the cells after endpoint_id"
    );
}

#[test]
fn a_node_answering_past_the_dispatched_limit_is_refused() {
    let rows = (0..3)
        .map(|index| vec![json!(index), json!(format!("u{index}"))])
        .collect();
    let merged = merge(
        vec![NodeAnswer::new("node-0", rows)],
        &order(Direction::Ascending, Some(2)),
    );
    assert_eq!(merged.refused()[0].reason(), Disagreement::PastTheLimit);
}

#[test]
fn a_row_without_the_ordered_column_is_refused() {
    let merged = merge(
        vec![NodeAnswer::new("node-0", vec![vec![json!(1)]])],
        &order(Direction::Ascending, Some(2)),
    );
    assert_eq!(merged.refused()[0].reason(), Disagreement::ShortRow);
}

#[test]
fn a_refused_node_leaves_the_other_nodes_merged() {
    let merged = merge(
        vec![
            NodeAnswer::new(
                "node-b",
                vec![vec![json!(2), json!("b1")], vec![json!(1), json!("b2")]],
            ),
            NodeAnswer::new("node-a", vec![vec![json!(3), json!("a1")]]),
        ],
        &order(Direction::Ascending, Some(2)),
    );
    assert_eq!(merged.refused()[0].endpoint(), "node-b");
    assert_eq!(
        merged.rows(),
        [vec![json!(3), json!("a1")]],
        "the gateway decides completeness; the merge only names the refused node"
    );
}

#[test]
fn rows_with_no_order_are_the_nodes_rows_in_endpoint_order_cut_at_the_limit() {
    let merged = merge(
        vec![
            NodeAnswer::new("node-b", vec![vec![json!("b1")], vec![json!("b2")]]),
            NodeAnswer::new("node-a", vec![vec![json!("a1")], vec![json!("a2")]]),
        ],
        &ResultOrder::new(Vec::new(), Vec::new(), Some(3)),
    );
    assert_eq!(
        merged.rows(),
        [vec![json!("a1")], vec![json!("a2")], vec![json!("b1")]],
        "N9: LIMIT is re-applied at the Tier"
    );
}

#[test]
fn equal_keys_across_nodes_are_tie_broken_on_the_endpoint_then_the_uid() {
    let merged = merge(
        vec![
            NodeAnswer::new("node-b", vec![vec![json!(1), json!("a")]]),
            NodeAnswer::new(
                "node-a",
                vec![vec![json!(1), json!("z")], vec![json!(1), json!("y")]],
            ),
        ],
        &order(Direction::Ascending, None),
    );
    assert_eq!(
        merged.rows(),
        [
            vec![json!(1), json!("y")],
            vec![json!(1), json!("z")],
            vec![json!(1), json!("a")]
        ],
        "§11.6.1: endpoint_id, then uid"
    );
}

#[test]
fn quantities_order_by_magnitude_never_by_their_text() {
    let quantity =
        |magnitude: f64| json!({"_type": "DV_QUANTITY", "magnitude": magnitude, "units": "kg"});
    let merged = merge(
        vec![
            NodeAnswer::new("node-a", vec![vec![quantity(9.5), json!("a")]]),
            NodeAnswer::new("node-b", vec![vec![quantity(10.0), json!("b")]]),
        ],
        &order(Direction::Ascending, None),
    );
    assert_eq!(
        merged.rows(),
        [
            vec![quantity(9.5), json!("a")],
            vec![quantity(10.0), json!("b")]
        ],
        "openEHR RM DV_ORDERED: 9.5 kg is less than 10 kg, whatever \"10\" < \"9.5\" says"
    );
}

#[test]
fn zoned_date_times_order_by_instant() {
    let merged = merge(
        vec![
            NodeAnswer::new(
                "node-a",
                vec![vec![json!("2026-01-01T09:00:00Z"), json!("a")]],
            ),
            NodeAnswer::new(
                "node-b",
                vec![vec![json!("2026-01-01T10:00:00+02:00"), json!("b")]],
            ),
        ],
        &order(Direction::Ascending, None),
    );
    assert_eq!(
        merged.rows()[0][1],
        json!("b"),
        "10:00+02:00 is 08:00Z, the earlier instant"
    );
}
