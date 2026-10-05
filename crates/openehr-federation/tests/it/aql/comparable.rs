// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keys a node can order on (§11.6.1, N13, AQL master03-syntax §ORDER BY):
//! a path to a primitive value or a `DV_ORDERED` data value. Under `SELECT
//! DISTINCT` only such a selected path is pushed as a key, and a `DISTINCT`
//! cut at a `LIMIT` that selects any other path is refused. Outside
//! `DISTINCT`, a cut at a `LIMIT` ordered on any other path is refused, and
//! the same order with no `LIMIT` is answered from every row at the Tier.

use std::num::NonZeroU32;

use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, OffsetStrategy, Paging, analyse};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_query::bind::Parameters;

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

/// The refusal of `aql` in an ask-all deployment under `paging` and the
/// offset `strategy`.
fn refused_with(aql: &str, paging: Paging, strategy: OffsetStrategy) -> Refusal {
    analyse(
        aql,
        &Parameters::new(),
        paging,
        &ask_all().with_offset_strategy(strategy),
    )
    .expect_err("the query is refused")
}

fn bounded() -> OffsetStrategy {
    OffsetStrategy::Bounded {
        max_window: NonZeroU32::new(100).expect("non-zero"),
    }
}

fn assert_incomparable_order(refusal: &Refusal, why: &str) {
    assert!(
        matches!(refusal, Refusal::IncomparableOrderKey { at: Some(_) }),
        "{why}: got {refusal:?}"
    );
    assert_eq!(refusal.kind(), "incomparable-order-key", "{why}");
}

// conformance: CP-32
#[test]
fn a_limit_on_a_dv_text_order_key_is_refused() {
    let refusal = refused(&patient_query("c/uid/value", "ORDER BY c/name LIMIT 10"));
    assert_incomparable_order(
        &refusal,
        "AQL §ORDER BY: a DV_TEXT is not a DV_ORDERED, so no node order on it holds the \
         federated first rows (§11.6.1)",
    );
}

// conformance: CP-32
#[test]
fn a_limit_on_a_whole_rm_object_is_refused() {
    let refusal = refused(&patient_query("c/uid/value", "ORDER BY c LIMIT 10"));
    assert_incomparable_order(
        &refusal,
        "AQL §ORDER BY: a COMPOSITION is neither a primitive nor an Ordered type",
    );
}

// conformance: CP-32
#[test]
fn a_limit_on_an_element_value_is_refused() {
    let refusal = refused(&patient_query(
        "c/uid/value",
        &format!("ORDER BY {ELEMENT_VALUE} DESC LIMIT 10"),
    ));
    assert_incomparable_order(
        &refusal,
        "the RM declares ELEMENT.value a DATA_VALUE, of which a DV_TEXT is one",
    );
}

// conformance: CP-32
#[test]
fn a_limit_on_a_collection_is_refused() {
    let refusal = refused(&patient_query("c/uid/value", "ORDER BY c/content LIMIT 10"));
    assert_incomparable_order(
        &refusal,
        "COMPOSITION.content is a list, and a list is not ordered",
    );
}

// conformance: CP-32
#[test]
fn a_limit_on_a_path_the_rm_does_not_resolve_is_refused() {
    let refusal = refused(&patient_query(
        "c/uid/value",
        "ORDER BY c/no_such_attribute LIMIT 10",
    ));
    assert_incomparable_order(
        &refusal,
        "no RM type gives the path a value, so no order is shown for it",
    );
}

// conformance: CP-32
#[test]
fn a_later_incomparable_key_is_refused_and_located() {
    let aql = patient_query(
        "c/uid/value",
        "ORDER BY c/context/start_time, c/name DESC LIMIT 10",
    );
    let refusal = refused(&aql);
    assert_incomparable_order(
        &refusal,
        "a tie on the first key is ordered on the second, and AQL defines no order for it",
    );
    let start = aql
        .find("c/name DESC")
        .expect("the fixture orders on c/name");
    assert_eq!(
        refusal.at(),
        Some(&(start..start + "c/name".len())),
        "§5.4.3: the refusal points at the key by position, never by its text"
    );
}

// conformance: CP-32
#[test]
fn top_on_an_incomparable_key_is_refused() {
    let refusal = refused(&patient_query("TOP 10 c/uid/value", "ORDER BY c/name"));
    assert_incomparable_order(
        &refusal,
        "AQL §TOP: TOP n is read as LIMIT n, which each node is sent",
    );
}

// conformance: CP-32
#[test]
fn the_fetch_member_on_an_incomparable_key_is_refused() {
    let refusal = refused_with(
        &patient_query("c/uid/value", "ORDER BY c/name"),
        Paging {
            offset: None,
            fetch: Some(10),
        },
        OffsetStrategy::Reject,
    );
    assert_incomparable_order(
        &refusal,
        "ITS-REST fetch pages like LIMIT, and each node is sent it",
    );
}

// conformance: CP-32
#[test]
fn a_bounded_page_on_an_incomparable_key_is_refused() {
    let refusal = refused_with(
        &patient_query("c/uid/value", "ORDER BY c/name LIMIT 10 OFFSET 10"),
        Paging::default(),
        bounded(),
    );
    assert_incomparable_order(
        &refusal,
        "§11.6.2: each node is cut at k + n, which needs the same total order",
    );
}

// conformance: CP-32
#[test]
fn an_unscoped_limit_on_an_incomparable_key_is_refused() {
    let refusal = refused(&format!(
        "SELECT c/uid/value {FROM} ORDER BY c/name LIMIT 10"
    ));
    assert_incomparable_order(
        &refusal,
        "the same containment holds with no patient, over every member's rows",
    );
}

// conformance: CP-32
#[test]
fn with_no_limit_an_incomparable_key_is_sent_as_written_and_ordered_at_the_tier() {
    let aql = patient_query("c/uid/value", "ORDER BY c/name DESC");
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT c/uid/value, c/name {SCOPED} = '{EHR_ID}' \
             ORDER BY c/name DESC, c/uid/value ASC"
        ),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(vec![SortKey::new(1, Direction::Descending)], vec![0], None),
        "§11.6.1: with no LIMIT every row reaches the Tier, which orders them all"
    );
}

// conformance: CP-32
#[test]
fn a_limit_on_primitives_and_ordered_data_values_is_answered() {
    let aql = patient_query(
        "c/uid/value",
        &format!(
            "ORDER BY c/name/value, c/context/start_time DESC, {ELEMENT_VALUE}/magnitude, \
             e/time_created, c/uid/value LIMIT 10"
        ),
    );
    let analysis = analysed(&aql, &ask_all()).expect("every key is comparable");
    assert!(
        matches!(analysis, Analysis::Patient(_)),
        "a patient query: {analysis:?}"
    );
    assert_eq!(
        analysis.order().limit(),
        Some(10),
        "AQL §ORDER BY: a string, a DV_DATE_TIME, a magnitude and a date-time are comparable"
    );
    assert_eq!(analysis.order().keys().len(), 5, "every client key is kept");
}

// conformance: CP-32
#[test]
fn a_bounded_page_on_a_comparable_key_is_answered() {
    let analysis = analyse(
        &patient_query("c/uid/value", "ORDER BY c/name/value LIMIT 10 OFFSET 10"),
        &Parameters::new(),
        Paging::default(),
        &ask_all().with_offset_strategy(bounded()),
    )
    .expect("the page is computed from k + n rows per node");
    assert_eq!(
        analysis.order().limit(),
        Some(20),
        "§11.6.2: k + n per node"
    );
}
