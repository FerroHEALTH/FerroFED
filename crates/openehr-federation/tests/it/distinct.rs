// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `SELECT DISTINCT` at the Tier (N13, CP-8): rows equal under the Tier
//! comparator collapse to the copy first in the Tier order, before the
//! `OFFSET` and the `LIMIT` (AQL 1.1.0 §LIMIT), and duplicates are kept
//! without `DISTINCT` (§10.1, N15).
#![cfg(feature = "merge")]

use std::cmp::Ordering;
use std::collections::BTreeSet;

use openehr_federation::merge::{Disagreement, NodeAnswer, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_its::rest::generated::query::ResultSetRow;
use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::json;

/// One `RESULT_SET` cell as a node writes it.
#[expect(
    clippy::disallowed_types,
    reason = "the test seam: a RESULT_SET cell is a JSON value on the ITS-REST wire"
)]
type Cell = serde_json::Value;

/// `SELECT DISTINCT a, b ORDER BY a LIMIT limit`: `b` breaks ties, and the
/// two columns make a row distinct.
fn distinct(direction: Direction, limit: Option<u64>) -> ResultOrder {
    ResultOrder::new(vec![SortKey::new(0, direction)], vec![1], limit).with_distinct(vec![0, 1])
}

fn kept(order: &ResultOrder, nodes: Vec<(&str, Vec<ResultSetRow>)>) -> Vec<ResultSetRow> {
    let answers = nodes
        .into_iter()
        .map(|(endpoint, rows)| NodeAnswer::new(endpoint, rows))
        .collect();
    let merged = merge(answers, order);
    assert!(
        merged.refused().is_empty(),
        "no node is refused: {:?}",
        merged.refused()
    );
    merged.rows().to_vec()
}

fn number(text: &str) -> Cell {
    serde_json::from_str(text).expect("a JSON number")
}

// conformance: CP-8
#[test]
fn a_duplicate_from_two_nodes_collapses_under_distinct_and_stays_without_it() {
    let nodes = || {
        vec![
            (
                "node-a",
                vec![vec![json!("x"), json!(1)], vec![json!("y"), json!(1)]],
            ),
            ("node-b", vec![vec![json!("x"), json!(1)]]),
        ]
    };
    assert_eq!(
        kept(&distinct(Direction::Ascending, None), nodes()),
        [vec![json!("x"), json!(1)], vec![json!("y"), json!(1)]],
        "N13: one row per value across nodes"
    );
    let plain = ResultOrder::new(vec![SortKey::new(0, Direction::Ascending)], vec![1], None);
    assert_eq!(
        kept(&plain, nodes()),
        [
            vec![json!("x"), json!(1)],
            vec![json!("x"), json!(1)],
            vec![json!("y"), json!(1)]
        ],
        "§10.1, N15: the Tier keeps duplicates unless the client asks for DISTINCT"
    );
}

// conformance: CP-8
#[test]
fn without_order_by_a_duplicate_collapses_to_the_first_endpoint_copy() {
    let order = ResultOrder::new(Vec::new(), Vec::new(), None).with_distinct(vec![0]);
    assert_eq!(
        kept(
            &order,
            vec![
                ("node-b", vec![vec![number("2.0")], vec![json!("b")]]),
                ("node-a", vec![vec![json!("a")], vec![number("2")]]),
            ]
        ),
        [vec![json!("a")], vec![number("2")], vec![json!("b")]],
        "N13: node-a's copy of 2 is first in endpoint id order and is the one kept"
    );
}

// conformance: CP-8
#[test]
fn the_kept_copy_is_the_first_under_order_by_then_endpoint_id() {
    let rows = kept(
        &distinct(Direction::Descending, None),
        vec![
            ("node-b", vec![vec![number("1.5"), number("3")]]),
            ("node-a", vec![vec![number("1.50"), number("3.0")]]),
            ("node-c", vec![vec![number("0"), number("3")]]),
        ],
    );
    assert_eq!(
        rows,
        [
            vec![number("1.5"), number("3.0")],
            vec![number("0"), number("3")]
        ],
        "§11.6.1: endpoint_id breaks the tie, so node-a's spelling 3.0 survives"
    );
}

