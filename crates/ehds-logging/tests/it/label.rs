// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The security labels of a record by its categories and its origins
//! (Regulation (EU) 2025/327 Annex II 3.4): each category by its own code
//! unless the deployment names labels for it, each origin by its endpoint in
//! the origin system unless the deployment names labels for it, an
//! unclassified access by every category's labels, and a policy that names
//! no category, an empty list or a malformed label refused.

use std::collections::BTreeMap;

use ehds_logging::category::{Category, PRIORITY_SYSTEM, PRIORITY_VERSION};
use ehds_logging::classify::{Basis, Classification, Evidence, RootObject};
use ehds_logging::label::{DeclaredLabel, Label, LabelCodeError, LabelError, LabelPolicy};

use super::support::{ADMIN, LAB_REPORT, NATIONAL, UNMAPPED, map};

/// The code system origins are labelled in.
const ORIGINS: &str = "https://example.org/fhir/CodeSystem/access-log-origin";

/// A code system a deployment declares its own labels in.
const CLASSES: &str = "https://example.org/fhir/CodeSystem/access-class";

fn classified(template: &str) -> Classification {
    map().classify(&Evidence::reached(
        Basis::Returned,
        vec![RootObject {
            template_id: Some(template.to_owned()),
            ..RootObject::default()
        }],
    ))
}

fn declared(code: &str) -> DeclaredLabel {
    DeclaredLabel {
        system: CLASSES.to_owned(),
        code: code.to_owned(),
    }
}

fn label(system: &str, code: &str) -> Label {
    Label::new(system, code).expect("a label")
}

fn policy(
    categories: &[(&str, Vec<DeclaredLabel>)],
    origins: &[(&str, Vec<DeclaredLabel>)],
) -> Result<LabelPolicy, LabelError> {
    let table = |entries: &[(&str, Vec<DeclaredLabel>)]| -> BTreeMap<String, Vec<DeclaredLabel>> {
        entries
            .iter()
            .map(|(key, labels)| ((*key).to_owned(), labels.clone()))
            .collect()
    };
    LabelPolicy::declare(Some(ORIGINS), &table(categories), &table(origins), &map())
}

#[test]
fn a_category_with_no_labels_declared_is_labelled_with_its_own_code() {
    let labels = policy(&[], &[])
        .expect("the policy")
        .label(&classified(LAB_REPORT), []);
    let [lab] = labels.categories() else {
        panic!("one label: {labels:?}");
    };
    assert_eq!(lab, &Label::of(&Category::MedicalTestResult));
    assert_eq!(lab.system(), PRIORITY_SYSTEM);
    assert_eq!(lab.version(), Some(PRIORITY_VERSION));
    assert_eq!(lab.code(), "Laboratory-Reports");
}

#[test]
fn an_origin_with_no_labels_declared_is_labelled_with_its_endpoint() {
    let labels = policy(&[], &[])
        .expect("the policy")
        .label(&classified(LAB_REPORT), ["node-a", "node-b", "node-a"]);
    assert_eq!(labels.origin("node-a"), [label(ORIGINS, "node-a")]);
    assert_eq!(labels.origin("node-b"), [label(ORIGINS, "node-b")]);
    assert_eq!(
        labels.all(),
        [
            Label::of(&Category::MedicalTestResult),
            label(ORIGINS, "node-a"),
            label(ORIGINS, "node-b"),
        ],
        "each label once, the categories' first"
    );
}

#[test]
fn the_labels_a_deployment_declares_replace_the_default() {
    let labels = policy(
        &[("Laboratory-Reports", vec![declared("lab"), declared("lab")])],
        &[("node-a", vec![declared("hospital-a")])],
    )
    .expect("the policy")
    .label(&classified(LAB_REPORT), ["node-a", "node-b"]);
    assert_eq!(labels.categories(), [label(CLASSES, "lab")]);
    assert_eq!(labels.origin("node-a"), [label(CLASSES, "hospital-a")]);
    assert_eq!(labels.origin("node-b"), [label(ORIGINS, "node-b")]);
}

#[test]
fn a_category_is_named_by_its_system_and_code_too() {
    let coded = format!("{PRIORITY_SYSTEM}|Laboratory-Reports");
    let labels = policy(&[(coded.as_str(), vec![declared("lab")])], &[])
        .expect("the policy")
        .label(&classified(LAB_REPORT), []);
    assert_eq!(labels.categories(), [label(CLASSES, "lab")]);
}

