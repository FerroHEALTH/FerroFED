// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dataset model does not depend on the order of the archive's members.

use std::error::Error;

use eehrxf::dataset::DatasetModel;
use proptest::prelude::*;
use proptest::test_runner::TestError;
use proptest::test_runner::TestRunner;

use super::support::archive;
use super::support::xtehr;
use super::support::xtehr_members;

#[test]
fn the_model_is_the_same_whatever_the_member_order() -> Result<(), Box<dyn Error>> {
    let expected = xtehr()?;
    let members = xtehr_members()?;
    // Each case re-packs and re-reads the whole package, so a few shuffles
    // are the budget.
    let mut runner = TestRunner::new(ProptestConfig::with_cases(6));
    runner
        .run(&Just(members).prop_shuffle(), |shuffled| {
            let repacked =
                archive(&shuffled).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let read = DatasetModel::read(repacked.as_slice())
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert!(
                read == expected,
                "a shuffled package reads to another model"
            );
            Ok(())
        })
        .map_err(|error| match error {
            // The failing input is the whole package, too large to print.
            TestError::Abort(reason) | TestError::Fail(reason, _) => reason.to_string(),
        })?;
    Ok(())
}
