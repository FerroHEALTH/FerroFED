// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The category map a deployment declares: every category names an Art 14(1)
//! category or a declared national one, `none` is written as such, and the
//! canonical text does not depend on the order of the declarations.
//!
//! The wire form is pinned here for every product that writes the same
//! record: the six codes of HL7 Europe's `EEHRxFDocumentPriorityCategoryCS`
//! (`hl7.fhir.eu.health-data-api` 1.0.0-ballot), compared exactly, and a
//! national category as `<system>|<code>`.

use std::collections::BTreeMap;

use ehds_logging::category::{
    Category, CodeError, NationalCategory, PRIORITY_SYSTEM, PRIORITY_VERSION,
};
use ehds_logging::map::{CategoryMap, Declared, MapError, Mapping, Table};

use super::support::{ADMIN, LAB_REPORT, NATIONAL, SUMMARY_ARCHETYPE, map};

fn one(key: &str, declared: Declared) -> BTreeMap<String, Declared> {
    BTreeMap::from([(key.to_owned(), declared)])
}

fn codes(codes: &[&str]) -> Declared {
    Declared::Codes(codes.iter().map(|code| (*code).to_owned()).collect())
}

#[test]
fn the_six_priority_categories_have_the_codes_of_the_hl7_europe_code_system() {
    let codes: Vec<&str> = Category::PRIORITY.iter().map(Category::code).collect();
    assert_eq!(
        codes,
        [
            "Patient-Summaries",
            "Electronic-Prescriptions",
            "Electronic-Dispensations",
            "Medical-Imaging",
            "Laboratory-Reports",
            "Discharge-Reports"
        ],
        "Art 14(1)(a) to (f), CodeSystem eehrxf-document-priority-category-cs"
    );
    assert_eq!(
        PRIORITY_SYSTEM,
        "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs"
    );
    assert_eq!(PRIORITY_VERSION, "1.0.0-ballot");
    for category in Category::PRIORITY {
        assert_eq!(category.system(), PRIORITY_SYSTEM);
        assert_eq!(category.version(), Some(PRIORITY_VERSION));
        assert_eq!(
            category.token(),
            format!("{PRIORITY_SYSTEM}|{}", category.code())
        );
        assert_eq!(category.to_string(), category.token());
    }
}

#[test]
fn a_priority_code_is_compared_exactly() {
    for spelling in [
        "laboratory-reports",
        "LABORATORY-REPORTS",
        "Laboratory-Report",
    ] {
        assert_eq!(Category::priority(spelling), None, "{spelling}");
    }
    for old in [
        "patient-summary",
        "electronic-prescription",
        "electronic-dispensation",
        "medical-imaging",
        "medical-test-result",
        "discharge-report",
    ] {
        assert_eq!(Category::priority(old), None, "{old}");
    }
}

#[test]
fn a_mapped_key_and_none_read_back() {
    let map = map();
    assert_eq!(
        map.template(LAB_REPORT),
        Some(&Mapping::Categories([Category::MedicalTestResult].into()))
    );
    assert_eq!(map.template(ADMIN), Some(&Mapping::NoCategory));
    let national: NationalCategory = NATIONAL.parse().expect("a national category");
    assert_eq!(
        map.archetype(SUMMARY_ARCHETYPE),
        Some(&Mapping::Categories(
            [Category::PatientSummary, Category::National(national)].into()
        ))
    );
}

#[test]
fn a_priority_category_is_named_by_its_code_or_by_its_system_and_code() {
    let token = format!("{PRIORITY_SYSTEM}|Discharge-Reports");
    let map = CategoryMap::declare(
        &[],
        &one(LAB_REPORT, codes(&[&token, "Discharge-Reports"])),
        &BTreeMap::new(),
    )
    .expect("a map");
    assert_eq!(
        map.template(LAB_REPORT),
        Some(&Mapping::Categories([Category::DischargeReport].into()))
    );
    assert_eq!(map.category(&token), Some(Category::DischargeReport));
}

