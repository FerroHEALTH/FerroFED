// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Tier order written into the node query (§11.6.1, N13, N39): the hidden
//! `ORDER BY` column, the row-key tie-break (the uid, or the `ehr_id` of a row
//! with no uid), the client's `LIMIT n` unchanged, and the refusals.

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
        "the hidden column is stripped: the façade answers only the column it selected"
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
        )
        .with_distinct(vec![0, 1]),
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
        "N13: a hidden column would change which rows are distinct, got {refusal:?}"
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

/// The node query, the Tier order and the façade columns of an unscoped
/// `aql`.
fn unscoped(aql: &str) -> (String, ResultOrder, Vec<Option<String>>) {
    let analysis = analysed(aql, &ask_all()).expect("the query is accepted");
    let columns = analysis
        .columns()
        .iter()
        .map(|column| column.path.clone())
        .collect();
    let order = analysis.order().clone();
    match analysis {
        Analysis::Unscoped(query) => (query.node_query().aql().to_owned(), order, columns),
        other @ Analysis::Patient(_) => panic!("expected an unscoped query, got {other:?}"),
    }
}

// conformance: CP-32
#[test]
fn an_ehr_only_query_is_tie_broken_on_the_ehr_id() {
    let aql = "SELECT e/ehr_status/uid/value AS status FROM EHR e \
               ORDER BY e/time_created/value DESC LIMIT 2";
    let (node, order, columns) = unscoped(aql);
    assert_same_aql(
        &node,
        "SELECT e/ehr_status/uid/value AS status, e/time_created/value, e/ehr_id/value \
         FROM EHR e ORDER BY e/time_created/value DESC, e/ehr_id/value ASC LIMIT 2",
    );
    assert_eq!(
        order,
        ResultOrder::new(
            vec![SortKey::new(1, Direction::Descending)],
            vec![2],
            Some(2)
        ),
        "§11.6.1: a row with no uid breaks ties on its ehr_id after endpoint_id"
    );
    assert_eq!(
        columns,
        [Some("/ehr_status/uid/value".to_owned())],
        "N17, §9.2: columns[] is the client's query, without the pushed key"
    );
    let Ok(Analysis::Unscoped(query)) = analysed(aql, &ask_all()) else {
        panic!("an unscoped query")
    };
    assert_eq!(
        query.node_query().columns(),
        [ColumnSource::Node(0)],
        "the hidden key and the ehr_id are stripped from the client's rows"
    );
}

// conformance: CP-32
#[test]
fn a_selected_ehr_id_is_the_tie_break_where_it_is() {
    let (node, order, _) =
        unscoped("SELECT e/ehr_id/value FROM EHR e ORDER BY e/time_created/value LIMIT 3");
    assert_same_aql(
        &node,
        "SELECT e/ehr_id/value, e/time_created/value FROM EHR e \
         ORDER BY e/time_created/value, e/ehr_id/value ASC LIMIT 3",
    );
    assert_eq!(
        order,
        ResultOrder::new(
            vec![SortKey::new(1, Direction::Ascending)],
            vec![0],
            Some(3)
        )
    );
}

// conformance: CP-32
#[test]
fn a_containment_with_no_versioned_object_is_keyed_on_its_ehr() {
    let magnitude = "o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude";
    let (node, order, _) = unscoped(&format!(
        "SELECT {magnitude} FROM EHR e CONTAINS OBSERVATION o ORDER BY {magnitude} DESC LIMIT 4"
    ));
    assert_same_aql(
        &node,
        &format!(
            "SELECT {magnitude}, e/ehr_id/value FROM EHR e CONTAINS OBSERVATION o \
             ORDER BY {magnitude} DESC, e/ehr_id/value ASC LIMIT 4"
        ),
    );
    assert_eq!(order.tie_break(), [1]);
}

