// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `SELECT DISTINCT` under version-identity dedup (§10.2, §11.6.1, N13,
//! CP-8): the version uid is one value however its case is spelled, as the
//! dedup groups it and the Tier orders it (BASE
//! `master05-identification_package.adoc` §"Composite Identifiers and Case";
//! AQL master03-syntax §DISTINCT), while outside dedup a uid compares as
//! sent.

use openehr_federation::merge::{Disagreement, NodeAnswer, Refused, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use serde_json::json;

use super::{accepted, answer, spelled, uid};

/// `SELECT DISTINCT c/uid/value, c/name/value ORDER BY c/name/value`, with
/// the client's `limit`: the name is the key and the uid the tie-break.
fn by_name(limit: Option<u64>) -> ResultOrder {
    ResultOrder::new(vec![SortKey::new(1, Direction::Ascending)], vec![0], limit)
        .with_distinct(vec![0, 1])
}

/// One node that breaks case preservation within itself: it answers one
/// version twice, once with its UUID in upper case.
fn spelled_twice(y: &str) -> NodeAnswer {
    answer(
        "node-0",
        0,
        vec![
            vec![json!(y), json!("Visit")],
            vec![json!(spelled(y, 2)), json!("Visit")],
        ],
    )
}

// conformance: CP-8 CP-9
#[test]
fn under_dedup_distinct_holds_case_variants_of_a_uid_one_value() {
    let y = uid(1, 0, 1);
    let merged = accepted(vec![spelled_twice(&y)], &by_name(None).with_version_key(0));
    assert_eq!(
        merged.rows(),
        [vec![json!(spelled(&y, 2)), json!("Visit")]],
        "BASE: the two spellings are one identifier, so one row is distinct; the one kept is \
         first in the Tier order and keeps its text as sent"
    );
}

// conformance: CP-8
#[test]
fn outside_dedup_distinct_compares_a_uid_as_sent() {
    let y = uid(1, 0, 1);
    let merged = accepted(vec![spelled_twice(&y)], &by_name(None));
    assert_eq!(
        merged.rows().len(),
        2,
        "the Tier orders a uid as sent outside dedup, and DISTINCT agrees with that order"
    );
}

// conformance: CP-8 CP-9 CP-32
#[test]
fn under_dedup_a_node_cut_on_two_case_variants_is_refused() {
    let y = uid(1, 0, 1);
    let merged = merge(
        vec![spelled_twice(&y)],
        &by_name(Some(2)).with_version_key(0),
    );
    assert_eq!(
        merged
            .refused()
            .iter()
            .map(Refused::reason)
            .collect::<Vec<_>>(),
        [Disagreement::Distinct],
        "§11.6.1: the node kept both at its LIMIT, so a distinct row may lie past its cut"
    );
    assert!(merged.rows().is_empty());
}

// conformance: CP-8 CP-9
#[test]
fn under_dedup_distinct_folds_case_beside_the_endpoint_attributes() {
    // `SELECT DISTINCT c/uid/value, c/name/value, p/id …`: the ENDPOINT id
    // joins each row's tuple (§9.3, N13), one value within one endpoint.
    let (y, z) = (uid(1, 0, 1), uid(2, 1, 1));
    let merged = accepted(
        vec![
            spelled_twice(&y).with_attributes(vec!["node-0".to_owned()]),
            answer("node-1", 1, vec![vec![json!(z), json!("Visit")]])
                .with_attributes(vec!["node-1".to_owned()]),
        ],
        &by_name(None).with_version_key(0),
    );
    assert_eq!(
        merged.rows(),
        [
            vec![json!(spelled(&y, 2)), json!("Visit")],
            vec![json!(z), json!("Visit")],
        ],
        "BASE: node-0's two spellings are one value beside its one attribute; node-1's row stays"
    );
    assert_eq!(
        merged.attributes(),
        [vec!["node-0".to_owned()], vec!["node-1".to_owned()]],
        "§9.3: each kept row carries its own endpoint's attributes"
    );
}
