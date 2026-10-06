// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The categories of one access (Regulation (EU) 2025/327 Annex II 3.2(c)):
//! a mapped object, `none`, an unmapped object, an access spanning
//! categories, a query of leaf values or with no row, and data that could not
//! be read.

use std::collections::{BTreeMap, BTreeSet};

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence, Queried, RootObject, Unclassified, Unreadable};

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
    assert_eq!(categories(classified.categories()), ["Laboratory-Reports"]);
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
    assert_eq!(categories(classified.categories()), ["Discharge-Reports"]);
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
        ["Laboratory-Reports"],
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
            "Patient-Summaries",
            "Laboratory-Reports",
            "Discharge-Reports",
            "nl-example"
        ]
    );
}

fn queried(templates: &[&str], archetypes: &[&str]) -> Queried {
    Queried {
        templates: set(templates),
        archetypes: set(archetypes),
        every_root_bound: true,
    }
}

#[test]
fn a_query_with_no_row_is_classified_by_what_it_queried() {
    let classified = map().classify(
        &Evidence::reached(Basis::Returned, Vec::new()).queried(queried(&[], &[LAB_ARCHETYPE])),
    );
    assert_eq!(
        classified.categories().get(&Category::MedicalTestResult),
        Some(&BTreeSet::from([Basis::Queried]))
    );
    assert!(classified.unclassified().is_none());
}

#[test]
fn a_query_naming_an_unmapped_id_beside_a_mapped_one_is_unclassified() {
    let classified = map().classify(&Evidence::reached(Basis::Returned, Vec::new()).queried(
        queried(&[], &[LAB_ARCHETYPE, "openEHR-EHR-COMPOSITION.unknown.v1"]),
    ));
    assert_eq!(categories(classified.categories()), ["Laboratory-Reports"]);
    assert_eq!(classified.unclassified(), Some(&Unclassified::Unmapped));
    assert_eq!(
        classified.unmapped(),
        &set(&["openEHR-EHR-COMPOSITION.unknown.v1"])
    );
}

#[test]
fn a_queried_archetype_written_in_another_form_is_unmapped_never_classified() {
    for written in [
        "openehr-ehr-observation.laboratory_test_result.v1",
        "openEHR-EHR-OBSERVATION.laboratory_test_result.v1.0.0",
        "openEHR-EHR-OBSERVATION.laboratory_test_result.v1-rc",
        "org-x::openEHR-EHR-OBSERVATION.laboratory_test_result.v1",
        "'openEHR-EHR-OBSERVATION.laboratory_test_result.v1'",
    ] {
        let classified = map().classify(
            &Evidence::reached(Basis::Returned, Vec::new()).queried(queried(&[], &[written])),
        );
        assert!(!classified.is_no_category(), "{written}");
        assert_eq!(
            classified.unclassified(),
            Some(&Unclassified::Unmapped),
            "{written}: a key matches exactly, so a variant is marked unclassified"
        );
        assert_eq!(classified.unmapped(), &set(&[written]), "{written}");
    }
}

#[test]
fn a_queried_template_wins_over_a_queried_archetype() {
    let classified = map().classify(
        &Evidence::reached(Basis::Returned, Vec::new())
            .queried(queried(&[DISCHARGE], &[LAB_ARCHETYPE])),
    );
    assert_eq!(categories(classified.categories()), ["Discharge-Reports"]);
}

#[test]
fn a_query_naming_nothing_mapped_or_nothing_at_all_is_unclassified() {
    let unmapped = map().classify(
        &Evidence::reached(Basis::Returned, Vec::new()).queried(queried(&[UNMAPPED], &[])),
    );
    assert_eq!(unmapped.unclassified(), Some(&Unclassified::Unmapped));
    assert_eq!(unmapped.unmapped(), &set(&[UNMAPPED]));
    let nothing = map().classify(&Evidence::reached(Basis::Returned, Vec::new()));
    assert_eq!(nothing.unclassified(), Some(&Unclassified::NamedNothing));
    assert!(nothing.categories().is_empty());
}

#[test]
fn rows_with_no_archetype_details_are_never_of_no_category() {
    let leaves = map().classify(
        &Evidence::reached(Basis::Returned, Vec::new())
            .with_unrooted()
            .queried(Queried::default()),
    );
    assert!(!leaves.is_no_category(), "{leaves:?}");
    assert!(leaves.unclassified().is_some());
    let bare = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![RootObject::default()],
    ));
    assert!(!bare.is_no_category(), "{bare:?}");
    assert_eq!(bare.unclassified(), Some(&Unclassified::Unmapped));
}

