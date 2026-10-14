// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ORDER BY` on an ENDPOINT attribute through the merge (§8.1, §9.3,
//! §11.6.1, N13, N39; CP-37): the Tier orders on the value each endpoint's
//! rows carry, in the place the client wrote the key, compares it as it
//! compares a string cell, and breaks a tie on the endpoint id and the uid.
#![cfg(feature = "merge")]

use openehr_federation::merge::{Disagreement, Merged, NodeAnswer, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use serde_json::json;

/// The answer of endpoint `endpoint`, of organisation `organisation`, with
/// one uid per row, its attribute values `[endpoint id, organisation]`.
fn answer(endpoint: &str, organisation: &str, uids: &[&str]) -> NodeAnswer {
    let rows = uids.iter().map(|uid| vec![json!(uid)]).collect();
    NodeAnswer::new(endpoint, rows)
        .with_attributes(vec![endpoint.to_owned(), organisation.to_owned()])
}

/// The uid of every merged row.
fn uids(merged: &Merged) -> Vec<&str> {
    merged
        .rows()
        .iter()
        .map(|row| {
            row.first()
                .and_then(|cell| cell.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// Two nodes whose uids interleave, as each sends them under `ORDER BY` the
/// uid ascending.
fn interleaved(second_organisation: &str) -> Vec<NodeAnswer> {
    vec![
        answer("node-b", second_organisation, &["u2", "u4"]),
        answer("node-a", "org-a", &["u1", "u3"]),
    ]
}

// conformance: CP-37
#[test]
fn an_attribute_key_alone_orders_the_endpoints_and_the_uid_breaks_the_tie() {
    for (direction, expected) in [
        (Direction::Ascending, ["u1", "u3", "u2"]),
        (Direction::Descending, ["u2", "u4", "u1"]),
    ] {
        let order = ResultOrder::new(vec![SortKey::endpoint(0, direction)], vec![0], Some(3));
        let merged = merge(interleaved("org-b"), &order);
        assert!(merged.refused().is_empty(), "{direction:?}");
        assert_eq!(expected.as_slice(), uids(&merged), "{direction:?}");
    }
}

// conformance: CP-37
#[test]
fn an_attribute_key_before_a_node_key_orders_first_and_a_tie_falls_to_the_node_key() {
    let order = ResultOrder::new(
        vec![
            SortKey::endpoint(1, Direction::Descending),
            SortKey::new(0, Direction::Descending),
        ],
        vec![0],
        Some(3),
    );
    let merged = merge(
        vec![
            answer("node-b", "org-b", &["u4", "u2"]),
            answer("node-a", "org-a", &["u3", "u1"]),
        ],
        &order,
    );
    assert!(merged.refused().is_empty());
    assert_eq!(["u4", "u2", "u3"].as_slice(), uids(&merged), "org-b first");

    let shared = merge(
        vec![
            answer("node-b", "org-a", &["u4", "u2"]),
            answer("node-a", "org-a", &["u3", "u1"]),
        ],
        &order,
    );
    assert_eq!(
        ["u4", "u3", "u2"].as_slice(),
        uids(&shared),
        "one organisation: the rows interleave on the uid"
    );
}

// conformance: CP-37
#[test]
fn an_attribute_key_after_a_node_key_breaks_its_ties() {
    let rows = |endpoint: &str, uids: &[&str]| {
        let rows = uids
            .iter()
            .map(|uid| vec![json!("same"), json!(uid)])
            .collect();
        NodeAnswer::new(endpoint, rows).with_attributes(vec![endpoint.to_owned()])
    };
    let order = ResultOrder::new(
        vec![
            SortKey::new(0, Direction::Ascending),
            SortKey::endpoint(0, Direction::Descending),
        ],
        vec![1],
        Some(3),
    );
    let merged = merge(
        vec![rows("node-a", &["u1", "u3"]), rows("node-b", &["u2", "u4"])],
        &order,
    );
    assert!(merged.refused().is_empty());
    let uids: Vec<&str> = merged
        .rows()
        .iter()
        .map(|row| {
            row.get(1)
                .and_then(|cell| cell.as_str())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(vec!["u2", "u4", "u1"], uids, "node-b before node-a");
}

// conformance: CP-37
#[test]
fn an_attribute_value_compares_as_a_string_cell_does() {
    let values = ["Zeta", "alpha", "Beta", "beta", "10", "9"];
    let as_attribute: Vec<NodeAnswer> = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            NodeAnswer::new(format!("node-{index}"), vec![vec![json!(value)]])
                .with_attributes(vec![(*value).to_owned()])
        })
        .collect();
    let as_column: Vec<NodeAnswer> = values
        .iter()
        .enumerate()
        .map(|(index, value)| NodeAnswer::new(format!("node-{index}"), vec![vec![json!(value)]]))
        .collect();
    for direction in [Direction::Ascending, Direction::Descending] {
        let by_attribute = merge(
            as_attribute.clone(),
            &ResultOrder::new(vec![SortKey::endpoint(0, direction)], vec![], None),
        );
        let by_column = merge(
            as_column.clone(),
            &ResultOrder::new(vec![SortKey::new(0, direction)], vec![], None),
        );
        assert_eq!(
            uids(&by_column),
            uids(&by_attribute),
            "{direction:?}: one string comparison for a node cell and an attribute"
        );
    }
}

#[test]
fn a_node_cut_at_its_limit_is_checked_on_its_own_keys_only() {
    let order = ResultOrder::new(
        vec![SortKey::endpoint(0, Direction::Ascending)],
        vec![0],
        Some(2),
    );
    let merged = merge(
        vec![
            answer("node-a", "org-a", &["u3", "u1"]),
            answer("node-b", "org-b", &["u2", "u4"]),
        ],
        &order,
    );
    assert_eq!(1, merged.refused().len());
    assert_eq!("node-a", merged.refused()[0].endpoint());
    assert_eq!(Disagreement::Order, merged.refused()[0].reason());
    assert_eq!(["u2", "u4"].as_slice(), uids(&merged));
}

#[test]
fn an_endpoint_without_the_attribute_value_is_refused() {
    let order = ResultOrder::new(
        vec![SortKey::endpoint(2, Direction::Ascending)],
        vec![0],
        None,
    );
    let merged = merge(interleaved("org-b"), &order);
    assert_eq!(2, merged.refused().len());
    assert!(
        merged
            .refused()
            .iter()
            .all(|refused| refused.reason() == Disagreement::ShortRow)
    );
    assert!(merged.rows().is_empty());
}
