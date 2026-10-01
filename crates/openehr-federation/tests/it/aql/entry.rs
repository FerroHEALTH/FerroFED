// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ENTRY`-level patient carrier: a `subject` `PARTY_IDENTIFIED` /
//! `DV_IDENTIFIER` predicate is resolution input on equal terms with
//! `EHR_STATUS.subject.external_ref` (§5.4.3, N33, CP-38), and is consumed and
//! stripped exactly as that one is (§7.1). The refusals are FerroFED's strict
//! reading where the specification leaves a form open, each named below.

use openehr_federation::aql::refusal::{Refusal, Unreducible};
use openehr_federation::aql::subject::NamespaceOrigin;
use openehr_federation::aql::{Analysis, Context, Paging, Targeting, analyse};
use openehr_query::ast::Primitive;
use openehr_query::bind::Parameters;

use super::{NAMESPACE, analysed, ask_all, assert_same_aql, node_aql, patient, refused};

const EXTERNAL_REF: &str = "e/ehr_status/subject/external_ref/id/value";
const ENTRY_ID: &str = "o/subject/identifiers/id";
const ENTRY_ISSUER: &str = "o/subject/identifiers/issuer";
const ENTRY_TYPE: &str = "o/subject/identifiers/type";
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";

fn query(select: &str, where_: &str) -> String {
    format!("SELECT {select} {FROM} WHERE {where_}")
}

fn unreducible(refusal: &Refusal) -> Unreducible {
    match refusal {
        Refusal::Unreducible { reason, .. } => *reason,
        other => panic!("expected an unreducible predicate, got {other:?}"),
    }
}

// ── both carriers resolve, and rewrite to the same node query ──────────────

// conformance: CP-38
#[test]
fn both_carriers_rewrite_the_same_patient_query_to_the_same_node_query() {
    let via_external_ref = query(
        "c/uid/value",
        &format!(
            "{EXTERNAL_REF} = '4711' AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}' AND c/name/value = 'Visit'"
        ),
    );
    let via_entry = query(
        "c/uid/value",
        &format!(
            "{ENTRY_ID} = '4711' AND {ENTRY_ISSUER} = '{NAMESPACE}' AND c/name/value = 'Visit'"
        ),
    );
    assert_eq!(
        patient(&via_external_ref).subject(),
        patient(&via_entry).subject(),
        "both name the same patient in the same namespace (§5.2)"
    );
    assert_eq!(
        node_aql(&via_external_ref),
        node_aql(&via_entry),
        "and dispatch the same node query, so they return the same rows (CP-38)"
    );
    assert!(
        !node_aql(&via_entry).contains("subject"),
        "the node never receives the carrier (N33): {}",
        node_aql(&via_entry)
    );
}

#[test]
fn golden_case_13_rewrites_in_the_declared_default_namespace() {
    let aql = query("c/uid/value", &format!("{ENTRY_ID} = '12345'"));
    let analysed = patient(&aql);
    assert_eq!(analysed.subject().value(), "12345", "the identifier");
    assert_eq!(
        analysed.subject().namespace_origin(),
        NamespaceOrigin::Default,
        "golden case 13 names no namespace, so the declared default applies (§5.2, decision A5)"
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT c/uid/value {FROM} WHERE e/ehr_id/value = '{}'",
            super::EHR_ID
        ),
    );
}

#[test]
fn the_entry_carrier_without_any_namespace_is_refused_when_no_default_is_configured() {
    let no_default = Context::new(Targeting::AskAll);
    let refusal = analysed(
        &query("c/uid/value", &format!("{ENTRY_ID} = '4711'")),
        &no_default,
    )
    .unwrap_err();
    assert_eq!(refusal, Refusal::NoNamespace, "§5.2 requires the namespace");
}

#[test]
fn the_issuer_is_the_issuing_namespace() {
    let aql = query(
        "c/uid/value",
        &format!("{ENTRY_ID} = '4711' AND {ENTRY_ISSUER} = 'urn:oid:2.999.7'"),
    );
    let analysed = patient(&aql);
    assert_eq!(analysed.subject().namespace(), "urn:oid:2.999.7", "§5.4.3");
    assert_eq!(
        analysed.subject().namespace_origin(),
        NamespaceOrigin::Query,
        "named by the query"
    );
    assert!(
        !node_aql(&aql).contains("issuer"),
        "the qualifier is consumed with the value: {}",
        node_aql(&aql)
    );
}

#[test]
fn the_type_is_the_issuing_namespace_when_it_is_the_only_qualifier() {
    let aql = query(
        "c/uid/value",
        &format!("{ENTRY_ID} = '4711' AND {ENTRY_TYPE} = 'urn:oid:2.999.8'"),
    );
    assert_eq!(
        patient(&aql).subject().namespace(),
        "urn:oid:2.999.8",
        "§5.4.3"
    );
    assert!(
        !node_aql(&aql).contains("type"),
        "the qualifier is consumed with the value: {}",
        node_aql(&aql)
    );
}