#[test]
fn an_id_differing_by_case_space_or_specialisation_is_unmapped_never_none() {
    for template in [
        "example admin note.v1",
        " Example Admin Note.v1",
        "Example Admin Note.v1 ",
        "Example Admin Note.v2",
    ] {
        let classified = map().classify(&Evidence::reached(
            Basis::Returned,
            vec![object(Some(template), None)],
        ));
        assert!(!classified.is_no_category(), "{template:?}");
        assert_eq!(
            classified.unclassified(),
            Some(&Unclassified::Unmapped),
            "{template:?}"
        );
    }
    let specialised = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![object(
            None,
            Some("openEHR-EHR-OBSERVATION.laboratory_test_result-special.v1"),
        )],
    ));
    assert_eq!(specialised.unclassified(), Some(&Unclassified::Unmapped));
}

#[test]
fn none_never_erases_a_category_in_a_mixed_result() {
    let classified = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![object(Some(ADMIN), None), object(Some(LAB_REPORT), None)],
    ));
    assert_eq!(categories(classified.categories()), ["Laboratory-Reports"]);
    assert!(!classified.is_no_category());
}

#[test]
fn an_unmapped_template_does_not_take_its_archetypes_none() {
    let none = BTreeMap::from([(
        "openEHR-EHR-COMPOSITION.admin.v1".to_owned(),
        ehds_logging::map::Declared::Word("none".to_owned()),
    )]);
    let map = ehds_logging::map::CategoryMap::declare(&[], &BTreeMap::new(), &none).expect("a map");
    let classified = map.classify(&Evidence::reached(
        Basis::Returned,
        vec![object(
            Some(UNMAPPED),
            Some("openEHR-EHR-COMPOSITION.admin.v1"),
        )],
    ));
    assert!(!classified.is_no_category());
    assert_eq!(classified.unclassified(), Some(&Unclassified::Unmapped));
}

#[test]
fn a_root_object_beside_a_leaf_of_an_unbound_class_is_unclassified() {
    let classified = map().classify(
        &Evidence::reached(Basis::Returned, vec![object(Some(ADMIN), None)])
            .with_unrooted()
            .queried(Queried {
                every_root_bound: false,
                ..queried(&[], &[])
            }),
    );
    assert!(!classified.is_no_category(), "{classified:?}");
    assert!(classified.unclassified().is_some());
}

#[test]
fn a_query_whose_classes_are_not_all_bound_is_unclassified_with_what_mapped() {
    let classified = map().classify(&Evidence::reached(Basis::Returned, Vec::new()).queried(
        Queried {
            every_root_bound: false,
            ..queried(&[], &[LAB_ARCHETYPE])
        },
    ));
    assert_eq!(categories(classified.categories()), ["Laboratory-Reports"]);
    assert_eq!(classified.unclassified(), Some(&Unclassified::Unbound));
}

#[test]
fn a_resource_of_no_category_is_classified_by_its_kind() {
    let classified = map().classify(&Evidence::no_category());
    assert!(classified.is_no_category());
    assert!(classified.unclassified().is_none());
}

#[test]
fn data_that_could_not_be_read_are_unclassified_with_the_reason() {
    let classified = map().classify(&Evidence::unreadable(Unreadable::Format));
    assert_eq!(
        classified.unclassified(),
        Some(&Unclassified::Unreadable(Unreadable::Format))
    );
}

#[test]
fn each_unreadable_reason_is_written_by_its_code() {
    let written: Vec<&str> = [
        Unreadable::Operation,
        Unreadable::Format,
        Unreadable::Body,
        Unreadable::NoObject,
    ]
    .into_iter()
    .map(|why| Unclassified::Unreadable(why).code())
    .collect();
    assert_eq!(
        written,
        [
            "operation-not-read",
            "format-not-read",
            "body-not-read",
            "no-object-returned"
        ]
    );
    assert_eq!(Unclassified::Unmapped.code(), "unmapped");
    assert_eq!(Unclassified::NamedNothing.code(), "named-nothing");
    assert_eq!(Unclassified::Unbound.code(), "unbound");
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

#[test]
fn a_request_of_one_category_by_construction_holds_it_beside_its_data() {
    let classified = map().classify(
        &Evidence::reached(Basis::Returned, vec![object(Some(LAB_REPORT), None)])
            .constructed(Category::PatientSummary),
    );
    assert_eq!(
        categories(classified.categories()),
        ["Patient-Summaries", "Laboratory-Reports"]
    );
    assert_eq!(
        classified.categories().get(&Category::PatientSummary),
        Some(&BTreeSet::from([Basis::Construction]))
    );
    assert!(classified.unclassified().is_none());
}

#[test]
fn a_category_by_construction_never_hides_an_unmapped_object() {
    let classified = map().classify(
        &Evidence::reached(Basis::Returned, vec![object(Some(UNMAPPED), None)])
            .constructed(Category::PatientSummary),
    );
    assert_eq!(categories(classified.categories()), ["Patient-Summaries"]);
    assert_eq!(classified.unclassified(), Some(&Unclassified::Unmapped));
    assert_eq!(classified.unmapped(), &set(&[UNMAPPED]));
}
