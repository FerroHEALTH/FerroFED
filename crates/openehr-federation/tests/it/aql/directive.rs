// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `FROM ENDPOINT` and `ORGANISATION` directive (§8.1, N11, CP-6): lifted
//! out of the façade query, so no node query carries it or a path through its
//! variable, and the node set it selects drives the directed rules of N14
//! and §11.6.3.

use std::num::NonZeroUsize;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::directive::FacadeQuery;
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, ColumnSource, Context, Paging, Targeting};
use openehr_query::bind::Parameters;
use openehr_query::federation::DirectiveKind;

use super::{EHR_ID, NAMESPACE, ask_all, assert_same_aql};

/// The patient predicate every patient fixture carries.
const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value = '4711'";

/// A deployment directed at `endpoints` endpoints.
fn directed(endpoints: usize) -> Context {
    let endpoints = NonZeroUsize::new(endpoints).expect("a directive names an endpoint");
    ask_all().with_targeting(Targeting::Directed { endpoints })
}

/// The analysis of `aql` under `context`.
fn analysed(aql: &str, context: &Context) -> Result<Analysis, Refusal> {
    FacadeQuery::parse(aql)?.analyse(&Parameters::new(), Paging::default(), context)
}

/// The node query of a patient analysis for the fixture `ehr_id`.
fn node_aql(analysis: Analysis) -> String {
    let Analysis::Patient(query) = analysis else {
        panic!("expected a patient query, got {analysis:?}");
    };
    let ehr_id = HierObjectId::new(EHR_ID).expect("the fixture ehr_id is a HIER_OBJECT_ID");
    query.for_node(&ehr_id).aql().to_owned()
}

// conformance: CP-6
#[test]
fn the_directive_is_lifted_out_with_its_kind_and_identifiers_in_order() {
    let endpoint = FacadeQuery::parse(&format!(
        r#"SELECT c/uid/value FROM ENDPOINT p ["node_2", "node_1"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {SUBJECT}"#
    ))
    .expect("the directive parses");
    let directive = endpoint.directive().expect("the query carries a directive");
    assert_eq!(DirectiveKind::Endpoint, directive.kind);
    assert_eq!(["node_2", "node_1"], directive.ids.as_slice());
    assert_eq!(Some("p"), directive.variable.as_deref());

    let organisation = FacadeQuery::parse(
        r#"SELECT c/uid/value FROM ORGANISATION ["org-a"] CONTAINS EHR e CONTAINS COMPOSITION c"#,
    )
    .expect("the organisation selector parses");
    let directive = organisation
        .directive()
        .expect("the query carries a directive");
    assert_eq!(DirectiveKind::Organisation, directive.kind);
    assert_eq!(["org-a"], directive.ids.as_slice());
}

#[test]
fn an_undirected_query_has_no_directive() {
    let query = FacadeQuery::parse("SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c")
        .expect("strict AQL parses");
    assert!(query.directive().is_none());
}

// conformance: CP-6 CP-26
#[test]
fn a_directed_patient_query_reaches_the_node_as_the_undirected_one_does() {
    let undirected =
        format!("SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {SUBJECT}");
    let pinned = format!(
        r#"SELECT c/uid/value FROM ENDPOINT ["node_1", "node_2"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {SUBJECT}"#
    );
    let directed_node = node_aql(analysed(&pinned, &directed(2)).expect("§8.1"));
    let undirected_node = node_aql(analysed(&undirected, &ask_all()).expect("§7.1"));
    assert_same_aql(&directed_node, &undirected_node);
    for absent in ["ENDPOINT", "node_1", "node_2", "4711"] {
        assert!(
            !directed_node.contains(absent),
            "N7: the node query carries {absent:?}: {directed_node}"
        );
    }
}

// conformance: CP-6 CP-26
#[test]
fn the_organisation_selector_never_reaches_a_node() {
    let aql = format!(
        r#"SELECT c/uid/value FROM ORGANISATION ["org-a"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {SUBJECT}"#
    );
    let node = node_aql(analysed(&aql, &directed(1)).expect("§8.1"));
    for absent in ["ORGANISATION", "org-a", "4711"] {
        assert!(
            !node.contains(absent),
            "the node query carries {absent:?}: {node}"
        );
    }
}

// conformance: CP-6
#[test]
fn an_unscoped_query_selecting_only_endpoint_attributes_asks_each_node_one_ehr_column() {
    let aql = r#"SELECT p/id AS endpoint_id FROM ENDPOINT p ["node_1"] CONTAINS COMPOSITION c"#;
    let Ok(Analysis::Unscoped(query)) = analysed(aql, &directed(1)) else {
        panic!("the query names no patient");
    };
    assert_eq!([ColumnSource::Endpoint], query.node_query().columns());
    let node = query.node_query().aql();
    assert!(
        !node.contains("p/"),
        "§9.3: no node is asked an ENDPOINT attribute: {node}"
    );
    assert!(
        openehr_query::parser::parse_str(node).is_ok(),
        "the node query is strict AQL: {node}"
    );
}

