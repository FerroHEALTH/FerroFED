// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Functions across a fan-out (§11.6.3, N14, N39; AQL master03-syntax
//! §Functions): a single-row function AQL 1.1.0 defines is computed at each
//! node over its own rows and forwarded as written, an aggregate follows the
//! decomposition rules, and a function AQL does not define is refused unless
//! the query is directed to one endpoint, because the gateway cannot tell
//! whether it aggregates.

use std::num::NonZeroUsize;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aggregate::{AggregateFunction, Recombine};
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, Context, Targeting};

use super::{EHR_ID, NAMESPACE, analysed, ask_all, assert_same_aql};

const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value = '4711'";
const MAGNITUDE: &str = "o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude";

/// A deployment directed to `endpoints` endpoints that declares every
/// aggregate decomposable.
fn directed(endpoints: NonZeroUsize) -> Context {
    Context::new(Targeting::Directed { endpoints })
        .with_default_namespace(NAMESPACE)
        .with_decomposable_aggregates(AggregateFunction::ALL)
}

/// An ask-all deployment that declares every aggregate decomposable, as the
/// server does by default.
fn declared() -> Context {
    ask_all().with_decomposable_aggregates(AggregateFunction::ALL)
}

/// The query every node receives for an analysis that names no patient.
fn unscoped_node_aql(analysis: &Analysis) -> String {
    let Analysis::Unscoped(query) = analysis else {
        panic!("the query names no patient, got {analysis:?}");
    };
    query.node_query().aql().to_owned()
}

/// The node query of a patient analysis, for the fixture `ehr_id`.
fn patient_node_aql(analysis: &Analysis) -> String {
    let Analysis::Patient(query) = analysis else {
        panic!("the query names a patient, got {analysis:?}");
    };
    query
        .for_node(&HierObjectId::new(EHR_ID).unwrap())
        .aql()
        .to_owned()
}

/// Asserts `aql` is answered as rows across a fan-out: dispatched as written to
/// every node, with no recombination.
fn forwarded_as_rows(aql: &str) {
    let analysis = analysed(aql, &declared()).unwrap_or_else(|refusal| panic!("{aql}: {refusal}"));
    assert!(
        analysis.recombination().is_none(),
        "{aql}: a single-row function keeps the node rows as rows"
    );
    assert_same_aql(&unscoped_node_aql(&analysis), aql);
}

fn refusal(aql: &str, context: &Context) -> Refusal {
    match analysed(aql, context) {
        Err(refusal) => refusal,
        Ok(analysis) => panic!("{aql} should be refused, got {analysis:?}"),
    }
}

// ── a function AQL 1.1.0 does not define ─────────────────────────────────────

// conformance: CP-10 CP-32
#[test]
fn an_undirected_function_outside_aql_in_select_is_refused() {
    let aql = format!("SELECT MEDIAN({MAGNITUDE}) AS median {FROM}");
    let refused = refusal(&aql, &declared());
    let Refusal::UndefinedFunction { at: Some(at) } = &refused else {
        panic!("§11.6.3: {refused:?}");
    };
    assert_eq!(
        aql.get(at.clone()),
        Some(MAGNITUDE),
        "the refusal points at the call's argument"
    );
    assert_eq!(refused.kind(), "undefined-function");
}

// conformance: CP-10 CP-32
#[test]
fn a_function_outside_aql_is_refused_in_a_patient_query_across_nodes() {
    let aql = format!("SELECT MEDIAN({MAGNITUDE}) {FROM} WHERE {SUBJECT}");
    assert!(matches!(
        refusal(&aql, &declared()),
        Refusal::UndefinedFunction { .. }
    ));
}

// conformance: CP-10 CP-32
#[test]
fn a_function_outside_aql_directed_to_two_endpoints_is_refused() {
    let aql = format!("SELECT MEDIAN({MAGNITUDE}) {FROM}");
    let two = directed(NonZeroUsize::MIN.saturating_add(1));
    assert!(matches!(
        refusal(&aql, &two),
        Refusal::UndefinedFunction { .. }
    ));
}

// conformance: CP-10
#[test]
fn a_function_outside_aql_directed_to_one_endpoint_passes_through() {
    let one = directed(NonZeroUsize::MIN);
    let unscoped = format!("SELECT MEDIAN({MAGNITUDE}) AS median {FROM}");
    let analysis = analysed(&unscoped, &one).unwrap();
    assert!(analysis.recombination().is_none(), "N14: the node answers");
    assert_same_aql(&unscoped_node_aql(&analysis), &unscoped);

    let patient = format!("SELECT MEDIAN({MAGNITUDE}) AS median {FROM} WHERE {SUBJECT}");
    let analysis = analysed(&patient, &one).unwrap();
    assert_same_aql(
        &patient_node_aql(&analysis),
        &format!("SELECT MEDIAN({MAGNITUDE}) AS median {FROM} WHERE e/ehr_id/value = '{EHR_ID}'"),
    );
}

