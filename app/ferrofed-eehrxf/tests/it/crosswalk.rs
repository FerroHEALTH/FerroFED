// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every section of the patient summary crosswalk has one stored query or
//! one recorded reason for none, and the crosswalk's gaps are among the
//! reasons.

use std::collections::BTreeSet;

use eehrxf::crosswalk::patient_summary::PATIENT_SUMMARY;
use ferrofed_eehrxf::patient_summary::{Reason, Section, UNSELECTED};

/// The Xt-EHR root every crosswalk path starts with.
const ROOT: &str = "EHDSPatientSummary.";

/// The header, which the document assembly writes and no query feeds.
const HEADER: &str = "EHDSPatientSummary.header";

/// The crosswalk's section rows: one element below the root, the header
/// left out.
fn crosswalk_sections() -> BTreeSet<&'static str> {
    PATIENT_SUMMARY
        .rows
        .iter()
        .map(|row| row.path)
        .filter(|path| {
            path.strip_prefix(ROOT)
                .is_some_and(|rest| !rest.contains('.'))
                && *path != HEADER
        })
        .collect()
}

#[test]
fn every_crosswalk_section_has_a_query_or_a_reason_and_never_both() {
    let queried: BTreeSet<&str> = Section::ALL
        .iter()
        .map(|section| section.element())
        .collect();
    let unselected: BTreeSet<&str> = UNSELECTED.iter().map(|entry| entry.element).collect();
    assert_eq!(Section::ALL.len(), queried.len(), "one query per section");
    assert_eq!(UNSELECTED.len(), unselected.len(), "one reason per section");
    assert!(
        queried.is_disjoint(&unselected),
        "{queried:?} {unselected:?}"
    );
    let covered: BTreeSet<&str> = queried.union(&unselected).copied().collect();
    assert_eq!(crosswalk_sections(), covered);
}

#[test]
fn each_crosswalk_gap_is_a_section_with_no_query_for_the_clinical_review() {
    for gap in PATIENT_SUMMARY.gaps {
        let section = gap
            .path
            .strip_prefix(ROOT)
            .and_then(|rest| rest.split('.').next())
            .map(|name| format!("{ROOT}{name}"))
            .expect("a gap below the root");
        let entry = UNSELECTED
            .iter()
            .find(|entry| entry.element == section)
            .unwrap_or_else(|| panic!("{} has a query", gap.path));
        assert_eq!(Reason::Gap { ehn: gap.ehn }, entry.reason, "{}", gap.path);
    }
    let gaps = UNSELECTED
        .iter()
        .filter(|entry| matches!(entry.reason, Reason::Gap { .. }))
        .count();
    assert_eq!(PATIENT_SUMMARY.gaps.len(), gaps);
}
