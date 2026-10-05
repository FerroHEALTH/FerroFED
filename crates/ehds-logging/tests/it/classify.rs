// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The categories of one access (Regulation (EU) 2025/327 Annex II 3.2(c)):
//! a mapped object, `none`, an unmapped object, an access spanning
//! categories, a query of leaf values or with no row, and data that could not
//! be read.

use std::collections::{BTreeMap, BTreeSet};

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence, RootObject, Unclassified};

use super::support::{
    ADMIN, DISCHARGE, LAB_ARCHETYPE, LAB_REPORT, SUMMARY_ARCHETYPE, UNMAPPED, map,
};

fn object(template: Option<&str>, archetype: Option<&str>) -> RootObject {
    RootObject {
        template_id: template.map(str::to_owned),
        archetype_id: archetype.map(str::to_owned),
        version_uid: Some("8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1".to_owned()),
    }
}

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

fn categories(found: &BTreeMap<Category, BTreeSet<Basis>>) -> Vec<&str> {
    found.keys().map(Category::code).collect()
}

#[test]
fn a_returned_object_takes_its_templates_categories() {
    let classified = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![object(
            Some(LAB_REPORT),
            Some("openEHR-EHR-COMPOSITION.report.v1"),
        )],
    ));
    assert_eq!(categories(classified.categories()), ["medical-test-result"]);
    assert_eq!(
        classified.categories().get(&Category::MedicalTestResult),
        Some(&BTreeSet::from([Basis::Returned]))
    );
    assert!(classified.unclassified().is_none());
    assert_eq!(
        classified.versions().len(),
        1,
        "the version uid is evidence"
    );
    assert_eq!(classified.digest(), Some("sha256:test"));
}

#[test]
fn the_template_key_wins_over_the_archetype_key() {
    let classified = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![object(Some(DISCHARGE), Some(LAB_ARCHETYPE))],
    ));
    assert_eq!(categories(classified.categories()), ["discharge-report"]);
}

#[test]
fn an_object_whose_template_is_unmapped_falls_back_to_its_archetype() {
    let classified = map().classify(&Evidence::reached(
        Basis::Written,
        vec![object(Some(UNMAPPED), Some(LAB_ARCHETYPE))],
    ));
    assert_eq!(
        classified.categories().get(&Category::MedicalTestResult),
        Some(&BTreeSet::from([Basis::Written]))
    );
    assert!(classified.unclassified().is_none());
}

#[test]
fn an_object_mapped_to_none_is_of_no_category() {
    let classified = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![object(Some(ADMIN), None)],
    ));
    assert!(classified.categories().is_empty());
    assert!(classified.is_no_category());
}

#[test]
fn an_unmapped_object_is_unclassified_with_its_ids_and_never_dropped() {
    let classified = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![
            object(Some(LAB_REPORT), None),
            object(Some(UNMAPPED), Some("openEHR-EHR-COMPOSITION.unknown.v1")),
        ],
    ));
    assert_eq!(
        categories(classified.categories()),
        ["medical-test-result"],
        "the mapped part keeps its category"
    );
    assert_eq!(classified.unclassified(), Some(&Unclassified::Unmapped));
    assert_eq!(
        classified.unmapped(),
        &set(&[UNMAPPED, "openEHR-EHR-COMPOSITION.unknown.v1"])
    );
}

#[test]
fn an_access_spanning_categories_records_them_all() {
    let classified = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![
            object(Some(LAB_REPORT), None),
            object(Some(DISCHARGE), None),
            object(None, Some(SUMMARY_ARCHETYPE)),
        ],
    ));
    assert_eq!(
        categories(classified.categories()),
        [
            "patient-summary",
            "medical-test-result",
            "discharge-report",
            "nl-example"
        ]
    );
}

#[test]
fn a_query_with_no_row_is_classified_by_what_it_queried() {
    let classified = map().classify(&Evidence::reached(Basis::Returned, Vec::new()).queried(
        set(&[]),
        set(&[LAB_ARCHETYPE, "openEHR-EHR-COMPOSITION.unknown.v1"]),
    ));
    assert_eq!(
        classified.categories().get(&Category::MedicalTestResult),
        Some(&BTreeSet::from([Basis::Queried]))
    );
    assert!(
        classified.unclassified().is_none(),
        "one mapped id classifies the query"
    );
    assert_eq!(
        classified.unmapped(),
        &set(&["openEHR-EHR-COMPOSITION.unknown.v1"])
    );
}

#[test]
fn a_queried_template_wins_over_a_queried_archetype() {
    let classified = map().classify(
        &Evidence::reached(Basis::Returned, Vec::new())
            .queried(set(&[DISCHARGE]), set(&[LAB_ARCHETYPE])),
    );
    assert_eq!(categories(classified.categories()), ["discharge-report"]);
}

#[test]
fn a_query_naming_nothing_mapped_or_nothing_at_all_is_unclassified() {
    let unmapped = map().classify(
        &Evidence::reached(Basis::Returned, Vec::new()).queried(set(&[UNMAPPED]), set(&[])),
    );
    assert_eq!(unmapped.unclassified(), Some(&Unclassified::Unmapped));
    assert_eq!(unmapped.unmapped(), &set(&[UNMAPPED]));
    let nothing = map().classify(&Evidence::reached(Basis::Returned, Vec::new()));
    assert_eq!(nothing.unclassified(), Some(&Unclassified::NamedNothing));
    assert!(nothing.categories().is_empty());
}

#[test]
fn a_resource_of_no_category_is_classified_by_its_kind() {
    let classified = map().classify(&Evidence::no_category());
    assert!(classified.is_no_category());
    assert!(classified.unclassified().is_none());
}

#[test]
fn data_that_could_not_be_read_are_unclassified_with_the_reason() {
    let classified = map().classify(&Evidence::unreadable("simplified-format"));
    assert_eq!(
        classified.unclassified(),
        Some(&Unclassified::Unreadable("simplified-format".to_owned()))
    );
}

#[test]
fn no_debug_shows_an_id() {
    let classified = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![object(Some(UNMAPPED), Some(LAB_ARCHETYPE))],
    ));
    let shown = format!("{classified:?} {:?}", object(Some(UNMAPPED), None));
    for id in [UNMAPPED, LAB_ARCHETYPE, "cdr-a.example.org"] {
        assert!(!shown.contains(id), "{id} in {shown}");
    }
}
