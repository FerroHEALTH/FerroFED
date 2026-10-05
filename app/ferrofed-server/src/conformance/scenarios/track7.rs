// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 7, auth conveyance: the caller authenticates to the gateway, and
//! one that does not is refused before anything else (§13.1, §16.3 track 7;
//! N25; CP-17).
//!
//! The token each node receives, the caller's identity conveyed to it, and
//! the three consent configurations are judged on node-side capture, with a
//! consent refusal answered at a node's proxy or a consent pre-filter
//! configured for the scenario, which the end-to-end suite provides.

use http::StatusCode;

use crate::conformance::Failure;
use crate::conformance::client::{Gateway, ask_anonymous, post_aql};
use crate::conformance::fixture::Fixture;
use crate::conformance::scenarios::patient_compositions;

/// Holds that the patient query with no credential is refused `401`
/// (CP-17).
///
/// # Errors
///
/// Returns [`Failure`] naming the expectation that did not hold.
pub async fn unauthenticated<G: Gateway>(gateway: &G, fixture: &Fixture) -> Result<(), Failure> {
    let anonymous = post_aql(&patient_compositions(fixture), &[])?;
    ask_anonymous(gateway, anonymous).await?.expect(
        StatusCode::UNAUTHORIZED,
        "CP-17: the client authenticates to the gateway",
    )
}
