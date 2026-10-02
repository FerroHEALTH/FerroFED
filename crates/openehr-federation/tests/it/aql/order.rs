// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Tier order written into the node query (§11.6.1, N13, N39;
//! `docs/architecture.md` section 9, decisions A28 and A43): the hidden
//! `ORDER BY` column, the uid tie-break, the client's `LIMIT n` unchanged, and
//! the refusals.

use std::num::NonZeroUsize;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, ColumnSource, Context, Paging, Targeting, analyse};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_query::bind::Parameters;

use super::{EHR_ID, NAMESPACE, analysed, ask_all, assert_same_aql, node_aql, refused};

const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value";
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c";
const SCOPED: &str = "FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value";

fn patient_query(select: &str, tail: &str) -> String {
    format!("SELECT {select} {FROM} WHERE {SUBJECT} = '4711' {tail}")
}

fn order(aql: &str) -> ResultOrder {
    analysed(aql, &ask_all())
        .expect("the query is accepted")
        .order()
        .clone()
}

fn ehr_id() -> HierObjectId {
    HierObjectId::new(EHR_ID).expect("the fixture ehr_id is a HIER_OBJECT_ID")
}

fn unscoped_aql(aql: &str) -> String {
    match analysed(aql, &ask_all()) {
        Ok(Analysis::Unscoped(query)) => query.node_query().aql().to_owned(),
        other => panic!("expected an unscoped query, got {other:?}"),
    }
}

#[test]
fn an_order_by_path_that_is_selected_is_read_where_it_is() {
    let aql = patient_query(
        "c/uid/value, c/context/start_time/value",
        "ORDER BY c/context/start_time/value DESC LIMIT 10",
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT c/uid/value, c/context/start_time/value {SCOPED} = '{EHR_ID}' \
             ORDER BY c/context/start_time/value DESC, c/uid/value ASC LIMIT 10"
        ),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(
            vec![SortKey::new(1, Direction::Descending)],
            vec![0],
            Some(10)
        ),
        "the key reads node column 1, the uid node column 0, and the façade keeps LIMIT 10"
    );
}

#[test]
fn an_order_by_path_that_is_not_selected_becomes_a_hidden_column() {
    let aql = patient_query(
        "c/name/value",
        "ORDER BY c/context/start_time/value LIMIT 5",
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT c/name/value, c/context/start_time/value, c/uid/value {SCOPED} = '{EHR_ID}' \
             ORDER BY c/context/start_time/value, c/uid/value ASC LIMIT 5"
        ),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(
            vec![SortKey::new(1, Direction::Ascending)],
            vec![2],
            Some(5)
        )
    );
    let Ok(Analysis::Patient(query)) = analysed(&aql, &ask_all()) else {
        panic!("a patient query")
    };
    assert_eq!(
        query.for_node(&ehr_id()).columns(),
        [ColumnSource::Node(0)],
        "A28: the façade answers only the column it selected"
    );
    assert_eq!(
        query.columns().len(),
        1,
        "N17: columns[] is the façade query's"
    );
}

#[test]
fn the_uid_already_in_the_order_is_not_added_twice() {
    let aql = patient_query("c/uid/value", "ORDER BY c/uid/value DESC LIMIT 3");
    assert_same_aql(
        &node_aql(&aql),
        &format!("SELECT c/uid/value {SCOPED} = '{EHR_ID}' ORDER BY c/uid/value DESC LIMIT 3"),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(
            vec![SortKey::new(0, Direction::Descending)],
            vec![0],
            Some(3)
        )
    );
}

#[test]
fn a_version_variable_supplies_the_uid_when_no_composition_is_contained() {
    let aql = format!(
        "SELECT v/commit_audit/time_committed/value FROM EHR e CONTAINS VERSION v \
         WHERE {SUBJECT} = '4711' ORDER BY v/commit_audit/time_committed/value LIMIT 2"
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT v/commit_audit/time_committed/value, v/uid/value FROM EHR e CONTAINS VERSION v \
             WHERE e/ehr_id/value = '{EHR_ID}' \
             ORDER BY v/commit_audit/time_committed/value, v/uid/value ASC LIMIT 2"
        ),
    );
}

