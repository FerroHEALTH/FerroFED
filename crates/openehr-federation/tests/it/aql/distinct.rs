// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The columns `SELECT DISTINCT` compares at the Tier (N13): the node columns
//! the client sees, never a subject column the gateway re-injects nor a
//! column the rewrite adds, and none at all for a query without `DISTINCT`.

use openehr_federation::aql::Analysis;

use super::{EHR_ID, analysed, ask_all, assert_same_aql, node_aql};

const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value";
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c";

fn distinct_columns(aql: &str) -> Option<Vec<usize>> {
    analysed(aql, &ask_all())
        .expect("the query is accepted")
        .order()
        .distinct()
        .map(<[usize]>::to_vec)
}

// conformance: CP-8
#[test]
fn every_selected_column_of_an_unscoped_query_is_compared() {
    let aql = format!("SELECT DISTINCT c/name/value, c/archetype_details/template_id/value {FROM}");
    assert!(matches!(
        analysed(&aql, &ask_all()),
        Ok(Analysis::Unscoped(_))
    ));
    assert_eq!(distinct_columns(&aql), Some(vec![0, 1]), "N13");
}

// conformance: CP-8
#[test]
fn a_re_injected_subject_column_is_not_compared() {
    let aql = format!(
        "SELECT DISTINCT {SUBJECT}, c/name/value {FROM} WHERE {SUBJECT} = '4711' ORDER BY c/name/value"
    );
    assert_eq!(
        distinct_columns(&aql),
        Some(vec![0]),
        "the subject is one value for the whole answer, so node column 0 decides alone"
    );
}

// conformance: CP-8
#[test]
fn the_column_kept_for_a_subject_only_query_is_not_compared() {
    let aql = format!("SELECT DISTINCT {SUBJECT} {FROM} WHERE {SUBJECT} = '4711'");
    assert_same_aql(
        &node_aql(&aql),
        &format!("SELECT DISTINCT e/ehr_id/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}'"),
    );
    assert_eq!(
        distinct_columns(&aql),
        Some(Vec::new()),
        "each node's ehr_id differs, and the client sees only the subject, one row"
    );
}

#[test]
fn a_query_without_distinct_compares_nothing() {
    let aql = format!("SELECT c/name/value {FROM}");
    assert_eq!(
        distinct_columns(&aql),
        None,
        "§10.1, N15: duplicates pass through"
    );
}