/// An unclassified access may hold data of any category, so it carries the
/// labels of every category the map can name: a repository that admits a
/// reader only to a record whose every label the reader is cleared for
/// shows it to a reader cleared for all of them alone.
#[test]
fn an_unclassified_access_carries_the_labels_of_every_category() {
    let labels = policy(&[("Discharge-Reports", vec![declared("discharge")])], &[])
        .expect("the policy")
        .label(&classified(UNMAPPED), []);
    let national = map().category(NATIONAL).expect("the national category");
    let mut expected: Vec<Label> = Category::PRIORITY
        .iter()
        .map(|category| match category {
            Category::DischargeReport => label(CLASSES, "discharge"),
            other => Label::of(other),
        })
        .collect();
    expected.push(Label::of(&national));
    assert_eq!(labels.categories(), expected.as_slice());
}

#[test]
fn an_access_to_data_of_no_category_carries_no_category_label() {
    let labels = policy(&[], &[])
        .expect("the policy")
        .label(&classified(ADMIN), []);
    assert!(labels.categories().is_empty(), "{labels:?}");
}

#[test]
fn with_no_origin_system_an_undeclared_origin_carries_no_label() {
    let labels = LabelPolicy::declare(None, &BTreeMap::new(), &BTreeMap::new(), &map())
        .expect("the policy")
        .label(&classified(LAB_REPORT), ["node-a"]);
    assert!(labels.origin("node-a").is_empty(), "{labels:?}");
    assert_eq!(
        LabelPolicy::default().label(&classified(LAB_REPORT), ["node-a"]),
        labels,
        "the default policy names no origin system"
    );
}

#[test]
fn a_category_the_map_does_not_name_is_refused() {
    let refused = policy(&[("nl-undeclared", vec![declared("x")])], &[]);
    assert!(
        matches!(&refused, Err(LabelError::UnknownCategory { code }) if code == "nl-undeclared"),
        "{refused:?}"
    );
}

#[test]
fn a_category_two_keys_name_is_refused() {
    let coded = format!("{PRIORITY_SYSTEM}|Laboratory-Reports");
    let refused = policy(
        &[
            ("Laboratory-Reports", vec![declared("a")]),
            (coded.as_str(), vec![declared("b")]),
        ],
        &[],
    );
    assert!(
        matches!(refused, Err(LabelError::CategoryTwice { .. })),
        "{refused:?}"
    );
}

#[test]
fn an_empty_list_is_refused() {
    for (refused, key) in [
        (
            policy(&[("Laboratory-Reports", Vec::new())], &[]),
            "categories.Laboratory-Reports",
        ),
        (policy(&[], &[("node-a", Vec::new())]), "origins.node-a"),
    ] {
        assert!(
            matches!(&refused, Err(LabelError::Empty { key: named }) if named == key),
            "{key}: {refused:?}"
        );
    }
}

#[test]
fn a_malformed_label_is_refused_naming_its_key() {
    for (label, expected) in [
        (
            DeclaredLabel {
                system: "not a uri".to_owned(),
                code: "x".to_owned(),
            },
            LabelCodeError::System,
        ),
        (
            DeclaredLabel {
                system: format!("{CLASSES}|x"),
                code: "x".to_owned(),
            },
            LabelCodeError::System,
        ),
        (declared(""), LabelCodeError::Code),
        (declared(" lab"), LabelCodeError::Code),
        (declared("a\tb"), LabelCodeError::Code),
    ] {
        let refused = policy(&[], &[("node-a", vec![label])]);
        assert!(
            matches!(
                &refused,
                Err(LabelError::Label { key, source }) if key == "origins.node-a" && *source == expected
            ),
            "{refused:?}"
        );
    }
}

#[test]
fn an_empty_or_malformed_origin_key_is_refused() {
    for key in ["", "node\u{7}"] {
        let refused = policy(&[], &[(key, vec![declared("x")])]);
        assert!(matches!(refused, Err(LabelError::OriginKey)), "{refused:?}");
    }
}

#[test]
fn an_origin_system_that_is_no_absolute_uri_is_refused() {
    for system in ["", "origin", "https://example.org/a|b"] {
        let refused =
            LabelPolicy::declare(Some(system), &BTreeMap::new(), &BTreeMap::new(), &map());
        assert!(
            matches!(refused, Err(LabelError::OriginSystem)),
            "{system}: {refused:?}"
        );
    }
}

#[test]
fn the_endpoints_a_policy_names_are_listed() {
    let policy = policy(&[], &[("node-b", vec![declared("b")])]).expect("the policy");
    assert_eq!(policy.origins().collect::<Vec<_>>(), ["node-b"]);
}