// conformance: CP-10 CP-32
#[test]
fn an_undirected_function_outside_aql_in_where_is_refused() {
    let aql = format!("SELECT c/uid/value {FROM} WHERE PERCENTILE({MAGNITUDE}, 90) > 120");
    let refused = refusal(&aql, &declared());
    assert!(
        matches!(refused, Refusal::UndefinedFunction { at: Some(_) }),
        "{refused:?}"
    );
    let one = directed(NonZeroUsize::MIN);
    let analysis = analysed(&aql, &one).unwrap();
    assert_same_aql(&unscoped_node_aql(&analysis), &aql);
}

// conformance: CP-10 CP-32
#[test]
fn a_function_outside_aql_nested_in_an_aql_function_is_refused() {
    let aql = format!("SELECT ROUND(MEDIAN({MAGNITUDE}), 1) {FROM}");
    assert!(matches!(
        refusal(&aql, &declared()),
        Refusal::UndefinedFunction { .. }
    ));
}

// conformance: CP-10 CP-32
#[test]
fn a_function_outside_aql_is_refused_before_the_aggregate_beside_it() {
    let aql = format!("SELECT COUNT(*), MEDIAN({MAGNITUDE}) {FROM}");
    assert!(matches!(
        refusal(&aql, &declared()),
        Refusal::UndefinedFunction { .. }
    ));
}

// conformance: CP-10 CP-32
#[test]
fn the_refusal_suggests_both_alternatives() {
    let refused = refusal(&format!("SELECT MEDIAN({MAGNITUDE}) {FROM}"), &declared());
    let text = refused.to_string();
    assert!(text.contains("direct the query to one node"), "{text}");
    assert!(text.contains("select the rows"), "{text}");
}

// ── the single-row functions AQL 1.1.0 defines ───────────────────────────────

// conformance: CP-32
#[test]
fn a_string_function_is_forwarded_across_a_fan_out() {
    forwarded_as_rows(&format!("SELECT LENGTH(c/name/value) {FROM}"));
    forwarded_as_rows(&format!(
        "SELECT c/uid/value {FROM} WHERE CONTAINS(c/name/value, 'pressure') = true"
    ));
}

// conformance: CP-32
#[test]
fn a_numeric_function_is_forwarded_across_a_fan_out() {
    forwarded_as_rows(&format!("SELECT ROUND({MAGNITUDE}, 1) {FROM}"));
    forwarded_as_rows(&format!(
        "SELECT c/uid/value {FROM} WHERE ABS({MAGNITUDE}) > 10"
    ));
}

// conformance: CP-32
#[test]
fn a_date_and_time_function_is_forwarded_across_a_fan_out() {
    forwarded_as_rows(&format!("SELECT c/uid/value, CURRENT_DATE() {FROM}"));
    forwarded_as_rows(&format!(
        "SELECT c/uid/value {FROM} WHERE c/context/start_time/value < NOW()"
    ));
}

// conformance: CP-32
#[test]
fn the_terminology_function_is_forwarded_across_a_fan_out() {
    forwarded_as_rows(&format!(
        "SELECT c/uid/value {FROM} WHERE TERMINOLOGY('validate', 'hl7.org/fhir/4.0', \
         'system=http://snomed.info/sct&code=122298005') = true"
    ));
}

// conformance: CP-32
#[test]
fn a_function_name_is_read_in_any_case() {
    forwarded_as_rows(&format!("SELECT length(c/name/value) {FROM}"));
    forwarded_as_rows(&format!("SELECT Current_Date_Time() {FROM}"));
}

// ── the aggregate functions AQL 1.1.0 defines ────────────────────────────────

// conformance: CP-10 CP-32
#[test]
fn an_aggregate_follows_the_decomposition_rules() {
    let aql = format!("SELECT COUNT(*) {FROM}");
    let recombined = analysed(&aql, &declared()).unwrap();
    assert_eq!(
        recombined.recombination().map(|r| r.columns().to_vec()),
        Some(vec![Recombine::Count { column: 0 }]),
        "§11.6.3: a declared aggregate is recombined"
    );
    assert!(
        matches!(
            refusal(&aql, &ask_all()),
            Refusal::UndirectedAggregate { .. }
        ),
        "N14: an undeclared one is refused"
    );
}
