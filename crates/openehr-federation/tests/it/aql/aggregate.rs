// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Aggregates across a fan-out (§11.6.3, N14, N39): an undirected aggregate
//! is refused unless every function it applies is declared decomposable, a
//! declared one is dispatched to every node with `AVG` asked as its `SUM`
//! and `COUNT`, and a query that breaks the decomposition is refused naming
//! both alternatives. A directed single-node aggregate is dispatched
//! unchanged.

use std::num::NonZeroUsize;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aggregate::{AggregateFunction, Recombination, Recombine};
use openehr_federation::aql::refusal::{Indecomposable, Refusal};
use openehr_federation::aql::{Analysis, Context, Targeting};

use super::{EHR_ID, NAMESPACE, analysed, ask_all, assert_same_aql};

const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value = '4711'";
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
const MAGNITUDE: &str = "o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude";

/// An ask-all deployment that declares every function decomposable, as the
/// server does by default.
fn declared() -> Context {
    ask_all().with_decomposable_aggregates(AggregateFunction::ALL)
}

/// The patient query selecting `select`.
fn aggregate(select: &str) -> String {
    format!("SELECT {select} {FROM} WHERE {SUBJECT}")
}

/// The node query and the recombination of `aql` under `context`.
fn recombined(aql: &str, context: &Context) -> (String, Recombination) {
    let analysis = analysed(aql, context).unwrap_or_else(|refusal| panic!("{aql}: {refusal}"));
    let recombination = analysis
        .recombination()
        .cloned()
        .unwrap_or_else(|| panic!("{aql} is recombined"));
    let Analysis::Patient(query) = analysis else {
        panic!("{aql} names a patient");
    };
    let node = query.for_node(&HierObjectId::new(EHR_ID).unwrap());
    (node.aql().to_owned(), recombination)
}

fn refusal(aql: &str, context: &Context) -> Refusal {
    match analysed(aql, context) {
        Err(refusal) => refusal,
        Ok(analysis) => panic!("{aql} should be refused, got {analysis:?}"),
    }
}

fn scoped(select: &str) -> String {
    format!("SELECT {select} {FROM} WHERE e/ehr_id/value = '{EHR_ID}'")
}

// conformance: CP-10 CP-32
#[test]
fn every_declared_function_but_avg_is_dispatched_as_written() {
    let cases = [
        ("COUNT(*)".to_owned(), Recombine::Count { column: 0 }),
        (
            "COUNT(c/uid/value)".to_owned(),
            Recombine::Count { column: 0 },
        ),
        (format!("SUM({MAGNITUDE})"), Recombine::Sum { column: 0 }),
        (format!("MIN({MAGNITUDE})"), Recombine::Min { column: 0 }),
        (format!("MAX({MAGNITUDE})"), Recombine::Max { column: 0 }),
    ];
    for (select, expected) in &cases {
        let (select, expected) = (select.as_str(), *expected);
        let (node, recombination) = recombined(&aggregate(select), &declared());
        assert_same_aql(&node, &scoped(select));
        assert_eq!(
            recombination.columns(),
            [expected],
            "§11.6.3: {select} is recombined"
        );
    }
}

// conformance: CP-10 CP-32
#[test]
fn avg_is_asked_of_each_node_as_its_sum_and_count() {
    let (node, recombination) = recombined(
        &aggregate(&format!("AVG({MAGNITUDE}) AS mean")),
        &declared(),
    );
    assert_same_aql(
        &node,
        &scoped(&format!("SUM({MAGNITUDE}), COUNT({MAGNITUDE})")),
    );
    assert_eq!(
        recombination.columns(),
        [Recombine::Avg { sum: 0, count: 1 }],
        "§11.6.3: AVG only with the per-node counts"
    );
}

