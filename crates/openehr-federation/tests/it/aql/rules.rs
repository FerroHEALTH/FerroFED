// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroFED's own strict corpus for the rewrite (where no specification
//! governs a case, it is our own design): the reduction constraint of §7.1, the
//! re-injection of N5 and the `columns[]` of N17. The wrapping of a query with
//! no `EHR` containment (N7) is asserted beside the rewrite it governs.

use std::num::NonZeroUsize;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aql::refusal::{Refusal, Unreducible};
use openehr_federation::aql::subject::NamespaceOrigin;
use openehr_federation::aql::{Analysis, ColumnSource, Context, Paging, Targeting, analyse};
use openehr_query::ast::Primitive;
use openehr_query::bind::Parameters;

use super::{EHR_ID, NAMESPACE, analysed, ask_all, assert_same_aql, node_aql, patient, refused};

const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value";
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c";

fn query(select: &str, where_: &str) -> String {
    format!("SELECT {select} {FROM} WHERE {where_}")
}

fn unreducible(refusal: &Refusal) -> Unreducible {
    match refusal {
        Refusal::Unreducible { reason, .. } => *reason,
        other => panic!("expected an unreducible predicate, got {other:?}"),
    }
}

// ── §5.2: the issuing namespace ─────────────────────────────────────────────

#[test]
fn an_unqualified_identifier_is_refused_when_no_default_namespace_is_configured() {
    let no_default = Context::new(Targeting::AskAll);
    let refusal = analysed(
        &query("c/uid/value", &format!("{SUBJECT} = '4711'")),
        &no_default,
    )
    .unwrap_err();
    assert_eq!(refusal, Refusal::NoNamespace, "§5.2 requires the namespace");
}

#[test]
fn an_unqualified_identifier_resolves_in_the_declared_default_namespace() {
    let query = patient(&query("c/uid/value", &format!("{SUBJECT} = '4711'")));
    assert_eq!(
        query.subject().namespace(),
        NAMESPACE,
        "the declared default applies"
    );
    assert_eq!(
        query.subject().namespace_origin(),
        NamespaceOrigin::Default,
        "and is reported as the default"
    );
}

#[test]
fn the_namespace_predicate_is_consumed_as_resolution_input() {
    let aql = query(
        "c/uid/value",
        &format!(
            "{SUBJECT} = '4711' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.7'"
        ),
    );
    let query = patient(&aql);
    assert_eq!(
        query.subject().namespace(),
        "urn:oid:2.999.7",
        "the query's namespace wins over the default"
    );
    assert_eq!(
        query.subject().namespace_origin(),
        NamespaceOrigin::Query,
        "and is reported as the query's"
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!("SELECT c/uid/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}'"),
    );
}

// ── AQL §Parameters: the identifier is a string ─────────────────────────────

