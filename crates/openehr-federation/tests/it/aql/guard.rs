// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The analysis guard judges an identifier-bearing path by its value, not its
//! shape (§5.4.3): a clinician or facility predicate is dispatched unchanged,
//! the same path compared with the patient identifier is refused, and what a
//! security event records of a refusal or a strip is its position, never the
//! text written there.

use openehr_federation::aql::refusal::Refusal;

use super::{analysed, ask_all, assert_same_aql, node_aql, patient, refused};

/// The synthetic patient identifier, digits only so no path or keyword holds
/// it by accident.
const PATIENT: &str = "460193";

/// A clinician or facility identifier: a different value on the same kind of
/// path.
const CLINICIAN: &str = "clinician-77";

/// The patient predicate every case starts from.
fn subject() -> String {
    format!("e/ehr_status/subject/external_ref/id/value = '{PATIENT}'")
}

/// The identifier paths of the people and places a record names that are not
/// the patient (CP-38): the composer, the care facility, a performer, and the
/// committer of the version and of an attestation.
const PARTY_PATHS: [(&str, &str); 5] = [
    ("COMPOSITION c", "c/composer/identifiers/id"),
    (
        "COMPOSITION c",
        "c/context/health_care_facility/identifiers/id",
    ),
    (
        "COMPOSITION c CONTAINS OBSERVATION o",
        "o/other_participations/performer/identifiers/id",
    ),
    (
        "VERSION v CONTAINS COMPOSITION c",
        "v/commit_audit/committer/identifiers/id",
    ),
    (
        "VERSION v CONTAINS COMPOSITION c",
        "v/attestations/committer/identifiers/id",
    ),
];

// conformance: CP-26 CP-38
#[test]
fn a_clinician_or_facility_predicate_is_dispatched_unchanged() {
    for (contains, path) in PARTY_PATHS {
        let aql = format!(
            "SELECT c/uid/value FROM EHR e CONTAINS {contains} WHERE {} AND {path} = '{CLINICIAN}'",
            subject()
        );
        let node = node_aql(&aql);
        assert_same_aql(
            &node,
            &format!(
                "SELECT c/uid/value FROM EHR e CONTAINS {contains} \
                 WHERE e/ehr_id/value = '{}' AND {path} = '{CLINICIAN}'",
                super::EHR_ID
            ),
        );
        assert!(
            !node.contains(PATIENT),
            "{path}: the patient identifier is not in {node}"
        );
    }
}

// conformance: CP-26
#[test]
fn the_same_path_compared_with_the_patient_identifier_is_refused() {
    for (contains, path) in PARTY_PATHS {
        let aql = format!(
            "SELECT c/uid/value FROM EHR e CONTAINS {contains} WHERE {} AND {path} = '{PATIENT}'",
            subject()
        );
        let refusal = refused(&aql);
        assert!(
            matches!(refusal, Refusal::IdentifierElsewhere { .. }),
            "{path}: the value decides, not the shape (§5.4.3): {refusal:?}"
        );
    }
}

#[test]
fn a_refusal_names_its_kind_and_position_and_never_the_value() {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {} AND c/composer/identifiers/id = '{PATIENT}'",
        subject()
    );
    let refusal = refused(&aql);
    assert_eq!("identifier-elsewhere", refusal.kind());
    let at = refusal.at().cloned().expect("the leak is located");
    assert!(
        at.start < at.end && at.end <= aql.len(),
        "a byte range inside the query: {at:?}"
    );
    for shown in [refusal.to_string(), format!("{refusal:?}")] {
        assert!(
            !shown.contains(PATIENT),
            "the refusal quotes nothing: {shown}"
        );
    }
}

#[test]
fn every_refusal_has_a_kind() {
    let cases = [
        ("SELECT", "not-aql"),
        (
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '1' OR c/name/value = 'x'",
            "unreducible",
        ),
        (
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 1",
            "identifier-not-string",
        ),
        (
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '1' AND e/ehr_status/subject/external_ref/id/value = '2'",
            "second-subject",
        ),
        (
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '1' ORDER BY e/ehr_status/subject/external_ref/id/value",
            "subject-ordering",
        ),
    ];
    for (aql, kind) in cases {
        assert_eq!(kind, refused(aql).kind(), "{aql}");
    }
}

#[test]
fn a_strip_is_recorded_by_the_position_of_each_consumed_predicate() {
    let id = subject();
    let namespace = "e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1'";
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {id} AND {namespace} AND c/name/value = 'x'"
    );
    let query = patient(&aql);
    let written: Vec<&str> = query
        .stripped()
        .iter()
        .map(|at| {
            let at = at.clone().expect("each predicate is located");
            aql.get(at).expect("the range is inside the query")
        })
        .collect();
    assert_eq!(
        vec![id.as_str(), namespace],
        written,
        "one range per consumed predicate, the identifier first"
    );
}

#[test]
fn an_unscoped_query_strips_nothing() {
    let analysis = analysed(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE c/composer/identifiers/id = 'clinician-77'",
        &ask_all(),
    )
    .expect("a query naming no patient is answered");
    assert!(
        matches!(analysis, openehr_federation::aql::Analysis::Unscoped(_)),
        "a clinician predicate alone is no patient: {analysis:?}"
    );
}