// conformance: CP-32
#[test]
fn a_version_outranks_the_ehr_and_the_ehr_outranks_its_status() {
    let (node, _, _) = unscoped(
        "SELECT v/commit_audit/time_committed/value FROM EHR e CONTAINS VERSION v \
         ORDER BY v/commit_audit/time_committed/value LIMIT 2",
    );
    assert_same_aql(
        &node,
        "SELECT v/commit_audit/time_committed/value, v/uid/value FROM EHR e CONTAINS VERSION v \
         ORDER BY v/commit_audit/time_committed/value, v/uid/value ASC LIMIT 2",
    );
    let (node, _, _) = unscoped(
        "SELECT s/is_queryable FROM EHR e CONTAINS EHR_STATUS s ORDER BY s/is_queryable LIMIT 2",
    );
    assert_same_aql(
        &node,
        "SELECT s/is_queryable, e/ehr_id/value FROM EHR e CONTAINS EHR_STATUS s \
         ORDER BY s/is_queryable, e/ehr_id/value ASC LIMIT 2",
    );
}

// conformance: CP-32
#[test]
fn an_ehr_status_with_no_ehr_class_is_keyed_on_its_uid() {
    let (node, order, _) =
        unscoped("SELECT s/is_queryable FROM EHR_STATUS s ORDER BY s/is_queryable LIMIT 2");
    assert_same_aql(
        &node,
        "SELECT s/is_queryable, s/uid/value FROM EHR_STATUS s \
         ORDER BY s/is_queryable, s/uid/value ASC LIMIT 2",
    );
    assert_eq!(order.tie_break(), [1]);
}

/// The RM recommends a uid only on a tree-root `FOLDER`, and a `FOLDER` class
/// matches sub-folders too, so a folder's uid is no key. A query with no key
/// is dispatched as written and never refused: AQL master03-syntax §LIMIT
/// leaves a unique order to the query's own `ORDER BY`.
// conformance: CP-32
#[test]
fn a_folder_is_never_a_key_and_a_query_with_no_key_is_dispatched_as_written() {
    let aql = "SELECT f/name/value FROM FOLDER f ORDER BY f/name/value LIMIT 2";
    let (node, order, _) = unscoped(aql);
    assert_same_aql(&node, aql);
    assert_eq!(
        order,
        ResultOrder::new(
            vec![SortKey::new(0, Direction::Ascending)],
            Vec::new(),
            Some(2)
        ),
        "no key to push: the Tier orders the rows it receives on their cells"
    );
    let (node, _, _) =
        unscoped("SELECT f/name/value FROM EHR e CONTAINS FOLDER f ORDER BY f/name/value LIMIT 2");
    assert_same_aql(
        &node,
        "SELECT f/name/value, e/ehr_id/value FROM EHR e CONTAINS FOLDER f \
         ORDER BY f/name/value, e/ehr_id/value ASC LIMIT 2",
    );
}

// conformance: CP-32
#[test]
fn under_distinct_an_ehr_only_query_gets_no_ehr_id_column() {
    // N13: a hidden ehr_id would make rows equal on the client's columns
    // distinct at the node; the selected columns break ties instead.
    let aql = "SELECT DISTINCT e/ehr_status/uid/value, e/time_created/value FROM EHR e \
               ORDER BY e/time_created/value DESC LIMIT 2";
    let (node, order, columns) = unscoped(aql);
    assert_same_aql(
        &node,
        "SELECT DISTINCT e/ehr_status/uid/value, e/time_created/value FROM EHR e \
         ORDER BY e/time_created/value DESC, e/ehr_status/uid/value ASC LIMIT 2",
    );
    assert_eq!(
        order,
        ResultOrder::new(
            vec![SortKey::new(1, Direction::Descending)],
            vec![0],
            Some(2)
        )
    );
    assert_eq!(columns.len(), 2, "N17, §9.2: the client's two columns");
}

// conformance: CP-32
#[test]
fn a_patient_query_scoped_to_one_ehr_pushes_no_ehr_key() {
    // Every row of the node query has the one ehr_id the gateway scoped it to.
    let aql = format!(
        "SELECT e/time_created/value FROM EHR e WHERE {SUBJECT} = '4711' \
         ORDER BY e/time_created/value LIMIT 1"
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT e/time_created/value FROM EHR e WHERE e/ehr_id/value = '{EHR_ID}' \
             ORDER BY e/time_created/value LIMIT 1"
        ),
    );
    assert_eq!(
        order(&aql),
        ResultOrder::new(
            vec![SortKey::new(0, Direction::Ascending)],
            Vec::new(),
            Some(1)
        )
    );
}
