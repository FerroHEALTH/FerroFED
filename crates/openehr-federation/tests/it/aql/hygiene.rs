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

/// Where the façade query names the patient: either carrier, which a gateway
/// must accept on equal terms (§5.4.3, CP-38).
#[derive(Debug, Clone, Copy)]
enum Carrier {
    /// `EHR_STATUS.subject.external_ref`, optionally selected for re-injection.
    ExternalRef { selected: bool },
    /// An `ENTRY`-level `subject` `DV_IDENTIFIER`, optionally with its issuer.
    Entry { issuer: bool },
}

fn carrier() -> impl Strategy<Value = Carrier> {
    prop_oneof![
        prop::bool::ANY.prop_map(|selected| Carrier::ExternalRef { selected }),
        prop::bool::ANY.prop_map(|issuer| Carrier::Entry { issuer }),
    ]
}

/// A façade query naming the patient in either carrier, with up to three
/// other conditions, written or bound.
fn facade() -> impl Strategy<Value = (String, String, bool, bool)> {
    identifier().prop_flat_map(|id| {
        let conditions = prop::collection::vec(condition(&id), 0..4);
        (Just(id), conditions, carrier(), prop::bool::ANY).prop_map(|(id, conditions, carrier, bound)| {
            let select = if matches!(carrier, Carrier::ExternalRef { selected: true }) {
                "e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value"
            } else {
                "c/uid/value"
            };
            let subject = if bound { "$patient".to_owned() } else { format!("'{id}'") };
            let mut where_ = match carrier {
                Carrier::ExternalRef { .. } => {
                    format!("e/ehr_status/subject/external_ref/id/value = {subject}")
                }
                Carrier::Entry { issuer: false } => format!("o/subject/identifiers/id = {subject}"),
                Carrier::Entry { issuer: true } => format!(
                    "o/subject/identifiers/id = {subject} AND o/subject/identifiers/issuer = 'urn:oid:2.999.1'"
                ),
            };
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
                prop_assert!(!node.aql().contains("subject/identifiers"), "node query {} keeps the ENTRY carrier", node.aql());
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
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE o/subject/identifiers/id = '{sentinel}' OR c/name/value = 'x'"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE o/subject/identifiers/id = '{sentinel}' AND {subject} = 'other'"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE o/subject/identifiers/id = '{sentinel}' AND c/composer/identifiers/id = '{sentinel}'"
        ),
        format!(
            "SELECT o/subject/identifiers/id FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE o/subject/identifiers/id = '{sentinel}'"
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

/// Where a second value sits beside the patient predicate: the places a client
/// can carry the identifier to a node that are not the patient predicate
/// itself (#45).
#[derive(Debug, Clone, Copy)]
enum Seat {
    /// A selected literal.
    Projection,
    /// A predicate on an `ORDER BY` path.
    OrderBy,
    /// A predicate over a `PARTY_RELATED` subject's relationship.
    PartyRelated,
    /// A parameter bound on a clinician path.
    Parameter,
}

const SEATS: [Seat; 4] = [
    Seat::Projection,
    Seat::OrderBy,
    Seat::PartyRelated,
    Seat::Parameter,
];

/// The façade query naming the patient `id`, with `seeded` sat per `seat`,
/// and the parameters it needs.
fn seated(id: &str, seat: Seat, seeded: &str) -> (String, Parameters) {
    let subject = format!("e/ehr_status/subject/external_ref/id/value = '{id}'");
    let mut parameters = Parameters::new();
    let aql = match seat {
        Seat::Projection => format!(
            "SELECT '{seeded}' AS marker, c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject}"
        ),
        Seat::OrderBy => format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} \
             ORDER BY c/content[at0001, '{seeded}']/time/value"
        ),
        Seat::PartyRelated => format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o \
             WHERE {subject} AND o/subject/relationship/value = '{seeded}'"
        ),
        Seat::Parameter => {
            parameters.insert("clinician", Primitive::String(seeded.to_owned()));
            format!(
                "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {subject} \
                 AND c/composer/identifiers/id = $clinician"
            )
        }
    };
    (aql, parameters)
}

proptest! {
    // conformance: CP-26
    #[test]
    fn a_seated_identifier_is_either_kept_from_the_node_or_refused(
        id in identifier(),
        seat in prop_oneof![
            Just(Seat::Projection),
            Just(Seat::OrderBy),
            Just(Seat::PartyRelated),
            Just(Seat::Parameter),
        ],
        same in prop::bool::ANY,
    ) {
        // NOTE: a different value is a word, which the digits-only identifier
        // cannot be part of.
        let seeded = if same { id.clone() } else { "clinician".to_owned() };
        let (aql, parameters) = seated(&id, seat, &seeded);
        match analyse(&aql, &parameters, Paging::default(), &ask_all()) {
            Err(refusal) => {
                let shown = format!("{refusal} {refusal:?}");
                prop_assert!(!shown.contains(&id), "the refusal named the identifier: {shown}");
            }
            Ok(Analysis::Patient(query)) => {
                prop_assert!(!same, "a query seating the identifier again was dispatched: {aql}");
                let node = query.for_node(&HierObjectId::new(EHR_ID).unwrap());
                prop_assert!(!node.aql().contains(&id), "node query {} carries {}", node.aql(), id);
            }
            Ok(other) => prop_assert!(false, "a patient query analysed as {other:?}"),
        }
    }
}

#[test]
fn every_seat_with_a_different_value_is_dispatched() {
    // The property above is not vacuous: the value decides, not the shape
    // (§5.4.3). With a different value in the seat, every seat reaches the
    // node; a `PARTY_RELATED` relationship code is not an identifier, so a
    // predicate on it is ordinary query material.
    for seat in SEATS {
        let (aql, parameters) = seated("460193", seat, "clinician");
        let outcome = analyse(&aql, &parameters, Paging::default(), &ask_all());
        assert!(
            matches!(outcome, Ok(Analysis::Patient(_))),
            "{seat:?} with a different value is dispatched: {aql} -> {outcome:?}"
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
