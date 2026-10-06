// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The integration tests of `ehds-logging`, one binary with a module per
//! subject.
//!
//! The modules carry `#[cfg(test)]`, which an integration test always has, so
//! the test-scoped relaxations of `clippy.toml` reach their helpers too.

#[cfg(test)]
#[cfg(feature = "balp")]
mod balp;
#[cfg(test)]
mod classify;
#[cfg(test)]
mod map;
#[cfg(test)]
mod property;
#[cfg(test)]
mod retention;

#[cfg(test)]
mod support {
    use std::collections::BTreeMap;

    use ehds_logging::map::{CategoryMap, Declared};

    /// A synthetic lab report template, mapped to medical test results.
    pub(crate) const LAB_REPORT: &str = "Example Lab Report.v1";
    /// A synthetic discharge template, mapped to discharge reports.
    pub(crate) const DISCHARGE: &str = "Example Discharge.v1";
    /// A synthetic administrative template, mapped to no category.
    pub(crate) const ADMIN: &str = "Example Admin Note.v1";
    /// A synthetic template the map holds no key for.
    pub(crate) const UNMAPPED: &str = "Example Unmapped.v1";
    /// An archetype mapped to medical test results.
    pub(crate) const LAB_ARCHETYPE: &str = "openEHR-EHR-OBSERVATION.laboratory_test_result.v1";
    /// An archetype mapped to patient summaries.
    pub(crate) const SUMMARY_ARCHETYPE: &str = "openEHR-EHR-EVALUATION.problem_diagnosis.v1";

    fn codes(codes: &[&str]) -> Declared {
        Declared::Codes(codes.iter().map(|code| (*code).to_owned()).collect())
    }

    /// The map the tests classify with.
    pub(crate) fn map() -> CategoryMap {
        let templates = BTreeMap::from([
            (LAB_REPORT.to_owned(), codes(&["medical-test-result"])),
            (DISCHARGE.to_owned(), codes(&["discharge-report"])),
            (ADMIN.to_owned(), Declared::Word("none".to_owned())),
        ]);
        let archetypes = BTreeMap::from([
            (LAB_ARCHETYPE.to_owned(), codes(&["medical-test-result"])),
            (
                SUMMARY_ARCHETYPE.to_owned(),
                codes(&["patient-summary", "nl-example"]),
            ),
        ]);
        CategoryMap::declare(&["nl-example".to_owned()], &templates, &archetypes)
            .expect("the test map")
            .with_digest("sha256:test".to_owned())
    }
}
