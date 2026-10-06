// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The classification fails closed: whatever an access shows, it is of no
//! category only when the map declares `none` by an exact key for every root
//! object it reached and nothing else is uncertain, and every result names a
//! category, that `none`, or a reason it is unclassified.

use std::collections::BTreeSet;

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Classification, Evidence, Queried, RootObject, Unreadable};
use proptest::prelude::*;

use super::support::{ADMIN, LAB_ARCHETYPE, LAB_REPORT, SUMMARY_ARCHETYPE, map};

/// One root object of a kind the property knows the answer for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A template the map gives a category.
    Mapped,
    /// A template the map declares `none`.
    None,
    /// An id the map does not hold, however close to one it is.
    Garbage,
    /// No id at all, as a row without `archetype_details`.
    Bare,
}

fn object(kind: Kind, garbage: &str) -> RootObject {
    let template = match kind {
        Kind::Mapped => Some(LAB_REPORT.to_owned()),
        Kind::None => Some(ADMIN.to_owned()),
        Kind::Garbage => Some(garbage.to_owned()),
        Kind::Bare => None,
    };
    RootObject {
        template_id: template,
        ..RootObject::default()
    }
}

/// Text that is no key of the test map: any string, and near misses of the
/// keys by case, space or suffix.
fn garbage() -> impl Strategy<Value = String> {
    prop_oneof![
        ".*",
        Just(ADMIN.to_lowercase()),
        Just(format!(" {ADMIN}")),
        Just(format!("{ADMIN}\n")),
        Just(format!("{ADMIN}-special")),
        Just(String::new()),
    ]
    .prop_filter("no key of the map", |text| {
        ![ADMIN, LAB_REPORT, LAB_ARCHETYPE, SUMMARY_ARCHETYPE].contains(&text.as_str())
            && text != "Example Discharge.v1"
    })
}

fn kind() -> impl Strategy<Value = Kind> {
    prop_oneof![
        Just(Kind::Mapped),
        Just(Kind::None),
        Just(Kind::Garbage),
        Just(Kind::Bare)
    ]
}

/// Every result says what it is.
fn says_what_it_is(classified: &Classification) -> bool {
    !classified.categories().is_empty()
        || classified.is_no_category()
        || classified.unclassified().is_some()
}

proptest! {
    #[test]
    fn a_result_is_of_no_category_only_when_every_object_is_an_exact_none(
        kinds in proptest::collection::vec(kind(), 0..6),
        garbage in garbage(),
        unrooted in any::<bool>(),
    ) {
        let objects: Vec<RootObject> = kinds.iter().map(|kind| object(*kind, &garbage)).collect();
        let mut evidence = Evidence::reached(Basis::Returned, objects);
        if unrooted {
            evidence = evidence.with_unrooted();
        }
        let classified = map().classify(&evidence);
        prop_assert!(says_what_it_is(&classified));
        let every_none = !kinds.is_empty() && kinds.iter().all(|kind| *kind == Kind::None);
        prop_assert_eq!(classified.is_no_category(), every_none && !unrooted);
        prop_assert_eq!(
            classified.categories().contains_key(&Category::MedicalTestResult),
            kinds.contains(&Kind::Mapped)
        );
        let uncertain = kinds.is_empty()
            || unrooted
            || kinds.iter().any(|kind| matches!(kind, Kind::Garbage | Kind::Bare));
        prop_assert_eq!(classified.unclassified().is_some(), uncertain);
    }

    #[test]
    fn a_query_naming_garbage_or_not_bound_is_never_of_no_category(
        templates in proptest::collection::btree_set(garbage(), 0..3),
        archetypes in proptest::collection::btree_set(garbage(), 0..3),
        bound in any::<bool>(),
    ) {
        let classified = map().classify(
            &Evidence::reached(Basis::Returned, Vec::new()).queried(Queried {
                templates,
                archetypes,
                every_root_bound: bound,
            }),
        );
        prop_assert!(!classified.is_no_category());
        prop_assert!(classified.unclassified().is_some());
        prop_assert!(says_what_it_is(&classified));
    }

    #[test]
    fn a_none_query_is_of_no_category_only_when_bound(bound in any::<bool>()) {
        let classified = map().classify(
            &Evidence::reached(Basis::Returned, Vec::new()).queried(Queried {
                templates: BTreeSet::from([ADMIN.to_owned()]),
                archetypes: BTreeSet::new(),
                every_root_bound: bound,
            }),
        );
        prop_assert_eq!(classified.is_no_category(), bound);
        prop_assert!(says_what_it_is(&classified));
    }
}

#[test]
fn evidence_that_shows_nothing_is_unclassified() {
    for evidence in [
        Evidence::default(),
        Evidence::reached(Basis::Written, Vec::new()),
        Evidence::unreadable(Unreadable::Body),
    ] {
        let classified = map().classify(&evidence);
        assert!(classified.unclassified().is_some(), "{evidence:?}");
        assert!(!classified.is_no_category(), "{evidence:?}");
    }
}