#[test]
fn an_integer_written_on_the_identifier_path_is_refused_never_coerced() {
    let refusal = refused(&query("c/uid/value", &format!("{SUBJECT} = 4711")));
    assert!(
        matches!(refusal, Refusal::IdentifierNotString { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn an_integer_bound_to_the_identifier_path_is_refused_never_coerced() {
    let mut parameters = Parameters::new();
    parameters.insert("patient", Primitive::Integer(4711));
    let refusal = analyse(
        &query("c/uid/value", &format!("{SUBJECT} = $patient")),
        &parameters,
        Paging::default(),
        &ask_all(),
    )
    .unwrap_err();
    assert!(
        matches!(refusal, Refusal::IdentifierNotString { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn a_string_bound_to_the_identifier_path_is_resolution_input() {
    let mut parameters = Parameters::new();
    parameters.insert("patient", Primitive::String("4711".into()));
    let analysis = analyse(
        &query("c/uid/value", &format!("{SUBJECT} = $patient")),
        &parameters,
        Paging::default(),
        &ask_all(),
    )
    .unwrap();
    let Analysis::Patient(query) = analysis else {
        panic!("a bound patient predicate names the patient")
    };
    assert_eq!(
        query.subject().value(),
        "4711",
        "the bound value is the identifier"
    );
}

#[test]
fn a_non_string_namespace_is_refused() {
    let refusal = refused(&query(
        "c/uid/value",
        &format!("{SUBJECT} = '4711' AND e/ehr_status/subject/external_ref/namespace = 7"),
    ));
    assert!(
        matches!(refusal, Refusal::IdentifierNotString { .. }),
        "got {refusal:?}"
    );
}

// ── §7.1: one patient ───────────────────────────────────────────────────────

#[test]
fn the_same_identifier_twice_is_consumed_once() {
    let aql = query(
        "c/uid/value",
        &format!("{SUBJECT} = '4711' AND {SUBJECT} = '4711'"),
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!("SELECT c/uid/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}'"),
    );
}

#[test]
fn two_different_identifiers_are_refused() {
    let refusal = refused(&query(
        "c/uid/value",
        &format!("{SUBJECT} = '4711' AND {SUBJECT} = '4712'"),
    ));
    assert!(
        matches!(refusal, Refusal::SecondSubject { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn two_different_namespaces_are_refused() {
    let ns = "e/ehr_status/subject/external_ref/namespace";
    let refusal = refused(&query(
        "c/uid/value",
        &format!("{SUBJECT} = '4711' AND {ns} = 'urn:oid:2.999.7' AND {ns} = 'urn:oid:2.999.8'"),
    ));
    assert!(
        matches!(refusal, Refusal::SecondNamespace { .. }),
        "got {refusal:?}"
    );
}

// ── N4: a query with no patient ─────────────────────────────────────────────

const NO_PATIENT: &str =
    "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = 'Visit'";

#[test]
fn a_query_with_no_patient_is_refused_where_a_localizer_decides_the_node_set() {
    let localized = Context::new(Targeting::Localized).with_default_namespace(NAMESPACE);
    assert_eq!(
        analysed(NO_PATIENT, &localized).unwrap_err(),
        Refusal::NodeSetUndefined,
        "N4 keys localization on the patient"
    );
}

#[test]
fn a_query_with_no_patient_is_dispatched_as_written_where_every_member_is_asked() {
    let Analysis::Unscoped(query) = analysed(NO_PATIENT, &ask_all()).unwrap() else {
        panic!("the query names no patient")
    };
    assert_same_aql(query.node_query().aql(), NO_PATIENT);
}

#[test]
fn a_query_with_no_patient_is_dispatched_as_written_to_the_endpoints_it_names() {
    let directed = Context::new(Targeting::Directed {
        endpoints: NonZeroUsize::MIN.saturating_add(1),
    });
    assert!(
        matches!(analysed(NO_PATIENT, &directed), Ok(Analysis::Unscoped(_))),
        "a directed query defines its node set"
    );
}

#[test]
fn a_query_already_scoped_by_ehr_id_is_accepted_as_written() {
    let aql = format!("SELECT c/uid/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}'");
    let Analysis::Unscoped(query) = analysed(&aql, &ask_all()).unwrap() else {
        panic!("no patient is named")
    };
    assert!(
        query.ehr_scoped(),
        "N29: the canonical ehr_id form is recognised"
    );
    assert_same_aql(query.node_query().aql(), &aql);
}

#[test]
fn a_namespace_predicate_without_an_identifier_is_ordinary_query_material() {
    let aql = format!(
        "SELECT c/uid/value {FROM} WHERE e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.7'"
    );
    let Analysis::Unscoped(query) = analysed(&aql, &ask_all()).unwrap() else {
        panic!("no patient is named")
    };
    assert_same_aql(query.node_query().aql(), &aql);
}

// ── §11.6: the ITS-REST paging members ──────────────────────────────────────

fn with_paging(aql: &str, offset: Option<i64>, fetch: Option<i64>) -> Result<Analysis, Refusal> {
    analyse(
        aql,
        &Parameters::new(),
        Paging { offset, fetch },
        &ask_all(),
    )
}

#[test]
fn the_fetch_member_pages_like_limit() {
    let aql = query("c/uid/value", &format!("{SUBJECT} = '4711'"));
    let Analysis::Patient(query) = with_paging(&aql, None, Some(10)).unwrap() else {
        panic!("a patient query")
    };
    let node = query.for_node(&HierObjectId::new(EHR_ID).unwrap());
    assert_same_aql(
        node.aql(),
        &format!("SELECT c/uid/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}' LIMIT 10"),
    );
}

#[test]
fn a_fetch_member_that_agrees_with_limit_is_accepted() {
    let aql = format!(
        "{} LIMIT 10",
        query("c/uid/value", &format!("{SUBJECT} = '4711'"))
    );
    assert!(
        with_paging(&aql, None, Some(10)).is_ok(),
        "the same page twice is one page"
    );
}

#[test]
fn a_fetch_member_that_disagrees_with_limit_is_refused() {
    let aql = format!(
        "{} LIMIT 10",
        query("c/uid/value", &format!("{SUBJECT} = '4711'"))
    );
    let refusal = with_paging(&aql, None, Some(20)).unwrap_err();
    assert_eq!(
        refusal,
        Refusal::PagingConflict {
            member: "fetch",
            clause: "LIMIT"
        },
        "the member and the clause page alike (§11.6)"
    );
}

#[test]
fn an_offset_member_past_zero_is_refused_like_offset() {
    let aql = query("c/uid/value", &format!("{SUBJECT} = '4711'"));
    assert_eq!(
        with_paging(&aql, Some(5), Some(10)).unwrap_err(),
        Refusal::OffsetUnsupported,
        "§11.6.2, N39"
    );
}

#[test]
fn an_offset_member_that_disagrees_with_offset_is_refused() {
    let aql = format!(
        "{} LIMIT 10 OFFSET 5",
        query("c/uid/value", &format!("{SUBJECT} = '4711'"))
    );
    let refusal = with_paging(&aql, Some(6), None).unwrap_err();
    assert_eq!(
        refusal,
        Refusal::PagingConflict {
            member: "offset",
            clause: "OFFSET"
        },
        "the member and the clause page alike (§11.6)"
    );
}

#[test]
fn a_zero_offset_skips_nothing_and_is_never_pushed_down() {
    let aql = format!(
        "{} LIMIT 10 OFFSET 0",
        query("c/uid/value", &format!("{SUBJECT} = '4711'"))
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!("SELECT c/uid/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}' LIMIT 10"),
    );
}

#[test]
fn a_negative_paging_member_is_refused() {
    let aql = query("c/uid/value", &format!("{SUBJECT} = '4711'"));
    assert_eq!(
        with_paging(&aql, None, Some(-1)).unwrap_err(),
        Refusal::NegativePaging { member: "fetch" },
        "fetch is a count"
    );
    assert_eq!(
        with_paging(&aql, Some(-1), None).unwrap_err(),
        Refusal::NegativePaging { member: "offset" },
        "offset is a count"
    );
}

// ── §7.1 reduction constraint and §5.4.2 leak paths ─────────────────────────

#[test]
fn a_subject_predicate_under_or_with_a_non_subject_term_is_refused_with_a_reason() {
    let refusal = refused(&query(
        "c/uid/value",
        &format!("{SUBJECT} = '4711' OR c/name/value = 'Visit'"),
    ));
    assert_eq!(
        unreducible(&refusal),
        Unreducible::NotConjunctive,
        "§7.1 reduction constraint"
    );
    assert!(
        refusal.to_string().contains("OR or NOT"),
        "the 400 carries its reason: {refusal}"
    );
}

#[test]
fn a_subject_predicate_under_not_is_refused() {
    let refusal = refused(&query("c/uid/value", &format!("NOT {SUBJECT} = '4711'")));
    assert_eq!(unreducible(&refusal), Unreducible::NotConjunctive, "§7.1");
}

#[test]
fn a_subject_predicate_with_another_operator_is_refused() {
    for op in ["!=", "<", ">", "<=", ">="] {
        let refusal = refused(&query("c/uid/value", &format!("{SUBJECT} {op} '4711'")));
        assert_eq!(
            unreducible(&refusal),
            Unreducible::NotEquality,
            "operator {op}"
        );
    }
}

#[test]
fn a_subject_path_under_matches_or_exists_is_refused() {
    for where_ in [
        format!("{SUBJECT} MATCHES {{'4711', '4712'}}"),
        format!("EXISTS {SUBJECT}"),
    ] {
        let refusal = refused(&query("c/uid/value", &where_));
        assert_eq!(
            unreducible(&refusal),
            Unreducible::NotEquality,
            "condition {where_}"
        );
    }
}

#[test]
fn a_subject_compared_with_a_path_is_refused() {
    let refusal = refused(&query("c/uid/value", &format!("{SUBJECT} = c/name/value")));
    assert_eq!(
        unreducible(&refusal),
        Unreducible::NotALiteral,
        "no literal, no resolution input"
    );
}

#[test]
fn another_attribute_of_the_subject_is_refused() {
    let refusal = refused(&query(
        "c/uid/value",
        "e/ehr_status/subject/external_ref/id/scheme = 'local'",
    ));
    assert_eq!(
        unreducible(&refusal),
        Unreducible::OtherSubjectPath,
        "§5.4.3"
    );
}

#[test]
fn a_subject_path_inside_a_function_is_refused() {
    let refusal = refused(&query("c/uid/value", &format!("LENGTH({SUBJECT}) = 4")));
    assert_eq!(
        unreducible(&refusal),
        Unreducible::InsideAnExpression,
        "§5.4.2"
    );
}

#[test]
fn a_query_binding_two_ehrs_is_refused_when_it_names_a_patient() {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c AND EHR f WHERE {SUBJECT} = '4711'"
    );
    assert_eq!(
        unreducible(&refused(&aql)),
        Unreducible::SeveralEhrs,
        "§7.1: one ehr_id scope per node"
    );
}

#[test]
fn an_empty_identifier_is_refused() {
    let refusal = refused(&query("c/uid/value", &format!("{SUBJECT} = ''")));
    assert!(
        matches!(refusal, Refusal::EmptyIdentifier { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn ordering_on_the_subject_is_refused() {
    let aql = format!(
        "{} ORDER BY {SUBJECT}",
        query("c/uid/value", &format!("{SUBJECT} = '4711'"))
    );
    assert!(
        matches!(refused(&aql), Refusal::SubjectOrdering { .. }),
        "§5.4.2 names ORDER BY a leak path"
    );
}

#[test]
fn the_identifier_bound_in_another_position_is_refused() {
    let mut parameters = Parameters::new();
    parameters.insert("p", Primitive::String("4711".into()));
    let aql = query(
        "c/uid/value",
        &format!("{SUBJECT} = '4711' AND c/name/value = $p"),
    );
    let refusal = analyse(&aql, &parameters, Paging::default(), &ask_all()).unwrap_err();
    assert!(
        matches!(refusal, Refusal::IdentifierElsewhere { .. }),
        "§5.4.1: in any position, got {refusal:?}"
    );
}

#[test]
fn a_clinician_identifier_with_another_value_is_dispatched() {
    let aql = query(
        "c/uid/value",
        &format!("{SUBJECT} = '4711' AND c/composer/identifiers/id = '9001'"),
    );
    let expected = format!(
        "SELECT c/uid/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}' AND c/composer/identifiers/id = '9001'"
    );
    assert_same_aql(&node_aql(&aql), &expected);
}

#[test]
fn a_query_with_the_parser_fix_of_ferroehr_3513_binds_and_analyses() {
    let mut parameters = Parameters::new();
    parameters.insert(
        "t",
        Primitive::String("openEHR-EHR-COMPOSITION.encounter.v1".into()),
    );
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c[$t] WHERE {SUBJECT} = '4711'"
    );
    assert!(
        analyse(&aql, &parameters, Paging::default(), &ask_all()).is_ok(),
        "a bound archetype predicate is ordinary material"
    );
}

// ── N14: aggregates ─────────────────────────────────────────────────────────

const AGGREGATE: &str = "SELECT COUNT(c/uid/value) FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '4711'";

// conformance: CP-10 CP-32
#[test]
fn an_undirected_aggregate_is_refused_with_its_reason() {
    let refusal = refused(AGGREGATE);
    assert!(
        matches!(refusal, Refusal::UndirectedAggregate { .. }),
        "N14, got {refusal:?}"
    );
    assert!(
        refusal.to_string().contains("direct the query to one node"),
        "§11.6.3 suggests the alternatives: {refusal}"
    );
}

// conformance: CP-10
#[test]
fn an_aggregate_directed_to_one_endpoint_is_dispatched_unchanged() {
    let one = Context::new(Targeting::Directed {
        endpoints: NonZeroUsize::MIN,
    })
    .with_default_namespace(NAMESPACE);
    assert!(
        matches!(analysed(AGGREGATE, &one), Ok(Analysis::Patient(_))),
        "N14: a directed single-node aggregate is permitted"
    );
}

// conformance: CP-10
#[test]
fn an_aggregate_directed_to_two_endpoints_is_refused() {
    let two = Context::new(Targeting::Directed {
        endpoints: NonZeroUsize::MIN.saturating_add(1),
    });
    assert!(
        matches!(
            analysed(AGGREGATE, &two),
            Err(Refusal::UndirectedAggregate { .. })
        ),
        "N14"
    );
}

// ── N5 re-injection and N17 columns ─────────────────────────────────────────

#[test]
fn a_selected_subject_column_is_re_injected_never_asked_of_the_node() {
    let aql = query(
        &format!("{SUBJECT} AS patient, c/uid/value"),
        &format!("{SUBJECT} = '4711'"),
    );
    let query = patient(&aql);
    let node = query.for_node(&HierObjectId::new(EHR_ID).unwrap());
    assert_eq!(
        node.columns(),
        [ColumnSource::Subject, ColumnSource::Node(0)],
        "N5: the input, not node data"
    );
    assert_same_aql(
        node.aql(),
        &format!("SELECT c/uid/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}'"),
    );
}

#[test]
fn a_query_selecting_only_the_subject_asks_each_node_for_its_ehr_id() {
    let aql = query(SUBJECT, &format!("{SUBJECT} = '4711'"));
    let node = patient(&aql).for_node(&HierObjectId::new(EHR_ID).unwrap());
    assert_eq!(
        node.columns(),
        [ColumnSource::Subject],
        "the only column is the re-injected input"
    );
    assert_same_aql(
        node.aql(),
        &format!("SELECT e/ehr_id/value {FROM} WHERE e/ehr_id/value = '{EHR_ID}'"),
    );
}

#[test]
fn a_selected_namespace_column_is_re_injected() {
    let aql = query(
        "e/ehr_status/subject/external_ref/namespace, c/uid/value",
        &format!("{SUBJECT} = '4711'"),
    );
    let node = patient(&aql).for_node(&HierObjectId::new(EHR_ID).unwrap());
    assert_eq!(
        node.columns(),
        [ColumnSource::Namespace, ColumnSource::Node(0)],
        "the namespace is resolution input too"
    );
}

#[test]
fn a_subject_object_the_gateway_cannot_re_inject_is_refused() {
    let refusal = refused(&query(
        "e/ehr_status/subject, c/uid/value",
        &format!("{SUBJECT} = '4711'"),
    ));
    assert!(
        matches!(refusal, Refusal::SubjectProjection { .. }),
        "N5, got {refusal:?}"
    );
}

#[test]
fn the_subject_column_without_a_patient_predicate_is_refused() {
    let refusal = refused(&format!("SELECT {SUBJECT}, c/uid/value {FROM}"));
    assert!(
        matches!(refusal, Refusal::SubjectWithoutPredicate { .. }),
        "N5, got {refusal:?}"
    );
}

#[test]
fn columns_are_the_gateways_rendering_of_the_facade_query_whatever_the_node() {
    let aql = query(
        &format!("{SUBJECT} AS patient, c/uid/value, c/context/start_time/value AS start"),
        &format!("{SUBJECT} = '4711'"),
    );
    let query = patient(&aql);
    let rendered = |columns: &[openehr_its::rest::generated::query::ResultSetColumn]| -> Vec<(String, Option<String>)> {
        columns.iter().map(|c| (c.name.clone(), c.path.clone())).collect()
    };
    let names: Vec<_> = query
        .columns()
        .iter()
        .map(|c| (c.name.as_str(), c.path.as_deref()))
        .collect();
    assert_eq!(
        names,
        [
            ("patient", Some("/ehr_status/subject/external_ref/id/value")),
            ("#1", Some("/uid/value")),
            ("start", Some("/context/start_time/value")),
        ],
        "N17, §9.2"
    );
    let first = query.for_node(&HierObjectId::new(EHR_ID).unwrap());
    let second =
        query.for_node(&HierObjectId::new("0b1e7d1e-3f5a-4a2b-9c0d-5e6f7a8b9c0d").unwrap());
    assert_ne!(first.aql(), second.aql(), "each node gets its own scope");
    assert_eq!(
        first.columns(),
        second.columns(),
        "and the same column mapping"
    );
    assert_eq!(
        rendered(patient(&aql).columns()),
        rendered(query.columns()),
        "columns[] is a function of the façade query alone"
    );
}
