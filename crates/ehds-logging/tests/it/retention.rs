// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How long a record is kept (Regulation (EU) 2025/327 Art 9(2), Annex II
//! 3.4): never under three years from the date of access, the longest its
//! categories and origins call for, and the longest declared anywhere for an
//! access that is unclassified.

use std::collections::BTreeMap;

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Classification, Evidence, RootObject};
use ehds_logging::retention::{FLOOR_YEARS, Ground, RetentionError, RetentionPolicy, Years};
use jiff::Timestamp;
use jiff::civil::date;
use proptest::prelude::*;

use super::support::{DISCHARGE, LAB_REPORT, UNMAPPED, map};

/// The classification of an access that returned compositions of each of
/// `templates`.
fn returned(templates: &[&str]) -> Classification {
    map().classify(&Evidence::reached(
        Basis::Returned,
        templates
            .iter()
            .map(|template| RootObject {
                template_id: Some((*template).to_owned()),
                ..RootObject::default()
            })
            .collect(),
    ))
}

fn table(entries: &[(&str, u16)]) -> BTreeMap<String, u16> {
    entries
        .iter()
        .map(|(key, years)| ((*key).to_owned(), *years))
        .collect()
}

/// The policy of `years` for every record, `categories` and `origins`.
fn policy(
    years: u16,
    categories: &[(&str, u16)],
    origins: &[(&str, u16)],
) -> Result<RetentionPolicy, RetentionError> {
    RetentionPolicy::declare(years, &table(categories), &table(origins), &map())
}

fn at(text: &str) -> Timestamp {
    text.parse().expect("a timestamp")
}

#[test]
fn the_default_policy_keeps_every_record_three_years() {
    let retention = RetentionPolicy::default().retain(
        at("2027-03-05T10:00:00Z"),
        &returned(&[LAB_REPORT]),
        ["node-a"],
    );
    assert_eq!(retention.years(), Years::FLOOR);
    assert_eq!(retention.years().get(), FLOOR_YEARS);
    assert_eq!(retention.ground(), &Ground::Default);
}

#[test]
fn a_period_under_three_years_is_refused_wherever_it_is_declared() {
    assert_eq!(
        policy(2, &[], &[]),
        Err(RetentionError::UnderFloor {
            key: "years".to_owned(),
            years: 2,
        })
    );
    assert_eq!(
        policy(3, &[("medical-test-result", 1)], &[]),
        Err(RetentionError::UnderFloor {
            key: "categories.medical-test-result".to_owned(),
            years: 1,
        })
    );
    assert_eq!(
        policy(3, &[], &[("node-a", 0)]),
        Err(RetentionError::UnderFloor {
            key: "origins.node-a".to_owned(),
            years: 0,
        })
    );
    assert_eq!(Years::new(2), None);
    assert_eq!(Years::new(3), Some(Years::FLOOR));
}

#[test]
fn a_category_the_map_does_not_declare_is_refused() {
    assert_eq!(
        policy(3, &[("nl-undeclared", 10)], &[]),
        Err(RetentionError::UnknownCategory {
            code: "nl-undeclared".to_owned(),
        })
    );
    let national = policy(3, &[("nl-example", 10)], &[]).expect("a declared national category");
    assert_eq!(national.longest().get(), 10);
}

#[test]
fn an_empty_or_malformed_origin_is_refused() {
    assert_eq!(policy(3, &[], &[("", 5)]), Err(RetentionError::OriginKey));
    assert_eq!(
        policy(3, &[], &[("node\na", 5)]),
        Err(RetentionError::OriginKey)
    );
}

#[test]
fn a_record_is_kept_for_the_longest_its_categories_and_origins_call_for() {
    let policy = policy(
        5,
        &[("medical-test-result", 10), ("discharge-report", 20)],
        &[("node-a", 15), ("node-b", 30)],
    )
    .expect("a policy");
    let recorded = at("2027-03-05T10:00:00Z");
    let lab = policy.retain(recorded, &returned(&[LAB_REPORT]), ["node-c"]);
    assert_eq!(lab.years().get(), 10);
    assert_eq!(lab.ground(), &Ground::Category(Category::MedicalTestResult));
    let both = policy.retain(recorded, &returned(&[LAB_REPORT, DISCHARGE]), ["node-a"]);
    assert_eq!(both.years().get(), 20);
    assert_eq!(both.ground(), &Ground::Category(Category::DischargeReport));
    let origin = policy.retain(recorded, &returned(&[LAB_REPORT]), ["node-c", "node-a"]);
    assert_eq!(origin.years().get(), 15);
    assert_eq!(origin.ground(), &Ground::Origin("node-a".to_owned()));
}