#[test]
fn a_national_category_is_named_by_its_system_and_code_only() {
    let map = map();
    let national: NationalCategory = NATIONAL.parse().expect("a national category");
    assert_eq!(map.category(NATIONAL), Some(Category::National(national)));
    assert_eq!(map.category("nl-example"), None, "a bare national code");
    assert_eq!(
        map.category("https://example.org/other|nl-example"),
        None,
        "the same code in another system"
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
fn a_code_of_the_priority_system_that_it_does_not_define_is_refused() {
    let token = format!("{PRIORITY_SYSTEM}|nl-example");
    let refused = CategoryMap::declare(
        &[],
        &one(LAB_REPORT, Declared::Codes(vec![token.clone()])),
        &BTreeMap::new(),
    );
    assert_eq!(
        refused,
        Err(MapError::Unknown {
            table: Table::Templates,
            key: LAB_REPORT.to_owned(),
            code: token
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
fn a_national_category_in_the_priority_system_is_refused() {
    let declared = format!("{PRIORITY_SYSTEM}|nl-example");
    let refused = CategoryMap::declare(
        std::slice::from_ref(&declared),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert_eq!(
        refused,
        Err(MapError::National {
            code: declared,
            source: CodeError::Priority
        })
    );
}

#[test]
fn a_national_category_without_a_system_is_refused() {
    assert_eq!(
        "nl-example".parse::<NationalCategory>(),
        Err(CodeError::NoSystem)
    );
    for system in ["", "example", "1urn:x", "urn:", "urn:ex ample", "urn:a|b"] {
        assert_eq!(
            NationalCategory::new(system, "nl-example"),
            Err(CodeError::System),
            "{system:?}"
        );
    }
}

#[test]
fn a_national_code_is_held_to_the_fhir_code_rules_only() {
    let system = "urn:oid:2.999.1";
    for code in ["Upper-Case", "a b", "x:y", "01", "é"] {
        assert!(NationalCategory::new(system, code).is_ok(), "{code:?}");
    }
    for code in ["", " a", "a ", "a  b", "a\tb", "a\nb", "a\u{1}b"] {
        assert_eq!(
            NationalCategory::new(system, code),
            Err(CodeError::Code),
            "{code:?}"
        );
    }
}

#[test]
fn the_canonical_text_names_every_declaration_in_order() {
    let lab = format!("{PRIORITY_SYSTEM}|Laboratory-Reports");
    let discharge = format!("{PRIORITY_SYSTEM}|Discharge-Reports");
    let summary = format!("{PRIORITY_SYSTEM}|Patient-Summaries");
    assert_eq!(
        map().canonical(),
        format!(
            "national\t{NATIONAL}\n\
             templates\tExample Admin Note.v1\tnone\n\
             templates\tExample Discharge.v1\t{discharge}\n\
             templates\tExample Lab Report.v1\t{lab}\n\
             archetypes\topenEHR-EHR-EVALUATION.problem_diagnosis.v1\t{summary},{NATIONAL}\n\
             archetypes\topenEHR-EHR-OBSERVATION.laboratory_test_result.v1\t{lab}\n"
        )
    );
}

#[test]
fn a_configuration_declares_the_map_in_toml() {
    #[derive(serde::Deserialize)]
    struct Table {
        templates: BTreeMap<String, Declared>,
    }
    let read: Table = toml::from_str(
        "[templates]\n\"Example Lab Report.v1\" = [\"Laboratory-Reports\"]\n\"Example Admin Note.v1\" = \"none\"\n",
    )
    .expect("TOML");
    let map = CategoryMap::declare(&[], &read.templates, &BTreeMap::new()).expect("a map");
    assert_eq!(map.template(ADMIN), Some(&Mapping::NoCategory));
    assert_eq!(
        map.template(LAB_REPORT),
        Some(&Mapping::Categories([Category::MedicalTestResult].into()))
    );
}
