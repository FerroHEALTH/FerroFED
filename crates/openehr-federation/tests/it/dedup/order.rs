// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Tier order under version-identity dedup (§10.2, §11.6.1, §11.6.2,
//! CP-9, CP-32): a suppressed copy takes no slot of the `LIMIT`, and the
//! copies of one version whose ids differ only in case rank as one (BASE
//! `master05-identification_package.adoc` §"Composite Identifiers and Case"),
//! while outside dedup a uid orders as sent.

use openehr_federation::merge::{Disagreement, NodeAnswer, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use serde_json::json;

use super::{accepted, answer, spelled, uid};

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

/// `SELECT c/uid/value, c/name/value LIMIT 1 OFFSET 1`: each node sent
/// `LIMIT 2` and ordered on the uid (§11.6.2).
fn second_by_uid() -> ResultOrder {
    ResultOrder::new(
        vec![SortKey::new(0, Direction::Ascending)],
        vec![0],
        Some(2),
    )
    .with_version_key(0)
    .with_offset(1)
}

// conformance: CP-9 CP-32
#[test]
fn a_case_variant_copy_at_a_page_edge_ranks_as_its_version_and_keeps_its_text() {
    // node-1 created `y` and spells its UUID in upper case; node-0 holds an
    // import of `y` in lower case. node-1 orders without regard to case, so
    // `r` comes first and `y` is the last row of its cut.
    let (r, y, s) = (uid(1, 1, 1), uid(2, 1, 1), uid(3, 0, 1));
    let original = spelled(&y, 2);
    assert!(
        original < r,
        "byte for byte, the upper-case spelling sorts first"
    );
    let merged = accepted(
        vec![
            answer(
                "node-0",
                0,
                vec![
                    vec![json!(y), json!("imported")],
                    vec![json!(s), json!("local")],
                ],
            ),
            answer(
                "node-1",
                1,
                vec![
                    vec![json!(r), json!("own")],
                    vec![json!(original), json!("original")],
                ],
            ),
        ],
        &second_by_uid(),
    );
    assert_eq!(
        merged.rows(),
        [vec![json!(original), json!("original")]],
        "§11.6.2: the second row of r, y, s is y, node-1's copy, its uid as node-1 sent it"
    );
    assert_eq!(merged.suppressed().rows(), 1, "§10.2: suppressed_rows");
    assert_eq!(merged.suppressed().endpoints(), ["node-0"]);
}

// conformance: CP-9 CP-32
#[test]
fn a_node_that_orders_uids_byte_for_byte_across_case_is_refused_at_its_cut() {
    let (r, y) = (uid(1, 1, 1), uid(2, 1, 1));
    let merged = merge(
        vec![answer(
            "node-1",
            1,
            vec![
                vec![json!(spelled(&y, 2)), json!("original")],
                vec![json!(r), json!("own")],
            ],
        )],
        &second_by_uid(),
    );
    let refused: Vec<(&str, Disagreement)> = merged
        .refused()
        .iter()
        .map(|refused| (refused.endpoint(), refused.reason()))
        .collect();
    assert_eq!(
        refused,
        [("node-1", Disagreement::Order)],
        "§11.1, §11.6.1: under dedup the Tier orders y after r, and a node cut in another \
         order can hide a row of the page"
    );
}

// conformance: CP-32
#[test]
fn without_dedup_a_uid_orders_by_code_point_as_sent() {
    let (r, y) = (uid(1, 1, 1), spelled(&uid(2, 1, 1), 2));
    let order = ResultOrder::new(
        vec![SortKey::new(0, Direction::Ascending)],
        vec![0],
        Some(1),
    );
    let merged = accepted(
        vec![
            NodeAnswer::new("node-0", vec![vec![json!(r), json!("a")]]),
            NodeAnswer::new("node-1", vec![vec![json!(y), json!("b")]]),
        ],
        &order,
    );
    assert_eq!(
        merged.rows(),
        [vec![json!(y), json!("b")]],
        "§11.6.1: outside dedup the Tier compares strings by code point, U+0041 before U+0061"
    );
}
