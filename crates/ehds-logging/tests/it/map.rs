// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The category map a deployment declares: every code names an Art 14(1)
//! category or a declared national one, `none` is written as such, and the
//! canonical text does not depend on the order of the declarations.

use std::collections::BTreeMap;

use ehds_logging::category::{Category, CodeError, NationalCode};
use ehds_logging::map::{CategoryMap, Declared, MapError, Mapping, Table};

use super::support::{ADMIN, LAB_REPORT, SUMMARY_ARCHETYPE, map};

fn one(key: &str, declared: Declared) -> BTreeMap<String, Declared> {
    BTreeMap::from([(key.to_owned(), declared)])
}

#[test]
fn the_six_priority_categories_have_their_codes() {
    let codes: Vec<&str> = Category::PRIORITY.iter().map(Category::code).collect();
    assert_eq!(
        codes,
        [
            "patient-summary",
            "electronic-prescription",
            "electronic-dispensation",
            "medical-imaging",
            "medical-test-result",
            "discharge-report"
        ],
        "Art 14(1)(a) to (f)"
    );
}

#[test]
fn a_mapped_key_and_none_read_back() {
    let map = map();
    assert_eq!(
        map.template(LAB_REPORT),
        Some(&Mapping::Categories([Category::MedicalTestResult].into()))
    );
    assert_eq!(map.template(ADMIN), Some(&Mapping::NoCategory));
    let national: NationalCode = "nl-example".parse().expect("a national code");
    assert_eq!(
        map.archetype(SUMMARY_ARCHETYPE),
        Some(&Mapping::Categories(
            [Category::PatientSummary, Category::National(national)].into()
        ))
    );
}

#[test]
fn a_code_that_names_no_category_is_refused() {
    let refused = CategoryMap::declare(
        &[],
        &one(LAB_REPORT, Declared::Codes(vec!["lab".to_owned()])),
        &BTreeMap::new(),
    );
    assert_eq!(
        refused,
        Err(MapError::Unknown {
            table: Table::Templates,
            key: LAB_REPORT.to_owned(),
            code: "lab".to_owned()
        })
    );
}

#[test]
fn an_empty_list_a_word_other_than_none_and_an_empty_key_are_refused() {
    let empty = CategoryMap::declare(
        &[],
        &BTreeMap::new(),
        &one(SUMMARY_ARCHETYPE, Declared::Codes(Vec::new())),
    );
    assert!(matches!(empty, Err(MapError::Empty { .. })), "{empty:?}");
    let word = CategoryMap::declare(
        &[],
        &one(ADMIN, Declared::Word("nothing".to_owned())),
        &BTreeMap::new(),
    );
    assert!(matches!(word, Err(MapError::Word { .. })), "{word:?}");
    let key = CategoryMap::declare(
        &[],
        &one("", Declared::Word("none".to_owned())),
        &BTreeMap::new(),
    );
    assert_eq!(key, Err(MapError::Key(Table::Templates)));
}

#[test]
fn a_national_code_repeating_a_priority_category_is_refused() {
    let refused = CategoryMap::declare(
        &["discharge-report".to_owned()],
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert_eq!(
        refused,
        Err(MapError::National {
            code: "discharge-report".to_owned(),
            source: CodeError::Priority(Category::DischargeReport)
        })
    );
    assert_eq!(
        "Upper".parse::<NationalCode>(),
        Err(CodeError::Malformed),
        "lower-case ASCII only"
    );
}

#[test]
fn the_canonical_text_names_every_declaration_in_order() {
    assert_eq!(
        map().canonical(),
        "national\tnl-example\n\
         templates\tExample Admin Note.v1\tnone\n\
         templates\tExample Discharge.v1\tdischarge-report\n\
         templates\tExample Lab Report.v1\tmedical-test-result\n\
         archetypes\topenEHR-EHR-EVALUATION.problem_diagnosis.v1\tpatient-summary,nl-example\n\
         archetypes\topenEHR-EHR-OBSERVATION.laboratory_test_result.v1\tmedical-test-result\n"
    );
}

#[test]
fn a_configuration_declares_the_map_in_toml() {
    #[derive(serde::Deserialize)]
    struct Table {
        templates: BTreeMap<String, Declared>,
    }
    let read: Table = toml::from_str(
        "[templates]\n\"Example Lab Report.v1\" = [\"medical-test-result\"]\n\"Example Admin Note.v1\" = \"none\"\n",
    )
    .expect("TOML");
    let map = CategoryMap::declare(&[], &read.templates, &BTreeMap::new()).expect("a map");
    assert_eq!(map.template(ADMIN), Some(&Mapping::NoCategory));
}