// conformance: CP-10 CP-32
#[test]
fn several_aggregates_map_to_their_node_columns() {
    let select =
        format!("COUNT(*) AS n, AVG({MAGNITUDE}) AS mean, MAX(c/context/start_time/value)");
    let (node, recombination) = recombined(&aggregate(&select), &declared());
    assert_same_aql(
        &node,
        &scoped(&format!(
            "COUNT(*) AS n, SUM({MAGNITUDE}), COUNT({MAGNITUDE}), MAX(c/context/start_time/value)"
        )),
    );
    assert_eq!(
        recombination.columns(),
        [
            Recombine::Count { column: 0 },
            Recombine::Avg { sum: 1, count: 2 },
            Recombine::Max { column: 3 },
        ]
    );
}

// conformance: CP-10 CP-32
#[test]
fn the_columns_are_the_clients_query_not_the_dispatched_one() {
    let analysis = analysed(
        &aggregate(&format!("AVG({MAGNITUDE}) AS mean, COUNT(*)")),
        &declared(),
    )
    .unwrap();
    let names: Vec<&str> = analysis.columns().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["mean", "#1"], "N17, §9.2: two façade columns");
}

// conformance: CP-10
#[test]
fn an_aggregate_naming_no_patient_is_recombined_across_every_member() {
    let analysis = analysed("SELECT COUNT(e/ehr_id/value) FROM EHR e", &declared()).unwrap();
    assert_eq!(
        analysis.recombination().map(Recombination::columns),
        Some(&[Recombine::Count { column: 0 }][..])
    );
    let Analysis::Unscoped(query) = analysis else {
        panic!("no patient");
    };
    assert_same_aql(
        query.node_query().aql(),
        "SELECT COUNT(e/ehr_id/value) FROM EHR e",
    );
}

// conformance: CP-10 CP-32
#[test]
fn an_aggregate_directed_to_two_endpoints_is_recombined() {
    let two = Context::new(Targeting::Directed {
        endpoints: NonZeroUsize::MIN.saturating_add(1),
    })
    .with_default_namespace(NAMESPACE)
    .with_decomposable_aggregates(AggregateFunction::ALL);
    let (_, recombination) = recombined(&aggregate("COUNT(*)"), &two);
    assert_eq!(recombination.columns(), [Recombine::Count { column: 0 }]);
}

// conformance: CP-10
#[test]
fn a_directed_single_node_aggregate_is_dispatched_unchanged() {
    let one = Context::new(Targeting::Directed {
        endpoints: NonZeroUsize::MIN,
    })
    .with_default_namespace(NAMESPACE)
    .with_decomposable_aggregates(AggregateFunction::ALL);
    let select = format!("AVG({MAGNITUDE}) AS mean");
    let analysis = analysed(&aggregate(&select), &one).unwrap();
    assert!(
        analysis.recombination().is_none(),
        "N14: the node's answer is the federated one"
    );
    assert_eq!(analysis.admit_best_effort(), Ok(()));
    let Analysis::Patient(query) = analysis else {
        panic!("a patient query");
    };
    let node = query.for_node(&HierObjectId::new(EHR_ID).unwrap());
    assert_same_aql(node.aql(), &scoped(&select));
}

// conformance: CP-10 CP-32
#[test]
fn with_nothing_declared_every_undirected_aggregate_is_refused() {
    for select in [
        "COUNT(*)".to_owned(),
        format!("SUM({MAGNITUDE})"),
        format!("MIN({MAGNITUDE})"),
        format!("MAX({MAGNITUDE})"),
        format!("AVG({MAGNITUDE})"),
    ] {
        let refused = refusal(&aggregate(&select), &ask_all());
        assert!(
            matches!(refused, Refusal::UndirectedAggregate { .. }),
            "N14: {select} -> {refused:?}"
        );
        assert_eq!(refused.kind(), "undirected-aggregate");
    }
}