// conformance: CP-8
#[test]
fn a_number_written_two_ways_is_one_value() {
    let order = ResultOrder::new(Vec::new(), Vec::new(), None).with_distinct(vec![0]);
    for (a, b) in [("2", "2.0"), ("1.5", "1.50"), ("-0.0", "0"), ("1e2", "100")] {
        assert_eq!(
            kept(
                &order,
                vec![
                    ("node-a", vec![vec![number(a)]]),
                    ("node-b", vec![vec![number(b)]])
                ]
            ),
            [vec![number(a)]],
            "{a} and {b} are one number"
        );
    }
}

// conformance: CP-8
#[test]
fn date_times_are_one_value_only_as_one_instant_written_one_way() {
    let order = ResultOrder::new(Vec::new(), Vec::new(), None).with_distinct(vec![0]);
    let same = kept(
        &order,
        vec![
            ("node-a", vec![vec![json!("2026-01-01T08:00:00Z")]]),
            ("node-b", vec![vec![json!("2026-01-01T08:00:00Z")]]),
        ],
    );
    assert_eq!(same.len(), 1, "one instant written one way is one value");
    for (a, b) in [
        ("2026-01-01T10:00:00+02:00", "2026-01-01T08:00:00Z"),
        ("2026-01-01T08:00:00Z", "2026-01-01T08:00:00"),
    ] {
        let rows = kept(
            &order,
            vec![
                ("node-a", vec![vec![json!(a)]]),
                ("node-b", vec![vec![json!(b)]]),
            ],
        );
        assert_eq!(
            rows.len(),
            2,
            "the Tier comparator orders {a} and {b} apart, so DISTINCT keeps both"
        );
    }
}

