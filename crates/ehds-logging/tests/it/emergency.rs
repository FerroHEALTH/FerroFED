// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The emergency mark (Regulation (EU) 2025/327 Art 11(5)): a declared
//! purpose of use marks an access only when the accessor declared exactly
//! that code in exactly that system, and nothing else marks it.

use ehds_logging::emergency::{EmergencyError, EmergencyPurposes};
use ehds_logging::record::Purpose;

/// The HL7 v3 `ActReason` code system.
const ACT_REASON: &str = "http://terminology.hl7.org/CodeSystem/v3-ActReason";

fn purpose(system: Option<&str>, code: &str) -> Purpose {
    Purpose {
        system: system.map(str::to_owned),
        code: code.to_owned(),
    }
}

fn break_the_glass() -> EmergencyPurposes {
    EmergencyPurposes::declare(&[
        purpose(Some(ACT_REASON), "BTG"),
        purpose(Some(ACT_REASON), "ETREAT"),
    ])
    .expect("the declared purposes")
}

#[test]
fn a_declared_purpose_marks_the_access_with_the_purposes_that_matched() {
    let declared = [
        purpose(Some(ACT_REASON), "TREAT"),
        purpose(Some(ACT_REASON), "BTG"),
    ];
    let mark = break_the_glass().mark(&declared).expect("a mark");
    assert_eq!(mark.purposes(), [purpose(Some(ACT_REASON), "BTG")]);
}

#[test]
fn each_matching_purpose_is_named_once_in_order() {
    let declared = [
        purpose(Some(ACT_REASON), "ETREAT"),
        purpose(Some(ACT_REASON), "BTG"),
        purpose(Some(ACT_REASON), "BTG"),
    ];
    let mark = break_the_glass().mark(&declared).expect("a mark");
    assert_eq!(
        mark.purposes(),
        [
            purpose(Some(ACT_REASON), "BTG"),
            purpose(Some(ACT_REASON), "ETREAT"),
        ]
    );
}

/// Never inferred: a purpose that is not declared, a code of another
/// system, a code of another case and a code with no system mark nothing.
#[test]
fn only_an_exact_match_marks_the_access() {
    let set = break_the_glass();
    for declared in [
        vec![],
        vec![purpose(Some(ACT_REASON), "TREAT")],
        vec![purpose(Some("urn:oid:2.999.5"), "BTG")],
        vec![purpose(Some(ACT_REASON), "btg")],
        vec![purpose(None, "BTG")],
    ] {
        assert!(set.mark(&declared).is_none(), "{declared:?}");
    }
}

#[test]
fn a_declared_code_with_no_system_matches_only_a_code_with_none() {
    let set = EmergencyPurposes::declare(&[purpose(None, "BTG")]).expect("declared");
    assert!(set.mark(&[purpose(None, "BTG")]).is_some());
    assert!(set.mark(&[purpose(Some(ACT_REASON), "BTG")]).is_none());
}

#[test]
fn an_empty_set_marks_nothing() {
    let set = EmergencyPurposes::default();
    assert!(set.is_empty());
    assert!(set.mark(&[purpose(Some(ACT_REASON), "BTG")]).is_none());
}

#[test]
fn an_empty_code_or_system_is_refused() {
    assert_eq!(
        EmergencyPurposes::declare(&[purpose(Some(ACT_REASON), "BTG"), purpose(None, " ")]),
        Err(EmergencyError::EmptyCode { index: 1 })
    );
    assert_eq!(
        EmergencyPurposes::declare(&[purpose(Some(""), "BTG")]),
        Err(EmergencyError::EmptySystem { index: 0 })
    );
}
