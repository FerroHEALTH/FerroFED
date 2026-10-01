// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pin matrix (`docs/VERSIONS.md`) is the single source of truth for every
//! version; the constants this crate exposes must agree with it.

use std::error::Error;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_specification_release_matches_the_pin_matrix() -> Result<(), Box<dyn Error>> {
    let pin = ferrofed_testkit::matrix_pin("Federation Tier with AQL")?;
    assert_eq!(
        openehr_federation::FEDERATION_SPEC,
        pin,
        "the crate constant and the docs/VERSIONS.md row for Federation Tier with AQL disagree"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_aql_release_matches_the_pin_matrix() -> Result<(), Box<dyn Error>> {
    let pin = ferrofed_testkit::matrix_pin("openEHR AQL")?;
    assert_eq!(
        openehr_federation::AQL,
        pin,
        "the crate constant and the docs/VERSIONS.md row for openEHR AQL disagree"
    );
    Ok(())
}