// conformance: CP-8
#[test]
fn a_data_value_is_one_value_whatever_the_order_of_its_members() {
    let order = ResultOrder::new(Vec::new(), Vec::new(), None).with_distinct(vec![0]);
    let a: Cell = serde_json::from_str(r#"{"_type":"DV_QUANTITY","magnitude":72.5,"units":"kg"}"#)
        .expect("JSON");
    let b: Cell = serde_json::from_str(r#"{"units":"kg","magnitude":72.5,"_type":"DV_QUANTITY"}"#)
        .expect("JSON");
    let c = json!({"_type": "DV_QUANTITY", "magnitude": 72.5, "units": "g"});
    let rows = kept(
        &order,
        vec![
            ("node-a", vec![vec![a.clone()]]),
            ("node-b", vec![vec![b], vec![c.clone()]]),
        ],
    );
    assert_eq!(
        rows,
        [vec![a], vec![c]],
        "72.5 kg is one value in either member order, and 72.5 g another"
    );
}

// conformance: CP-8
#[test]
fn a_column_the_client_does_not_see_never_makes_rows_distinct() {
    let order = ResultOrder::new(Vec::new(), Vec::new(), None).with_distinct(vec![0]);
    let rows = kept(
        &order,
        vec![
            ("node-a", vec![vec![json!("x"), json!("ehr-a")]]),
            ("node-b", vec![vec![json!("x"), json!("ehr-b")]]),
        ],
    );
    assert_eq!(
        rows,
        [vec![json!("x"), json!("ehr-a")]],
        "N13: only the selected columns decide, and node-a's copy is kept"
    );
}

// conformance: CP-8
#[test]
fn a_cut_node_holding_two_equal_rows_is_refused() {
    let merged = merge(
        vec![
            NodeAnswer::new(
                "node-a",
                vec![vec![number("1"), json!(1)], vec![number("1.0"), json!(1)]],
            ),
            NodeAnswer::new("node-b", vec![vec![json!(5), json!(1)]]),
        ],
        &distinct(Direction::Ascending, Some(2)),
    );
    assert_eq!(merged.refused().len(), 1);
    assert_eq!(merged.refused()[0].endpoint(), "node-a");
    assert_eq!(
        merged.refused()[0].reason(),
        Disagreement::Distinct,
        "node-a's LIMIT 2 holds one distinct value, so a second may lie past its cut"
    );
    assert_eq!(merged.rows(), [vec![json!(5), json!(1)]]);
}

// conformance: CP-8
#[test]
fn an_uncut_node_holding_two_equal_rows_is_collapsed() {
    let rows = kept(
        &distinct(Direction::Ascending, Some(3)),
        vec![(
            "node-a",
            vec![vec![number("1"), json!(1)], vec![number("1.0"), json!(1)]],
        )],
    );
    assert_eq!(
        rows,
        [vec![number("1"), json!(1)]],
        "a node under its LIMIT returned every row it holds, so the Tier can collapse them"
    );
}

// conformance: CP-8 CP-32
#[test]
fn distinct_runs_before_the_limit() {
    let rows = kept(
        &distinct(Direction::Ascending, Some(2)),
        vec![
            (
                "node-a",
                vec![vec![json!(1), json!(0)], vec![json!(2), json!(0)]],
            ),
            (
                "node-b",
                vec![vec![json!(1), json!(0)], vec![json!(3), json!(0)]],
            ),
        ],
    );
    assert_eq!(
        rows,
        [vec![json!(1), json!(0)], vec![json!(2), json!(0)]],
        "AQL 1.1.0 §LIMIT: the duplicate of 1 takes no slot of LIMIT 2"
    );
}

// conformance: CP-8 CP-32
#[test]
fn a_duplicate_straddling_a_page_edge_takes_one_slot() {
    let rows = kept(
        &distinct(Direction::Ascending, Some(3)).with_offset(1),
        vec![
            (
                "node-a",
                vec![
                    vec![json!(1), json!(0)],
                    vec![json!(2), json!(0)],
                    vec![json!(3), json!(0)],
                ],
            ),
            (
                "node-b",
                vec![
                    vec![json!(2), json!(0)],
                    vec![json!(3), json!(0)],
                    vec![json!(4), json!(0)],
                ],
            ),
        ],
    );
    assert_eq!(
        rows,
        [vec![json!(2), json!(0)], vec![json!(3), json!(0)]],
        "§11.6.2, AQL 1.1.0 §LIMIT: LIMIT 2 OFFSET 1 is the distinct rows 1 and 2, never 2 twice"
    );
}

/// One generated node: its distinct `(a, b)` values and whether it writes
/// its numbers as reals.
#[derive(Debug, Clone)]
struct Node {
    values: Vec<(Option<i32>, i32)>,
    reals: bool,
}

fn endpoint(index: usize) -> String {
    format!("node-{index}")
}

fn cell(value: i32, reals: bool) -> Cell {
    if reals {
        json!(f64::from(value))
    } else {
        json!(value)
    }
}

fn row((a, b): (Option<i32>, i32), reals: bool) -> ResultSetRow {
    vec![a.map_or(json!(null), |a| cell(a, reals)), cell(b, reals)]
}

/// The Tier order of the generated keys, null greatest.
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

/// What a node that orders as the Tier does answers: its values by `a` and
/// then `b`, cut at the window it was sent.
fn answered(node: &Node, direction: Direction, window: Option<u64>) -> Vec<ResultSetRow> {
    let mut values = node.values.clone();
    values.sort_by(|x, y| key_order(x.0, y.0, direction).then_with(|| x.1.cmp(&y.1)));
    if let Some(window) = window {
        values.truncate(usize::try_from(window).expect("a small window"));
    }
    values
        .into_iter()
        .map(|value| row(value, node.reals))
        .collect()
}

/// The page `[k, k + n)` of the distinct union of every value of every node,
/// uncut: the copy kept is the first under the key, then the endpoint id.
fn oracle(nodes: &[Node], direction: Direction, k: u64, n: Option<u64>) -> Vec<ResultSetRow> {
    let mut all: Vec<(usize, (Option<i32>, i32))> = nodes
        .iter()
        .enumerate()
        .flat_map(|(index, node)| node.values.iter().map(move |value| (index, *value)))
        .collect();
    all.sort_by(|(ea, a), (eb, b)| {
        key_order(a.0, b.0, direction)
            .then_with(|| endpoint(*ea).cmp(&endpoint(*eb)))
            .then_with(|| a.1.cmp(&b.1))
    });
    let mut seen = BTreeSet::new();
    let skip = usize::try_from(k).expect("a small offset");
    let take = n.map_or(usize::MAX, |n| usize::try_from(n).expect("a small limit"));
    all.into_iter()
        .filter(|(_, value)| seen.insert(*value))
        .map(|(index, value)| row(value, nodes[index].reals))
        .skip(skip)
        .take(take)
        .collect()
}

/// Up to four nodes of up to six values each, drawn from a small range so
/// that one value is often held by several nodes.
fn nodes() -> impl Strategy<Value = Vec<Node>> {
    vec(
        (
            proptest::collection::btree_set(
                (proptest::option::weighted(0.8, -2_i32..3), 0_i32..2),
                0..6,
            ),
            any::<bool>(),
        ),
        1..5,
    )
    .prop_map(|nodes| {
        nodes
            .into_iter()
            .map(|(values, reals)| Node {
                values: values.into_iter().collect(),
                reals,
            })
            .collect()
    })
}

fn direction() -> impl Strategy<Value = Direction> {
    prop_oneof![Just(Direction::Ascending), Just(Direction::Descending)]
}

/// The generated value of a merged row, whichever way the node wrote it.
fn value_of(row: &ResultSetRow) -> (Option<i64>, i64) {
    let integer = |cell: &Cell| -> i64 {
        cell.as_i64()
            .or_else(|| {
                cell.as_f64()
                    .and_then(|real| format!("{real:.0}").parse().ok())
            })
            .expect("a generated number")
    };
    let a = row.first().expect("two columns");
    let b = row.get(1).expect("two columns");
    ((!a.is_null()).then(|| integer(a)), integer(b))
}

proptest! {
    // conformance: CP-8 CP-32
    #[test]
    fn distinct_with_order_by_and_a_bounded_page_is_the_page_of_the_distinct_union(
        nodes in nodes(),
        direction in direction(),
        k in 0_u64..6,
        n in proptest::option::of(0_u64..5),
    ) {
        let (k, window) = match n {
            Some(n) => (k, Some(k + n)),
            None => (0, None),
        };
        let answers: Vec<NodeAnswer> = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| NodeAnswer::new(endpoint(index), answered(node, direction, window)))
            .collect();
        let merged = merge(answers, &distinct(direction, window).with_offset(k));
        let expected = oracle(&nodes, direction, k, n);
        prop_assert!(merged.refused().is_empty(), "a conforming node is never refused: {:?}", merged.refused());
        prop_assert_eq!(
            merged.rows(),
            expected.as_slice(),
            "N13, N39, AQL 1.1.0 §LIMIT: k + n rows per node give the distinct page"
        );
    }

    // conformance: CP-8
    #[test]
    fn distinct_is_idempotent_and_leaves_no_two_rows_equal(
        nodes in nodes(),
        direction in direction(),
    ) {
        let answers: Vec<NodeAnswer> = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| NodeAnswer::new(endpoint(index), answered(node, direction, None)))
            .collect();
        let once = merge(answers, &distinct(direction, None));
        let values: BTreeSet<_> = once.rows().iter().map(value_of).collect();
        prop_assert_eq!(values.len(), once.rows().len(), "no two rows are one value");
        let twice = merge(
            vec![NodeAnswer::new("node-0", once.rows().to_vec())],
            &distinct(direction, None),
        );
        // One endpoint now holds every row, so the tie-break may reorder them.
        let text = |rows: &[ResultSetRow]| -> BTreeSet<String> {
            rows.iter().map(|row| format!("{row:?}")).collect()
        };
        prop_assert_eq!(twice.rows().len(), once.rows().len(), "DISTINCT of a distinct answer drops nothing");
        prop_assert_eq!(text(twice.rows()), text(once.rows()), "and keeps every row as written");
    }

    // conformance: CP-8 CP-32
    #[test]
    fn distinct_without_order_by_keeps_limit_distinct_values_of_the_union(
        nodes in nodes(),
        limit in proptest::option::of(0_u64..6),
    ) {
        let order = ResultOrder::new(Vec::new(), Vec::new(), limit).with_distinct(vec![0, 1]);
        let answers: Vec<NodeAnswer> = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let mut rows: Vec<ResultSetRow> = node.values.iter().map(|value| row(*value, node.reals)).collect();
                if let Some(limit) = limit {
                    rows.truncate(usize::try_from(limit).expect("a small limit"));
                }
                NodeAnswer::new(endpoint(index), rows)
            })
            .collect();
        let merged = merge(answers, &order);
        prop_assert!(merged.refused().is_empty());
        let union: BTreeSet<(Option<i64>, i64)> = nodes
            .iter()
            .flat_map(|node| node.values.iter().map(|(a, b)| (a.map(i64::from), i64::from(*b))))
            .collect();
        let values: BTreeSet<_> = merged.rows().iter().map(value_of).collect();
        prop_assert_eq!(values.len(), merged.rows().len(), "no two rows are one value");
        prop_assert!(values.is_subset(&union));
        let expected = limit.map_or(union.len(), |limit| union.len().min(usize::try_from(limit).expect("small")));
        prop_assert_eq!(values.len(), expected, "LIMIT counts distinct rows");
    }
}
