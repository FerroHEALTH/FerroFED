// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keys a node is sent under `SELECT DISTINCT` (§11.6.1, N13, AQL
//! master03-syntax §ORDER BY): only a selected path to a primitive value or a
//! `DV_ORDERED` data value is pushed as a key, and a `DISTINCT` cut at a
//! `LIMIT` that selects any other path is refused, since AQL defines no order
//! a node could pin it by.

use std::num::NonZeroU32;

use openehr_federation::aql::OffsetStrategy;
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::order::{Direction, ResultOrder, SortKey};

use super::{EHR_ID, analysed, ask_all, assert_same_aql, node_aql, refused};

const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value";
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
const SCOPED: &str =
    "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE e/ehr_id/value";

/// An archetype path to the `DATA_VALUE` of an `ELEMENT`: the RM admits a
/// `DV_TEXT` there as well as a `DV_QUANTITY`.
const ELEMENT_VALUE: &str = "o/data[at0001]/events[at0002]/data[at0003]/items[at0004]/value";

fn patient_query(select: &str, tail: &str) -> String {
    format!("SELECT {select} {FROM} WHERE {SUBJECT} = '4711' {tail}")
}

fn order(aql: &str) -> ResultOrder {
    analysed(aql, &ask_all())
        .expect("the query is accepted")
        .order()
        .clone()
}

fn assert_incomparable(select: &str, tail: &str, why: &str) {
    let refusal = refused(&patient_query(select, tail));
    assert!(
        matches!(refusal, Refusal::IncomparableDistinctKey { at: Some(_) }),
        "{why}: got {refusal:?}"
    );
    assert_eq!(refusal.kind(), "incomparable-distinct-key");
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_a_limit_with_a_whole_rm_object_is_refused() {
    assert_incomparable(
        "DISTINCT c, c/name/value",
        "ORDER BY c/name/value LIMIT 10",
        "AQL §ORDER BY: a COMPOSITION is neither a primitive nor an Ordered type",
    );
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_a_limit_with_a_dv_text_is_refused() {
    assert_incomparable(
        "DISTINCT c/name, c/uid/value",
        "ORDER BY c/uid/value LIMIT 10",
        "AQL §ORDER BY: a DV_TEXT is not a DV_ORDERED",
    );
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_a_limit_with_an_element_value_is_refused() {
    assert_incomparable(
        &format!("DISTINCT {ELEMENT_VALUE}, c/uid/value"),
        "ORDER BY c/uid/value LIMIT 10",
        "the RM declares ELEMENT.value a DATA_VALUE, of which a DV_TEXT is one",
    );
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_a_limit_with_a_collection_is_refused() {
    assert_incomparable(
        "DISTINCT c/content, c/uid/value",
        "ORDER BY c/uid/value LIMIT 10",
        "COMPOSITION.content is a list, and a list is not ordered",
    );
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_a_limit_with_a_path_the_rm_does_not_resolve_is_refused() {
    assert_incomparable(
        "DISTINCT c/no_such_attribute, c/uid/value",
        "ORDER BY c/uid/value LIMIT 10",
        "no RM type gives the path a value, so no order is shown for it",
    );
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_a_limit_on_an_incomparable_order_by_key_is_refused() {
    assert_incomparable(
        "DISTINCT c/name",
        "ORDER BY c/name LIMIT 10",
        "§11.6.1: the client's own key orders a node no better than a pushed one",
    );
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_a_bounded_page_with_a_whole_rm_object_is_refused() {
    let bounded = ask_all().with_offset_strategy(OffsetStrategy::Bounded {
        max_window: NonZeroU32::new(100).expect("non-zero"),
    });
    let aql = patient_query(
        "DISTINCT c, c/name/value",
        "ORDER BY c/name/value LIMIT 10 OFFSET 10",
    );
    let refusal = analysed(&aql, &bounded).expect_err("the page is refused");
    assert_eq!(
        refusal.kind(),
        "incomparable-distinct-key",
        "§11.6.2: each node is cut at k + n, which the same tie decides"
    );
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_with_no_limit_an_incomparable_path_is_not_a_key() {
    let aql = patient_query(
        "DISTINCT c/name/value, c, c/name, c/uid/value",
        "ORDER BY c/name/value",
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT DISTINCT c/name/value, c, c/name, c/uid/value {SCOPED} = '{EHR_ID}' \
             ORDER BY c/name/value, c/uid/value ASC"
        ),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(vec![SortKey::new(0, Direction::Ascending)], vec![3], None)
            .with_distinct(vec![0, 1, 2, 3]),
        "AQL §ORDER BY: c and c/name are compared under DISTINCT and never sent as keys"
    );
}

// conformance: CP-8 CP-32
#[test]
fn under_distinct_a_limit_with_ordered_data_values_and_primitives_is_pinned() {
    let aql = patient_query(
        &format!(
            "DISTINCT c/name/value, c/context/start_time, {ELEMENT_VALUE}/magnitude, \
             e/time_created"
        ),
        "ORDER BY c/name/value LIMIT 10",
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT DISTINCT c/name/value, c/context/start_time, {ELEMENT_VALUE}/magnitude, \
             e/time_created {SCOPED} = '{EHR_ID}' \
             ORDER BY c/name/value, c/context/start_time ASC, {ELEMENT_VALUE}/magnitude ASC, \
             e/time_created ASC LIMIT 10"
        ),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(
            vec![SortKey::new(0, Direction::Ascending)],
            vec![1, 2, 3],
            Some(10)
        )
        .with_distinct(vec![0, 1, 2, 3]),
        "AQL §ORDER BY: a DV_DATE_TIME is a DV_ORDERED and a magnitude is a number"
    );
}
