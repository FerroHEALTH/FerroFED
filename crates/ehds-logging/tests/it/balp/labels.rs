// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The security labels of a record (Regulation (EU) 2025/327 Annex II 3.4):
//! all of them in `meta.security`, the categories' on the `ehds-categories`
//! entity and each origin's on its `origin` entity.

use ehds_logging::category::{PRIORITY_SYSTEM, PRIORITY_VERSION};
use ehds_logging::record::{Action, Outcome};
use serde_json::{Value, json};

use super::{ORIGIN_SYSTEM, entity, record, written};

/// The `Coding`s at `pointer` of `value`.
fn codings<'a>(value: &'a Value, pointer: &str) -> &'a [Value] {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The lab report category's label: its own code, at its system's version.
fn lab() -> Value {
    json!({"system": PRIORITY_SYSTEM, "version": PRIORITY_VERSION, "code": "Laboratory-Reports"})
}

/// Node A's label: its endpoint id in the origin system.
fn node_a() -> Value {
    json!({"system": ORIGIN_SYSTEM, "code": "node-a"})
}

#[test]
fn the_record_carries_the_labels_of_its_category_and_its_origin_in_meta_security() {
    let written = written(&record(Action::Query, Outcome::Success));
    assert_eq!(codings(&written, "/meta/security"), [lab(), node_a()]);
    assert!(
        written["meta"]["profile"][0].is_string(),
        "the labels keep the pattern: {written}"
    );
}

/// `RESTful` ATNA Rev. 3.6 Table 3.20.4.2.2.1-1 maps `entity.securityLabel`
/// to the DICOM `ParticipantObjectSensitivity`, so a repository that keeps
/// the DICOM form keeps each label with the entity it belongs to.
#[test]
fn each_label_is_also_on_the_entity_it_belongs_to() {
    let written = written(&record(Action::Read, Outcome::Success));
    let categories = entity(&written, "ehds-categories");
    let [categories] = categories.as_slice() else {
        panic!("one categories entity: {written}");
    };
    assert_eq!(codings(categories, "/securityLabel"), [lab()]);
    let origins = entity(&written, "origin");
    let [origin] = origins.as_slice() else {
        panic!("one origin entity: {written}");
    };
    assert_eq!(codings(origin, "/securityLabel"), [node_a()]);
    for unlabelled in ["ehr"] {
        for entity in entity(&written, unlabelled) {
            assert!(entity.get("securityLabel").is_none(), "{entity}");
        }
    }
}

/// A failed delete claims no pattern, and its labels still reach the record.
#[test]
fn a_record_that_claims_no_pattern_is_labelled_all_the_same() {
    let written = written(&record(Action::Delete, Outcome::MinorFailure));
    assert!(written.pointer("/meta/profile").is_none(), "{written}");
    assert_eq!(codings(&written, "/meta/security"), [lab(), node_a()]);
}