#[test]
fn an_order_with_no_limit_is_pushed_with_the_tie_break_and_no_limit() {
    let aql = patient_query("c/name/value", "ORDER BY c/name/value");
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT c/name/value, c/uid/value {SCOPED} = '{EHR_ID}' \
             ORDER BY c/name/value, c/uid/value ASC"
        ),
    );
    assert_eq!(
        order(&aql).limit(),
        None,
        "no LIMIT, so every row reaches the Tier"
    );
}

#[test]
fn a_query_with_no_order_by_is_dispatched_with_its_limit_unchanged() {
    let aql = patient_query("c/uid/value", "LIMIT 10");
    assert_same_aql(
        &node_aql(&aql),
        &format!("SELECT c/uid/value {SCOPED} = '{EHR_ID}' LIMIT 10"),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(Vec::new(), Vec::new(), Some(10)),
        "§11.6.1: any n rows of the union answer a query that fixes no order"
    );
}

#[test]
fn an_unscoped_query_gets_the_same_pushdown() {
    let aql = format!("SELECT c/name/value {FROM} ORDER BY c/name/value DESC LIMIT 1");
    assert_same_aql(
        &unscoped_aql(&aql),
        &format!(
            "SELECT c/name/value, c/uid/value {FROM} \
             ORDER BY c/name/value DESC, c/uid/value ASC LIMIT 1"
        ),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(
            vec![SortKey::new(0, Direction::Descending)],
            vec![1],
            Some(1)
        )
    );
}

#[test]
fn top_is_folded_into_limit() {
    let aql =
        format!("SELECT TOP 4 c/name/value {FROM} WHERE {SUBJECT} = '4711' ORDER BY c/name/value");
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT c/name/value, c/uid/value {SCOPED} = '{EHR_ID}' \
             ORDER BY c/name/value, c/uid/value ASC LIMIT 4"
        ),
    );
    assert_eq!(order(&aql).limit(), Some(4));
}

/// AQL master03-syntax §TOP: "It is not allowed to use `TOP` while also using
/// `LIMIT` clause in the same query", so counts that agree are no excuse.
#[test]
fn top_and_a_limit_that_agree_are_refused() {
    let aql = format!(
        "SELECT TOP 4 c/name/value {FROM} WHERE {SUBJECT} = '4711' ORDER BY c/name/value LIMIT 4"
    );
    let refusal = refused(&aql);
    assert_eq!(refusal, Refusal::TopWithLimit);
    assert_eq!(refusal.kind(), "top-with-limit");
}

/// AQL master03-syntax §LIMIT: "It is not allowed to use `LIMIT` while also
/// using `TOP` clause in the same query".
#[test]
fn top_and_a_limit_that_disagree_are_refused() {
    let aql = format!(
        "SELECT TOP 4 c/name/value {FROM} WHERE {SUBJECT} = '4711' ORDER BY c/name/value LIMIT 5"
    );
    assert_eq!(refused(&aql), Refusal::TopWithLimit);
}

/// AQL master03-syntax §TOP forbids the pair before anything reads the
/// direction or the `OFFSET`.
#[test]
fn top_with_a_limit_and_an_offset_is_refused() {
    let aql = format!(
        "SELECT TOP 4 BACKWARD c/name/value {FROM} WHERE {SUBJECT} = '4711' \
         ORDER BY c/name/value LIMIT 4 OFFSET 2"
    );
    assert_eq!(refused(&aql), Refusal::TopWithLimit);
}