#[test]
fn issuer_and_type_naming_the_same_namespace_reduce_to_it() {
    let aql = query(
        "c/uid/value",
        &format!(
            "{ENTRY_ID} = '4711' AND {ENTRY_ISSUER} = '{NAMESPACE}' AND {ENTRY_TYPE} = '{NAMESPACE}'"
        ),
    );
    assert_eq!(
        patient(&aql).subject().namespace(),
        NAMESPACE,
        "one namespace"
    );
}

#[test]
fn issuer_and_type_with_different_values_do_not_reduce_to_one_namespace() {
    // Golden case 14's adjudication on #18: the strict reading, a 400.
    let aql = query(
        "c/uid/value",
        &format!(
            "{ENTRY_ID} = '4711' AND {ENTRY_ISSUER} = 'urn:oid:2.999.7' AND {ENTRY_TYPE} = 'MR'"
        ),
    );
    assert!(
        matches!(refused(&aql), Refusal::SecondNamespace { .. }),
        "§5.2 names one issuing namespace"
    );
}

#[test]
fn two_issuers_are_refused() {
    let aql = query(
        "c/uid/value",
        &format!(
            "{ENTRY_ID} = '4711' AND {ENTRY_ISSUER} = 'urn:oid:2.999.7' AND {ENTRY_ISSUER} = 'urn:oid:2.999.8'"
        ),
    );
    assert!(
        matches!(refused(&aql), Refusal::SecondNamespace { .. }),
        "one patient, one namespace"
    );
}

#[test]
fn the_two_carriers_naming_different_namespaces_are_refused() {
    let aql = query(
        "c/uid/value",
        &format!(
            "{EXTERNAL_REF} = '4711' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.7' AND {ENTRY_ID} = '4711' AND {ENTRY_ISSUER} = 'urn:oid:2.999.8'"
        ),
    );
    assert!(
        matches!(refused(&aql), Refusal::SecondNamespace { .. }),
        "§5.2"
    );
}

// ── decision A7 across both carriers ────────────────────────────────────────

#[test]
fn the_same_value_in_both_carriers_is_consumed_once() {
    let aql = query(
        "c/uid/value",
        &format!("{EXTERNAL_REF} = '4711' AND {ENTRY_ID} = '4711'"),
    );
    let node = node_aql(&aql);
    assert!(
        !node.contains("4711") && !node.contains("subject"),
        "both leaves are consumed and stripped: {node}"
    );
}

#[test]
fn golden_case_17_a_second_value_in_the_entry_carrier_is_refused() {
    // The second value may name a relative (PARTY_RELATED), which no path tells
    // apart from the patient; refusing it is the reading that cannot leak.
    let aql = query(
        "c/uid/value",
        &format!("{EXTERNAL_REF} = '12345' AND {ENTRY_ID} = '999'"),
    );
    assert!(
        matches!(refused(&aql), Refusal::SecondSubject { .. }),
        "§7.1 reduction constraint, decision A7"
    );
}

#[test]
fn two_different_entry_values_are_refused() {
    let aql = query(
        "c/uid/value",
        &format!(
            "{ENTRY_ID} = '4711' AND c/content[openEHR-EHR-OBSERVATION.x.v1]/subject/identifiers/id = '999'"
        ),
    );
    assert!(
        matches!(refused(&aql), Refusal::SecondSubject { .. }),
        "decision A7"
    );
}

// ── the carrier is consumed in the paths a client writes ───────────────────

#[test]
fn the_carrier_reached_through_the_composition_content_is_consumed() {
    // The §5.4.2 example path.
    let aql = query(
        "c/uid/value",
        "c/content[openEHR-EHR-OBSERVATION.blood_pressure.v2]/subject/identifiers/id = '4711'",
    );
    let node = node_aql(&aql);
    assert!(
        !node.contains("4711") && !node.contains("subject"),
        "consumed and stripped: {node}"
    );
}

#[test]
fn a_bound_parameter_on_the_carrier_is_consumed_as_its_literal() {
    let mut parameters = Parameters::new();
    parameters.insert("patient", Primitive::String("4711".into()));
    let aql = query("c/uid/value", &format!("{ENTRY_ID} = $patient"));
    match analyse(&aql, &parameters, Paging::default(), &ask_all()) {
        Ok(Analysis::Patient(query)) => {
            assert_eq!(query.subject().value(), "4711", "AQL §Parameters");
        }
        other => panic!("expected a patient query, got {other:?}"),
    }
}

#[test]
fn a_query_with_no_ehr_containment_is_wrapped_for_the_entry_carrier() {
    // Decision A3, which §5.4.3 makes reachable through this carrier.
    let aql = format!(
        "SELECT c/uid/value FROM COMPOSITION c CONTAINS OBSERVATION o WHERE {ENTRY_ID} = '4711'"
    );
    assert_same_aql(
        &node_aql(&aql),
        &format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE e/ehr_id/value = '{}'",
            super::EHR_ID
        ),
    );
}

