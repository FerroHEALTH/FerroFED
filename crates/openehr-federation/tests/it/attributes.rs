// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ENDPOINT attributes through the merge (§9.3, N12, N13; CP-37): each
//! merged row keeps the attribute values of the endpoint it came from, and
//! under `DISTINCT` they take part in which rows are equal.
#![cfg(feature = "merge")]

use openehr_federation::aggregate::{Recombination, Recombine};
use openehr_federation::merge::{NodeAnswer, combine, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use serde_json::json;

/// The answer of endpoint `endpoint`, of organisation `organisation`, with
/// one name per row.
fn answer(endpoint: &str, organisation: &str, names: &[&str]) -> NodeAnswer {
    let rows = names.iter().map(|name| vec![json!(name)]).collect();
    NodeAnswer::new(endpoint, rows)
        .with_attributes(vec![endpoint.to_owned(), organisation.to_owned()])
}

/// The attribute values with each merged row, as `(name, endpoint)`.
fn provenance(merged: &openehr_federation::merge::Merged) -> Vec<(String, String)> {
    merged
        .rows()
        .iter()
        .zip(merged.attributes())
        .map(|(row, values)| {
            (
                row.first()
                    .and_then(|cell| cell.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                values.first().cloned().unwrap_or_default(),
            )
        })
        .collect()
}

// conformance: CP-37
#[test]
fn each_row_keeps_the_attributes_of_its_endpoint_in_the_tier_order() {
    let order = ResultOrder::new(vec![SortKey::new(0, Direction::Ascending)], vec![], Some(3));
    let merged = merge(
        vec![
            answer("node-b", "org-b", &["beta", "delta"]),
            answer("node-a", "org-a", &["alpha", "gamma"]),
        ],
        &order,
    );
    assert!(merged.refused().is_empty());
    assert_eq!(
        vec![
            ("alpha".to_owned(), "node-a".to_owned()),
            ("beta".to_owned(), "node-b".to_owned()),
            ("delta".to_owned(), "node-b".to_owned()),
        ],
        provenance(&merged),
        "§11.6.1: the attributes follow their row through the order and the cut"
    );
    assert_eq!(merged.rows().len(), merged.attributes().len());
}

// conformance: CP-37
#[test]
fn without_an_order_each_row_keeps_its_endpoint_attributes() {
    let merged = merge(
        vec![
            answer("node-b", "org-b", &["beta"]),
            answer("node-a", "org-a", &["alpha"]),
        ],
        &ResultOrder::unordered(),
    );
    assert_eq!(
        vec![
            ("alpha".to_owned(), "node-a".to_owned()),
            ("beta".to_owned(), "node-b".to_owned()),
        ],
        provenance(&merged)
    );
}

// conformance: CP-8 CP-37
#[test]
fn under_distinct_rows_of_two_endpoints_are_two_values() {
    let order = ResultOrder::new(vec![SortKey::new(0, Direction::Ascending)], vec![], None)
        .with_distinct(vec![0]);
    let merged = merge(
        vec![
            answer("node-a", "org-a", &["alpha"]),
            answer("node-b", "org-a", &["alpha"]),
        ],
        &order,
    );
    assert_eq!(
        vec![
            ("alpha".to_owned(), "node-a".to_owned()),
            ("alpha".to_owned(), "node-b".to_owned()),
        ],
        provenance(&merged),
        "N13: the endpoint ids differ, so the rows do"
    );
}

// conformance: CP-8 CP-37
#[test]
fn under_distinct_a_shared_attribute_value_collapses_the_rows() {
    let order = ResultOrder::new(vec![SortKey::new(0, Direction::Ascending)], vec![], None)
        .with_distinct(vec![0]);
    let shared = |endpoint: &str| {
        NodeAnswer::new(endpoint, vec![vec![json!("alpha")]])
            .with_attributes(vec!["org-a".to_owned()])
    };
    let merged = merge(vec![shared("node-b"), shared("node-a")], &order);
    assert_eq!(1, merged.rows().len(), "N13: one organisation, one name");
    assert_eq!(
        [vec!["org-a".to_owned()]],
        merged.attributes(),
        "the copy first in the Tier order, node-a's, is kept"
    );
}

// conformance: CP-8 CP-37
#[test]
fn distinct_on_attributes_alone_keeps_one_row_per_value_and_refuses_no_cut_node() {
    let order = ResultOrder::new(vec![], vec![], Some(2)).with_distinct(vec![]);
    let node = |endpoint: &str| {
        NodeAnswer::new(endpoint, vec![vec![json!("ehr-1")], vec![json!("ehr-2")]])
            .with_attributes(vec![endpoint.to_owned()])
    };
    let merged = merge(vec![node("node-a"), node("node-b")], &order);
    assert!(
        merged.refused().is_empty(),
        "every row of a node is its one attribute value, so its cut hides none"
    );
    assert_eq!(
        [vec!["node-a".to_owned()], vec!["node-b".to_owned()]],
        merged.attributes()
    );
}

#[test]
fn a_recombined_row_carries_no_endpoint_attributes() {
    let recombination = Recombination::new(vec![Recombine::Count { column: 0 }]);
    let node = |endpoint: &str| {
        NodeAnswer::new(endpoint, vec![vec![json!(2)]]).with_attributes(vec![endpoint.to_owned()])
    };
    let merged = combine(
        vec![node("node-a"), node("node-b")],
        &recombination,
        &ResultOrder::unordered(),
    )
    .expect("a count recombines");
    assert_eq!([vec![json!(4)]], merged.rows());
    assert_eq!(
        [Vec::<String>::new()],
        merged.attributes(),
        "the recombined row comes from no single endpoint"
    );
}