/// ITS-REST Query API, Common Headers and Query Parameters: `fetch` "cannot
/// be combined with AQL-top".
#[test]
fn top_and_the_fetch_member_are_refused() {
    let aql =
        format!("SELECT TOP 4 c/name/value {FROM} WHERE {SUBJECT} = '4711' ORDER BY c/name/value");
    for fetch in [4, 10] {
        let refusal = analyse(
            &aql,
            &Parameters::new(),
            Paging {
                offset: None,
                fetch: Some(fetch),
            },
            &ask_all(),
        )
        .expect_err("fetch cannot be combined with TOP");
        assert_eq!(refusal, Refusal::TopWithFetch, "fetch {fetch}");
        assert_eq!(refusal.kind(), "top-with-fetch");
    }
}

/// The ITS-REST text restricts only `fetch`, so `TOP n` with an `offset`
/// member of zero is still `LIMIT n`.
#[test]
fn top_with_a_zero_offset_member_is_read_as_limit() {
    let aql =
        format!("SELECT TOP 4 c/name/value {FROM} WHERE {SUBJECT} = '4711' ORDER BY c/name/value");
    let analysis = analyse(
        &aql,
        &Parameters::new(),
        Paging {
            offset: Some(0),
            fetch: None,
        },
        &ask_all(),
    )
    .expect("an offset member of zero skips nothing");
    assert_eq!(analysis.order().limit(), Some(4));
}

#[test]
fn top_backward_is_refused() {
    let aql = format!("SELECT TOP 4 BACKWARD c/name/value {FROM} WHERE {SUBJECT} = '4711'");
    let refusal = refused(&aql);
    assert_eq!(refusal, Refusal::TopBackward);
    assert_eq!(refusal.kind(), "top-backward");
}

#[test]
fn under_distinct_the_selected_paths_are_the_tie_break_and_nothing_is_added() {
    let aql = patient_query(
        "DISTINCT c/name/value, c/archetype_details/template_id/value",
        "ORDER BY c/name/value DESC LIMIT 2",
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT DISTINCT c/name/value, c/archetype_details/template_id/value {SCOPED} = '{EHR_ID}' \
             ORDER BY c/name/value DESC, c/archetype_details/template_id/value ASC LIMIT 2"
        ),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(
            vec![SortKey::new(0, Direction::Descending)],
            vec![1],
            Some(2)
        ),
        "N13: no uid under DISTINCT, the other selected column breaks ties after endpoint_id"
    );
}

#[test]
fn under_distinct_an_order_by_path_that_is_not_selected_is_refused() {
    let aql = patient_query(
        "DISTINCT c/name/value",
        "ORDER BY c/context/start_time/value",
    );
    let refusal = refused(&aql);
    assert!(
        matches!(refusal, Refusal::OrderNotSelected { at: Some(_) }),
        "A28: a hidden column would change which rows are distinct, got {refusal:?}"
    );
    assert_eq!(refusal.kind(), "order-not-selected");
}

#[test]
fn a_directed_aggregate_is_dispatched_unchanged_with_its_limit() {
    let one = Context::new(Targeting::Directed {
        endpoints: NonZeroUsize::MIN,
    })
    .with_default_namespace(NAMESPACE);
    let aql = format!(
        "SELECT COUNT(c/uid/value) {FROM} WHERE {SUBJECT} = '4711' ORDER BY c/name/value LIMIT 1"
    );
    let Ok(Analysis::Patient(query)) = analysed(&aql, &one) else {
        panic!("a directed aggregate is a patient query")
    };
    assert_same_aql(
        query.for_node(&ehr_id()).aql(),
        &format!("SELECT COUNT(c/uid/value) {SCOPED} = '{EHR_ID}' ORDER BY c/name/value LIMIT 1"),
    );
    assert_eq!(
        analysed(&aql, &one).expect("accepted").order(),
        &ResultOrder::new(Vec::new(), Vec::new(), Some(1)),
        "N14: one node's aggregate is the answer, so the merge only applies the limit"
    );
}
