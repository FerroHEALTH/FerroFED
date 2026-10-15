// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A write the data subject inserted (Regulation (EU) 2025/327 Art 5),
//! marked in an entity of its own.

use ehds_logging::balp::inserted::{
    DESCRIPTION, ENTITY, REPRESENTATIVE_DESCRIPTION, REPRESENTATIVE_ENTITY,
};
use ehds_logging::record::{Action, Insertion, Outcome};

use super::{PATIENT, details, entity, record, written};

/// Regulation (EU) 2025/327 Art 5: the record of a write the data subject
/// inserted says so in an entity of its own, in words and as a detail, and
/// names no more of the patient than the record of any other write.
#[test]
fn a_write_the_subject_inserted_is_marked_in_its_own_entity() {
    let mut inserted = record(Action::Create, Outcome::Success);
    inserted.inserted = Some(Insertion::Subject);
    let written = written(&inserted);
    let marks = entity(&written, ENTITY);
    let [mark] = marks.as_slice() else {
        panic!("one mark of the subject's insertion: {written}");
    };
    assert_eq!(mark["type"]["code"], "4");
    assert_eq!(mark["description"], DESCRIPTION);
    assert_eq!(details(mark, ENTITY), ["true"]);
    let plain = super::written(&record(Action::Create, Outcome::Success));
    assert_eq!(
        plain.to_string().matches(PATIENT).count(),
        written.to_string().matches(PATIENT).count(),
        "the mark names no identifier of the patient"
    );
}

#[test]
fn a_write_the_subject_did_not_insert_carries_no_mark() {
    let written = written(&record(Action::Create, Outcome::Success));
    assert!(
        entity(&written, ENTITY).is_empty(),
        "no mark unless the subject inserted the data: {written}"
    );
    assert!(
        entity(&written, REPRESENTATIVE_ENTITY).is_empty(),
        "no mark unless a representative inserted the data: {written}"
    );
}

/// Regulation (EU) 2025/327 Art 4(2), Art 5: the record of a write a
/// representative of the data subject inserted says so in an entity of its
/// own, never as the subject's insertion.
#[test]
fn a_write_a_representative_inserted_is_marked_as_theirs() {
    let mut inserted = record(Action::Create, Outcome::Success);
    inserted.inserted = Some(Insertion::Representative);
    let written = written(&inserted);
    let marks = entity(&written, REPRESENTATIVE_ENTITY);
    let [mark] = marks.as_slice() else {
        panic!("one mark of the representative's insertion: {written}");
    };
    assert_eq!(mark["type"]["code"], "4");
    assert_eq!(mark["description"], REPRESENTATIVE_DESCRIPTION);
    assert_eq!(details(mark, REPRESENTATIVE_ENTITY), ["true"]);
    assert!(
        entity(&written, ENTITY).is_empty(),
        "a representative's insertion is not the subject's: {written}"
    );
}