#[test]
fn a_directed_query_names_its_node_set_in_a_localizing_deployment() {
    let localized = Context::new(Targeting::Localized).with_default_namespace(NAMESPACE);
    let aql =
        r#"SELECT c/uid/value FROM ENDPOINT ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c"#;
    assert_eq!(
        Err(Refusal::NodeSetUndefined),
        analysed(aql, &localized).map(|_| ()),
        "undirected in a localizing deployment, the node set is undefined (N4)"
    );
    let endpoints = NonZeroUsize::MIN;
    let pinned = localized.with_targeting(Targeting::Directed { endpoints });
    assert!(
        matches!(analysed(aql, &pinned), Ok(Analysis::Unscoped(_))),
        "§8.1: the directive is the node set"
    );
}

// conformance: CP-6
#[test]
fn a_directed_aggregate_to_one_endpoint_is_dispatched_unchanged_and_to_two_is_refused() {
    let aql = format!(
        r#"SELECT COUNT(c/uid/value) AS n FROM ENDPOINT ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {SUBJECT}"#
    );
    let analysis = analysed(&aql, &directed(1)).expect("N14: a directed single-node aggregate");
    assert!(
        analysis.recombination().is_none(),
        "§11.6.3: nothing to recombine"
    );
    assert_same_aql(
        &node_aql(analysis),
        &format!(
            "SELECT COUNT(c/uid/value) AS n FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'"
        ),
    );
    let refusal = analysed(&aql, &directed(2)).expect_err("N14: two endpoints are a fan-out");
    assert!(
        matches!(refusal, Refusal::UndirectedAggregate { .. }),
        "{refusal:?}"
    );
    let declared = directed(2).with_decomposable_aggregates(AggregateFunction::ALL);
    let recombined = analysed(&aql, &declared).expect("§11.6.3: COUNT decomposes");
    assert!(recombined.recombination().is_some(), "summed at the Tier");
}

// conformance: CP-6
#[test]
fn an_undefined_function_directed_to_one_endpoint_passes_and_to_two_is_refused() {
    let aql = r#"SELECT MEDIAN(c/context/start_time/magnitude) FROM ENDPOINT ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c"#;
    assert!(
        analysed(aql, &directed(1)).is_ok(),
        "§11.6.3: one node answers its own function"
    );
    assert!(
        matches!(
            analysed(aql, &directed(2)),
            Err(Refusal::UndefinedFunction { .. })
        ),
        "N14: the gateway cannot tell whether the function aggregates"
    );
}

#[test]
fn a_directed_limit_follows_the_merge_rules() {
    let aql = format!(
        r#"SELECT c/uid/value FROM ENDPOINT ["node_1", "node_2"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {SUBJECT} ORDER BY c/uid/value LIMIT 5"#
    );
    let analysis = analysed(&aql, &directed(2)).expect("§11.6.1");
    assert_eq!(
        Some(5),
        analysis.order().limit(),
        "the Tier cuts the merged rows"
    );
    assert!(
        node_aql(analysis).contains("LIMIT 5"),
        "each node keeps LIMIT n (§11.6.1)"
    );
}

/// The refusal `aql` draws when directed at one endpoint.
fn refused(aql: &str) -> Refusal {
    analysed(aql, &directed(1)).expect_err("the query is refused")
}

#[test]
fn the_directive_variable_bound_again_in_from_is_refused() {
    let refusal = refused(
        r#"SELECT c/uid/value FROM ENDPOINT e ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c"#,
    );
    assert!(
        matches!(refusal, Refusal::EndpointVariable { .. }),
        "{refusal:?}"
    );
    assert_eq!("endpoint-variable", refusal.kind());
}

#[test]
fn the_directive_variable_outside_a_selected_column_is_refused() {
    for aql in [
        format!(
            r#"SELECT c/uid/value FROM ENDPOINT p ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {SUBJECT} AND p/id = 'node_1'"#
        ),
        r#"SELECT c/uid/value FROM ENDPOINT p ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c ORDER BY p/id"#
            .to_owned(),
        r#"SELECT LENGTH(p/id) FROM ENDPOINT p ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c"#
            .to_owned(),
    ] {
        let refusal = refused(&aql);
        assert!(
            matches!(refusal, Refusal::EndpointVariable { at: Some(_) }),
            "{aql}: {refusal:?}"
        );
    }
}

#[test]
fn a_malformed_directive_is_not_aql() {
    for aql in [
        "SELECT c/uid/value FROM ENDPOINT p [] CONTAINS EHR e",
        "SELECT c/uid/value FROM ENDPOINT p [node_1] CONTAINS EHR e",
        r#"SELECT c/uid/value FROM ENDPOINT p ["node_1" CONTAINS EHR e"#,
        r#"SELECT c/uid/value FROM ENDPOINT p ["node_1"] EHR e"#,
    ] {
        assert!(
            matches!(FacadeQuery::parse(aql), Err(Refusal::NotAql { .. })),
            "{aql}"
        );
    }
}