#[test]
fn a_qualifier_with_no_identifier_is_ordinary_query_material() {
    // It identifies no patient, as for the external_ref namespace (golden case 15 note).
    match analysed(
        &query("c/uid/value", &format!("{ENTRY_ISSUER} = '{NAMESPACE}'")),
        &ask_all(),
    ) {
        Ok(Analysis::Unscoped(query)) => assert!(
            query.node_query().aql().contains("issuer"),
            "dispatched as written: {}",
            query.node_query().aql()
        ),
        other => panic!("expected an unscoped query, got {other:?}"),
    }
}

// ── what cannot be consumed exactly is refused ──────────────────────────────

#[test]
fn the_carrier_under_or_is_refused() {
    let aql = query(
        "c/uid/value",
        &format!("{ENTRY_ID} = '4711' OR c/name/value = 'Visit'"),
    );
    assert_eq!(
        unreducible(&refused(&aql)),
        Unreducible::NotConjunctive,
        "§7.1"
    );
}

#[test]
fn the_carrier_under_a_pattern_is_refused() {
    let aql = query("c/uid/value", &format!("{ENTRY_ID} LIKE '47*'"));
    assert_eq!(
        unreducible(&refused(&aql)),
        Unreducible::NotEquality,
        "§7.1"
    );
}

#[test]
fn the_carrier_compared_with_a_path_is_refused() {
    let aql = query("c/uid/value", &format!("{ENTRY_ID} = c/name/value"));
    assert_eq!(
        unreducible(&refused(&aql)),
        Unreducible::NotALiteral,
        "§7.1"
    );
}

#[test]
fn an_integer_on_the_carrier_is_refused() {
    let aql = query("c/uid/value", &format!("{ENTRY_ID} = 4711"));
    assert!(
        matches!(refused(&aql), Refusal::IdentifierNotString { .. }),
        "DV_IDENTIFIER.id is a String (decision A6)"
    );
}

#[test]
fn the_assigner_is_not_a_namespace_and_is_refused() {
    // §5.4.3 names issuer and type as the namespace; assigner is not resolved on.
    let aql = query(
        "c/uid/value",
        &format!("{ENTRY_ID} = '4711' AND o/subject/identifiers/assigner = 'Clinic'"),
    );
    assert_eq!(
        unreducible(&refused(&aql)),
        Unreducible::OtherSubjectPath,
        "strict"
    );
}

#[test]
fn a_predicate_on_the_identifier_list_is_refused() {
    let aql = query(
        "c/uid/value",
        "o/subject/identifiers[type='MR']/id = '4711'",
    );
    assert_eq!(
        unreducible(&refused(&aql)),
        Unreducible::OtherSubjectPath,
        "strict"
    );
}

#[test]
fn an_entry_subject_external_ref_is_refused() {
    // §5.4.3 names the PARTY_IDENTIFIED / DV_IDENTIFIER form of the ENTRY carrier only.
    let aql = query("c/uid/value", "o/subject/external_ref/id/value = '4711'");
    assert_eq!(
        unreducible(&refused(&aql)),
        Unreducible::OtherSubjectPath,
        "strict"
    );
}

#[test]
fn the_entry_carrier_selected_is_refused() {
    // N5 re-injects the external_ref column; an ENTRY row may be about a relative.
    let aql = query(ENTRY_ID, &format!("{ENTRY_ID} = '4711'"));
    assert!(
        matches!(refused(&aql), Refusal::SubjectProjection { .. }),
        "§5.4.2, N5"
    );
}

#[test]
fn the_entry_carrier_ordered_on_is_refused() {
    let aql = format!(
        "{} ORDER BY {ENTRY_ID}",
        query("c/uid/value", &format!("{ENTRY_ID} = '4711'"))
    );
    assert!(
        matches!(refused(&aql), Refusal::SubjectOrdering { .. }),
        "§5.4.2"
    );
}

#[test]
fn the_entry_carrier_inside_a_function_is_refused() {
    let aql = query(
        "c/uid/value",
        &format!("{ENTRY_ID} = '4711' AND LENGTH({ENTRY_ID}) > 2"),
    );
    assert_eq!(
        unreducible(&refused(&aql)),
        Unreducible::InsideAnExpression,
        "§5.4.2"
    );
}

#[test]
fn the_resolved_value_on_another_path_is_still_refused() {
    // §5.4.3 note: the test is the value; golden case 16 through the ENTRY carrier.
    let aql = query(
        "c/uid/value",
        &format!("{ENTRY_ID} = '4711' AND c/composer/identifiers/id = '4711'"),
    );
    assert!(
        matches!(refused(&aql), Refusal::IdentifierElsewhere { .. }),
        "§5.4.1"
    );
}

#[test]
fn a_clinician_predicate_beside_the_entry_carrier_is_dispatched() {
    // §5.4.3 note, CP-38 second sentence: not rejected on path grounds.
    let aql = query(
        "c/uid/value",
        &format!("{ENTRY_ID} = '4711' AND c/composer/identifiers/id = 'clinician-7'"),
    );
    assert!(
        node_aql(&aql).contains("clinician-7"),
        "ordinary query material: {}",
        node_aql(&aql)
    );
}
