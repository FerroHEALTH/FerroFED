// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Identifier hygiene (§5.4.1, N33): no node query the rewrite produces
//! carries the patient identifier, and no refusal names it.

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aql::{Analysis, Paging, analyse};
use openehr_query::ast::Primitive;
use openehr_query::bind::Parameters;
use proptest::prelude::{Just, Strategy, prop, prop_assert, prop_oneof, proptest};

use super::{EHR_ID, ask_all, refused};

/// A synthetic identifier: digits only, so that no keyword, path or code of a
/// generated query can contain it by accident.
fn identifier() -> impl Strategy<Value = String> {
    "[1-9][0-9]{4,8}"
}

/// A condition beside the patient predicate, with a literal drawn from a
/// domain that may or may not contain the identifier, and whether the
/// condition rebuilds the identifier from split literals.
fn condition(identifier: &str) -> impl Strategy<Value = (String, bool)> + use<> {
    let words = "[a-z]{3,8}";
    let plain = prop_oneof![
        words.prop_map(|w| format!("c/name/value = '{w}'")),
        (0_u32..1_000_000).prop_map(|n| format!(
            "o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude > {n}"
        )),
        Just(format!("c/composer/identifiers/id = '{identifier}'")),
        Just(format!("c/name/value = 'x{identifier}y'")),
        Just(format!("c/name/value LIKE '*{identifier}*'")),
        words.prop_map(|w| format!("c/composer/name = '{w}'")),
    ];
    prop_oneof![
        plain.prop_map(|condition| (condition, false)),
        reconstruction(identifier).prop_map(|condition| (condition, true)),
    ]
}

/// The identifier rebuilt by a string function from split literals, so no
/// single literal holds it (decision A4): `CONCAT`, `CONCAT_WS` with an empty
/// separator, or a nested `CONCAT` around a `SUBSTRING`, on a plain or an
/// identifier path.
fn reconstruction(identifier: &str) -> impl Strategy<Value = String> + use<> {
    let chars: Vec<char> = identifier.chars().collect();
    let cut = 1..chars.len();
    (cut, 0_u8..3, prop::bool::ANY).prop_map(move |(at, shape, on_identifier)| {
        let head: String = chars.iter().take(at).collect();
        let tail: String = chars.iter().skip(at).collect();
        let call = match shape {
            0 => format!("CONCAT('{head}', '{tail}')"),
            1 => format!("CONCAT_WS('', '{head}', '{tail}')"),
            _ => format!("CONCAT(SUBSTRING('x{head}', 2), '{tail}')"),
        };
        let path = if on_identifier {
            "c/composer/identifiers/id"
        } else {
            "c/name/value"
        };
        format!("{path} = {call}")
    })
}

/// A façade query naming the patient, with up to three other conditions, an
/// optional subject column, written or bound.
fn facade() -> impl Strategy<Value = (String, String, bool, bool)> {
    identifier().prop_flat_map(|id| {
        let conditions = prop::collection::vec(condition(&id), 0..4);
        (Just(id), conditions, prop::bool::ANY, prop::bool::ANY).prop_map(|(id, conditions, select_subject, bound)| {
            let select = if select_subject {
                "e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value"
            } else {
                "c/uid/value"
            };
            let subject = if bound { "$patient".to_owned() } else { format!("'{id}'") };
            let mut where_ = format!("e/ehr_status/subject/external_ref/id/value = {subject}");
            let mut rebuilds = false;
            for (condition, rebuilt) in conditions {
                where_.push_str(" AND ");
                where_.push_str(&condition);
                rebuilds |= rebuilt;
            }
            let aql = format!(
                "SELECT {select} FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE {where_}"
            );
            (aql, id, bound, rebuilds)
        })
    })
}

proptest! {
    #[test]
    fn no_node_query_carries_the_patient_identifier((aql, id, bound, rebuilds) in facade()) {
        let mut parameters = Parameters::new();
        if bound {
            parameters.insert("patient", Primitive::String(id.clone()));
        }
        match analyse(&aql, &parameters, Paging::default(), &ask_all()) {
            Err(refusal) => {
                let shown = format!("{refusal} {refusal:?}");
                prop_assert!(!shown.contains(&id), "the refusal named the identifier: {shown}");
            }
            Ok(Analysis::Patient(query)) => {
                prop_assert!(!rebuilds, "a query rebuilding the identifier from split literals was dispatched: {aql}");
                let node = query.for_node(&HierObjectId::new(EHR_ID).unwrap());
                let without_scope = node.aql().replace(&format!("'{EHR_ID}'"), "");
                prop_assert!(!without_scope.contains(&id), "node query {} carries {}", node.aql(), id);
                prop_assert!(!node.aql().contains("ehr_status/subject"), "node query {} keeps the subject", node.aql());
            }
            Ok(other) => prop_assert!(false, "a patient query analysed as {other:?}"),
        }
    }
}

#[test]
fn a_query_whose_other_literals_avoid_the_identifier_is_dispatched() {
    // The property above is not vacuous: a clean query passes.
    let aql = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
               WHERE e/ehr_status/subject/external_ref/id/value = '4711' AND c/name/value = 'Visit'";
    assert!(
        matches!(
            analyse(aql, &Parameters::new(), Paging::default(), &ask_all()),
            Ok(Analysis::Patient(_))
        ),
        "dispatched"
    );
}

#[test]
fn no_refusal_names_the_identifier() {
    let sentinel = "sentinel4711";
    let subject = "e/ehr_status/subject/external_ref/id/value";
    let refusing = [
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} = '{sentinel}' OR c/name/value = 'x'"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} != '{sentinel}'"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} = '{sentinel}' AND {subject} = 'other'"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} = '{sentinel}' AND c/name/value = '{sentinel}'"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} = '{sentinel}' ORDER BY {subject}"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} LIKE '{sentinel}*'"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} = '{sentinel}' AND"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE o/subject/identifiers/id = '{sentinel}'"
        ),
    ];
    for aql in &refusing {
        let refusal = refused(aql);
        let shown = format!("{refusal} {refusal:?}");
        assert!(
            !shown.contains(sentinel),
            "the refusal for {aql} named the identifier: {shown}"
        );
    }
}

#[test]
fn a_parameter_fault_names_the_parameter_never_its_value() {
    let mut parameters = Parameters::new();
    parameters.insert("patient", Primitive::String("sentinel4711".into()));
    parameters.insert("unused", Primitive::String("sentinel4711".into()));
    let aql = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = $patient";
    let refusal = analyse(aql, &parameters, Paging::default(), &ask_all()).unwrap_err();
    let shown = format!("{refusal} {refusal:?}");
    assert!(
        shown.contains("unused"),
        "the fault names the parameter: {shown}"
    );
    assert!(
        !shown.contains("sentinel4711"),
        "and never its value: {shown}"
    );
}
