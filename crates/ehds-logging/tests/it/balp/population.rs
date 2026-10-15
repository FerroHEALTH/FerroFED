// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A population access (Regulation (EU) 2025/327 Art 9(1)): a query over
//! many patients' data whose record names none of them states its scope in
//! an entity of its own, beside the origins it asked.

use std::collections::BTreeSet;

use ehds_logging::balp::{POPULATION_DESCRIPTION, POPULATION_ENTITY};
use ehds_logging::record::{AccessRecord, Action, Outcome, Population, PopulationReason};

use super::{LAB_REPORT, details, entity, patient_values, profile, record, written};

/// The archetype the population query reads.
const ARCHETYPE: &str = "openEHR-EHR-OBSERVATION.laboratory_test_result.v1";

/// A query over many patients' data, its record naming no patient, for
/// `reason`, with `ehrs` counted.
fn population_record(reason: PopulationReason, ehrs: Option<usize>) -> AccessRecord {
    let mut record = record(Action::Query, Outcome::Success);
    record.subject.patient = None;
    record.subject.ehrs = Vec::new();
    record.subject.population = Some(Population {
        reason,
        ehrs,
        limit: 2,
        templates: BTreeSet::from([LAB_REPORT.to_owned()]),
        archetypes: BTreeSet::from([ARCHETYPE.to_owned()]),
    });
    record
}

/// The entity marks the access a population access in words, names why no
/// `ehr_id` is named and what the query read, and the record names no
/// patient and claims the plain `Query` pattern.
#[test]
fn a_population_access_states_its_scope_in_its_own_entity() {
    let written = written(&population_record(PopulationReason::NotProjected, None));
    let marks = entity(&written, POPULATION_ENTITY);
    let [mark] = marks.as_slice() else {
        panic!("one population entity: {written}");
    };
    assert_eq!(mark["type"]["code"], "4");
    assert_eq!(mark["description"], POPULATION_DESCRIPTION);
    assert_eq!(details(mark, "ehds-population"), ["true"]);
    assert_eq!(details(mark, "ehds-population-reason"), ["not-projected"]);
    assert!(details(mark, "ehds-population-ehrs").is_empty());
    assert_eq!(details(mark, "ehds-population-limit"), ["2"]);
    assert_eq!(details(mark, "template-id"), [LAB_REPORT]);
    assert_eq!(details(mark, "archetype-id"), [ARCHETYPE]);
    assert!(patient_values(&written).is_empty());
    assert_eq!(
        profile(&written),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Query")
    );
    let origins = entity(&written, "origin");
    assert_eq!(1, origins.len(), "the member the query ran over");
    assert_eq!(details(origins[0], "node"), ["cdr-a"]);
    assert_eq!(details(origins[0], "system-id"), ["cdr-a.example.org"]);
}

/// Annex II 3.4: a population record carries the security labels of its
/// categories and origins as every record does.
#[test]
fn a_population_record_is_labelled_as_any_record() {
    let population = written(&population_record(PopulationReason::NotProjected, None));
    let named = written(&record(Action::Query, Outcome::Success));
    assert!(
        population["meta"]["security"]
            .as_array()
            .is_some_and(|labels| !labels.is_empty()),
        "{population}"
    );
    assert_eq!(population["meta"]["security"], named["meta"]["security"]);
    let origins = entity(&population, "origin");
    assert_eq!(
        origins[0]["securityLabel"],
        entity(&named, "origin")[0]["securityLabel"]
    );
}

#[test]
fn a_record_over_its_limit_says_how_many_ehrs_the_rows_named() {
    let written = written(&population_record(PopulationReason::OverLimit, Some(3)));
    let marks = entity(&written, POPULATION_ENTITY);
    assert_eq!(details(marks[0], "ehds-population-reason"), ["over-limit"]);
    assert_eq!(details(marks[0], "ehds-population-ehrs"), ["3"]);
}

#[test]
fn a_record_that_names_its_ehrs_carries_no_population_entity() {
    let written = written(&record(Action::Query, Outcome::Success));
    assert!(entity(&written, POPULATION_ENTITY).is_empty());
}

#[test]
fn the_reasons_are_written_as_their_codes() {
    assert_eq!(
        [
            "not-projected",
            "over-limit",
            "not-delivered",
            "unattributed"
        ],
        [
            PopulationReason::NotProjected,
            PopulationReason::OverLimit,
            PopulationReason::NotDelivered,
            PopulationReason::Unattributed,
        ]
        .map(PopulationReason::code)
    );
}

/// `Debug` names the reason and counts, never a template or archetype id.
#[test]
fn the_debug_of_a_population_names_no_id() {
    let record = population_record(PopulationReason::NotProjected, None);
    let text = format!("{:?}", record.subject);
    assert!(text.contains("NotProjected"), "{text}");
    assert!(!text.contains(LAB_REPORT), "{text}");
    assert!(!text.contains(ARCHETYPE), "{text}");
}
