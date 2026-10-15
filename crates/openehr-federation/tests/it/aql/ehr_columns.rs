// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The columns of a query that names no patient whose values name the `EHR`
//! each delivered row came from: a projected `<ehr>/ehr_id/value` or
//! `<ehr>/ehr_id` of an `EHR` the query binds. Reading them changes nothing
//! a node is asked (§7.1): the node query is the one the client wrote.

use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::{Analysis, ColumnSource};

use super::{analysed, ask_all, assert_same_aql};

fn ehr_columns(aql: &str) -> Vec<usize> {
    match analysed(aql, &ask_all()) {
        Ok(analysis) => analysis.ehr_columns().to_vec(),
        Err(refusal) => panic!("the query is analysed: {refusal:?}"),
    }
}

#[test]
fn a_projected_ehr_id_value_names_the_ehr_of_each_row() {
    let aql = "SELECT c/uid/value, e/ehr_id/value FROM EHR e CONTAINS COMPOSITION c";
    assert_eq!(vec![1], ehr_columns(aql));
}

#[test]
fn the_whole_ehr_id_names_it_too() {
    assert_eq!(
        vec![0],
        ehr_columns("SELECT e/ehr_id, c FROM EHR e CONTAINS COMPOSITION c")
    );
}

#[test]
fn a_query_that_projects_no_ehr_id_names_none() {
    assert!(ehr_columns("SELECT c FROM EHR e CONTAINS COMPOSITION c").is_empty());
    assert!(
        ehr_columns("SELECT c/uid/value FROM COMPOSITION c").is_empty(),
        "a query that binds no EHR"
    );
}

#[test]
fn another_attribute_of_the_ehr_names_none() {
    assert!(
        ehr_columns("SELECT e/system_id/value, c FROM EHR e CONTAINS COMPOSITION c").is_empty()
    );
}

#[test]
fn a_patient_query_names_its_ehrs_by_resolution_never_by_a_column() {
    let aql = "SELECT e/ehr_id/value FROM EHR e \
               WHERE e/ehr_status/subject/external_ref/id/value = '12345' \
               AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1'";
    assert!(ehr_columns(aql).is_empty());
}

#[test]
fn reading_the_column_leaves_the_node_query_as_written() {
    let aql = "SELECT e/ehr_id/value, c/uid/value FROM EHR e CONTAINS COMPOSITION c";
    let Ok(Analysis::Unscoped(query)) = analysed(aql, &ask_all()) else {
        panic!("a query that names no patient");
    };
    assert_same_aql(query.node_query().aql(), aql);
    assert_eq!(
        &[ColumnSource::Node(0), ColumnSource::Node(1)],
        query.node_query().columns(),
        "no column added"
    );
}

#[test]
fn a_recombined_aggregate_names_no_ehr() {
    let aql = "SELECT COUNT(e/ehr_id/value) FROM EHR e CONTAINS COMPOSITION c";
    let context = ask_all().with_decomposable_aggregates(AggregateFunction::ALL);
    let analysis = analysed(aql, &context).expect("a decomposable aggregate is analysed");
    assert!(analysis.recombination().is_some(), "recombined at the Tier");
    assert!(analysis.ehr_columns().is_empty());
}