#[test]
fn a_period_no_longer_than_the_default_leaves_the_default_as_ground() {
    let policy = policy(10, &[("medical-test-result", 10)], &[("node-a", 4)]).expect("a policy");
    let retention = policy.retain(
        at("2027-03-05T10:00:00Z"),
        &returned(&[LAB_REPORT]),
        ["node-a"],
    );
    assert_eq!(retention.years().get(), 10);
    assert_eq!(retention.ground(), &Ground::Default);
}

/// An access the map cannot classify takes the longest period declared
/// anywhere, for a category or an origin the access did not reach too.
#[test]
fn an_unclassified_access_takes_the_longest_period_declared() {
    let policy = policy(5, &[("medical-test-result", 10)], &[("node-b", 25)]).expect("a policy");
    assert_eq!(policy.longest().get(), 25);
    let mixed = policy.retain(
        at("2027-03-05T10:00:00Z"),
        &returned(&[LAB_REPORT, UNMAPPED]),
        ["node-a"],
    );
    assert_eq!(mixed.years().get(), 25);
    assert_eq!(mixed.ground(), &Ground::Unclassified);
    assert_eq!(Ground::Unclassified.code(), "unclassified");
}

#[test]
fn the_record_may_be_deleted_from_the_day_after_its_period_ends() {
    let policy = RetentionPolicy::default();
    let lab = returned(&[LAB_REPORT]);
    let ends = |text: &str| policy.retain(at(text), &lab, []).ends();
    assert_eq!(ends("2027-03-05T10:00:00Z"), date(2030, 3, 6));
    // An access just after midnight east of UTC is dated by its UTC day.
    assert_eq!(ends("2027-03-06T00:30:00+02:00"), date(2030, 3, 6));
    // An access on a leap day is kept until the period after it has passed.
    assert_eq!(ends("2028-02-29T10:00:00Z"), date(2031, 3, 1));
}

#[test]
fn a_ground_is_written_by_its_code() {
    assert_eq!(Ground::Default.code(), "default");
    assert_eq!(
        Ground::Category(Category::DischargeReport).code(),
        "category:discharge-report"
    );
    assert_eq!(Ground::Origin("node-a".to_owned()).code(), "origin:node-a");
}

proptest! {
    /// Whatever the policy declares, a classified record is kept for exactly
    /// the longest period among the default, its categories and its origins,
    /// never under three years, and its deletion date is past the date of
    /// access plus that many years.
    #[test]
    fn the_period_is_the_longest_applicable_and_never_under_the_floor(
        default in FLOOR_YEARS..60u16,
        lab in FLOOR_YEARS..60u16,
        discharge in FLOOR_YEARS..60u16,
        node_a in FLOOR_YEARS..60u16,
        reached_discharge in any::<bool>(),
        seconds in 0i64..4_000_000_000,
    ) {
        let policy = policy(
            default,
            &[("medical-test-result", lab), ("discharge-report", discharge)],
            &[("node-a", node_a)],
        )
        .expect("a policy");
        let templates: &[&str] = if reached_discharge {
            &[LAB_REPORT, DISCHARGE]
        } else {
            &[LAB_REPORT]
        };
        let recorded = Timestamp::from_second(seconds).expect("a timestamp");
        let retention = policy.retain(recorded, &returned(templates), ["node-a"]);
        let mut expected = default.max(lab).max(node_a);
        if reached_discharge {
            expected = expected.max(discharge);
        }
        prop_assert_eq!(retention.years().get(), expected);
        prop_assert!(retention.years() >= Years::FLOOR);
        let accessed = jiff::tz::Offset::UTC.to_datetime(recorded).date();
        let kept = accessed
            .checked_add(jiff::Span::new().years(i64::from(expected)))
            .expect("a date");
        prop_assert!(retention.ends() > kept);
    }
}
