// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A term mapping a FHIRconnect run wrote as no coding, read back from the
//! run's loss: each of `'>'`, `'<'` and `'?'` is withheld with its relation
//! and target, and every other loss is none (RM Release 1.1.0
//! `TERM_MAPPING.match`; eHN PS Art 10(3)).

use eehrxf::terminology::transcode::Relation;
use eehrxf::terminology::withheld::Withheld;
use fhirconnect::engine::outcome::{Location, Loss, SkipReason, Warning};

/// The loss a run declares for a term mapping of `match_code` at the
/// substance's element.
fn skipped(match_code: char) -> Warning {
    Warning::new(
        Loss::Skipped {
            mapping: String::from("substance"),
            reason: SkipReason::TermMappingMatch {
                match_code,
                terminology: String::from("SNOMED-CT"),
                code: String::from("762952008"),
            },
        },
        Location::default().with_fhir("AllergyIntolerance.code"),
    )
}

#[test]
fn each_relation_that_is_no_equivalence_is_withheld_with_its_line() {
    for (code, relation, words) in [
        ('>', Relation::Broader, "broader term"),
        ('<', Relation::Narrower, "narrower term"),
        ('?', Relation::Unknown, "related term of unknown kind"),
    ] {
        let withheld = Withheld::of(&skipped(code)).expect("a withheld mapping");
        assert_eq!(relation, withheld.relation());
        assert_eq!(
            ("SNOMED-CT", "762952008"),
            (withheld.terminology(), withheld.code())
        );
        assert_eq!(Some("AllergyIntolerance.code"), withheld.element());
        assert_eq!(
            format!("AllergyIntolerance.code: {words} SNOMED-CT 762952008"),
            withheld.narrative()
        );
    }
}

#[test]
fn an_equivalence_and_every_other_loss_are_none() {
    assert_eq!(None, Withheld::of(&skipped('=')));
    let defaulted = Warning::new(
        Loss::Defaulted {
            field: String::from("/language"),
        },
        Location::default().with_openehr("/language"),
    );
    assert_eq!(None, Withheld::of(&defaulted));
}

#[test]
fn a_mapping_with_no_located_element_says_so_in_its_line() {
    let warning = Warning::new(
        Loss::Skipped {
            mapping: String::from("substance"),
            reason: SkipReason::TermMappingMatch {
                match_code: '>',
                terminology: String::from("SNOMED-CT"),
                code: String::from("762952008"),
            },
        },
        Location::default(),
    );
    let withheld = Withheld::of(&warning).expect("a withheld mapping");
    assert_eq!(
        "A coded value: broader term SNOMED-CT 762952008",
        withheld.narrative()
    );
}
