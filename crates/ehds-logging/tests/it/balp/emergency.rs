// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An emergency access (Regulation (EU) 2025/327 Art 11(5)), marked in an
//! entity of its own.

use ehds_logging::balp::{EMERGENCY_DESCRIPTION, EMERGENCY_ENTITY};
use ehds_logging::emergency::EmergencyPurposes;
use ehds_logging::record::{AccessRecord, Action, Outcome, Purpose};

use super::{details, entity, person, profile, record, written};

/// The HL7 v3 `ActReason` code system the purposes of use are drawn from.
const ACT_REASON: &str = "http://terminology.hl7.org/CodeSystem/v3-ActReason";

/// A record of an access whose accessor declared treatment and break the
/// glass, the deployment declaring break the glass an emergency purpose.
fn emergency_record() -> AccessRecord {
    let mut record = record(Action::Query, Outcome::Success);
    let glass = Purpose {
        system: Some(ACT_REASON.to_owned()),
        code: "BTG".to_owned(),
    };
    record.accessor.purposes.push(glass.clone());
    let declared = EmergencyPurposes::declare(&[glass]).expect("the declared purposes");
    record.emergency = declared.mark(&record.accessor.purposes);
    record
}

/// Regulation (EU) 2025/327 Art 11(5): an emergency access is marked in an
/// entity of its own, stated in words and naming the purpose that marked
/// it, and its purposes stay in `agent:user` `purposeOfUse`, where BALP
/// 1.1.4 puts every purpose.
#[test]
fn an_emergency_access_is_marked_in_its_own_entity() {
    let written = written(&emergency_record());
    let marks = entity(&written, EMERGENCY_ENTITY);
    let [mark] = marks.as_slice() else {
        panic!("one emergency entity: {written}");
    };
    assert_eq!(mark["type"]["code"], "4");
    assert_eq!(mark["description"], EMERGENCY_DESCRIPTION);
    assert_eq!(details(mark, "ehds-emergency-access"), ["true"]);
    assert_eq!(
        details(mark, "ehds-emergency-purpose"),
        [format!("{ACT_REASON}|BTG")],
        "the purpose that marked it, and not TREAT"
    );
    let codes: Vec<&str> = person(&written)["purposeOfUse"]
        .as_array()
        .expect("purposeOfUse")
        .iter()
        .filter_map(|purpose| purpose["coding"][0]["code"].as_str())
        .collect();
    assert_eq!(codes, ["TREAT", "BTG"], "BALP agent:user purposeOfUse");
    assert_eq!(
        profile(&written),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientQuery"),
        "the mark changes no pattern"
    );
}

/// A record with no mark has no emergency entity.
#[test]
fn an_access_with_no_mark_has_no_emergency_entity() {
    let written = written(&record(Action::Query, Outcome::Success));
    assert!(entity(&written, EMERGENCY_ENTITY).is_empty(), "{written}");
    assert!(!written.to_string().contains("ehds-emergency"), "{written}");
}

/// A refused emergency access is marked too: the mark records what the
/// accessor asserted, whatever the origins answered.
#[test]
fn a_refused_emergency_access_is_still_marked() {
    let mut record = emergency_record();
    record.outcome = Outcome::MinorFailure;
    let written = written(&record);
    assert_eq!(written["outcome"], "4");
    assert_eq!(entity(&written, EMERGENCY_ENTITY).len(), 1, "{written}");
}