// conformance: CP-10 CP-32
#[test]
fn a_function_left_out_of_the_declaration_is_refused() {
    // The OPTIONS example of §7a.2 declares COUNT, SUM, MIN and MAX: AVG is
    // then not decomposed, as §11.6.3 allows only with the per-node counts.
    let without_avg = ask_all().with_decomposable_aggregates([
        AggregateFunction::Count,
        AggregateFunction::Sum,
        AggregateFunction::Min,
        AggregateFunction::Max,
    ]);
    let select = format!("COUNT(*), AVG({MAGNITUDE})");
    let aql = aggregate(&select);
    let refused = refusal(&aql, &without_avg);
    let Refusal::UndirectedAggregate { at: Some(at) } = &refused else {
        panic!("N14: {refused:?}");
    };
    assert_eq!(
        aql.get(at.clone()),
        Some(MAGNITUDE),
        "the refusal points at the undeclared aggregate"
    );
    for function in AggregateFunction::ALL {
        let others = AggregateFunction::ALL
            .into_iter()
            .filter(|f| *f != function);
        let context = ask_all().with_decomposable_aggregates(others);
        let call = match function {
            AggregateFunction::Count => "COUNT(*)".to_owned(),
            other => format!("{}({MAGNITUDE})", other.name()),
        };
        assert!(
            matches!(
                refusal(&aggregate(&call), &context),
                Refusal::UndirectedAggregate { .. }
            ),
            "{call} without {} declared",
            function.name()
        );
    }
}

/// The reason a declared aggregate query is refused for.
fn indecomposable(aql: &str) -> Indecomposable {
    match refusal(aql, &declared()) {
        Refusal::Indecomposable { reason, .. } => reason,
        other => panic!("{aql}: expected an indecomposable aggregate, got {other:?}"),
    }
}

// conformance: CP-10 CP-32
#[test]
fn distinct_breaks_the_decomposition() {
    assert_eq!(
        indecomposable(&format!("SELECT DISTINCT COUNT(*) {FROM} WHERE {SUBJECT}")),
        Indecomposable::Distinct,
        "§11.6.3: never with DISTINCT"
    );
}

// conformance: CP-10 CP-32
#[test]
fn a_distinct_count_does_not_decompose() {
    assert_eq!(
        indecomposable(&aggregate("COUNT(DISTINCT c/uid/value)")),
        Indecomposable::CountDistinct,
        "a value counted at two nodes is one value"
    );
}

// conformance: CP-10 CP-32
#[test]
fn a_plain_column_beside_an_aggregate_is_refused() {
    for column in [
        "c/uid/value",
        "e/ehr_status/subject/external_ref/id/value",
        "'marker'",
        "LENGTH(c/name/value)",
    ] {
        assert_eq!(
            indecomposable(&aggregate(&format!("{column}, COUNT(*)"))),
            Indecomposable::PlainColumn,
            "§11.6.3: {column} would group the rows across nodes"
        );
    }
}

// conformance: CP-10 CP-32
#[test]
fn an_indecomposable_aggregate_suggests_both_alternatives() {
    let refused = refusal(&aggregate("COUNT(DISTINCT c/uid/value)"), &declared());
    let text = refused.to_string();
    assert!(text.contains("direct the query to one node"), "{text}");
    assert!(
        text.contains("select the rows and aggregate them"),
        "{text}"
    );
    assert_eq!(refused.kind(), "indecomposable-aggregate");
    assert!(refused.at().is_some(), "it points at the aggregate");
}

// conformance: CP-10 CP-32
#[test]
fn a_recombined_aggregate_is_never_answered_best_effort() {
    let analysis = analysed(&aggregate("COUNT(*)"), &declared()).unwrap();
    let refused = analysis.admit_best_effort().unwrap_err();
    assert_eq!(refused, Refusal::PartialAggregate, "§11.6.3, §11.4");
    assert_eq!(refused.kind(), "partial-aggregate");
    let rows = analysed(&aggregate("c/uid/value"), &declared()).unwrap();
    assert_eq!(rows.admit_best_effort(), Ok(()), "rows may be partial");
}

#[test]
fn the_declared_functions_are_listed_in_declaration_order() {
    let context = ask_all().with_decomposable_aggregates([
        AggregateFunction::Avg,
        AggregateFunction::Count,
        AggregateFunction::Count,
    ]);
    let names: Vec<&str> = context
        .decomposable_aggregates()
        .iter()
        .map(|function| function.name())
        .collect();
    assert_eq!(names, ["COUNT", "AVG"], "§7a.2 aggregates.decomposable");
    assert!(ask_all().decomposable_aggregates().is_empty());
}
