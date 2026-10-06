// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 4, partial results (§16.3 track 4).
//!
//! A patient found nowhere is a `200` with empty rows, never a `424` or a
//! `404`, and every member reports why it gave no row (§11.1, §11.3, §16.3
//! track 4; N6, N16; CP-12, CP-30).
//!
//! The statuses of a node that answers an error, is unreachable or is slow,
//! the client wait, best-effort completion and a consent refusal need a
//! fault injected at a node or a gateway configured for the scenario, which
//! the end-to-end suite provides.

use http::StatusCode;

use crate::conformance::aql::ClientQuery;
use crate::conformance::client::{Federated, Gateway, ask, checked, post_aql};
use crate::conformance::fixture::{Fixture, SyntheticPatient};
use crate::conformance::scenarios::{COMPOSITION_UIDS, UNRESOLVED, statuses};
use crate::conformance::{Failure, ensure, ensure_eq};

/// Holds that a patient found nowhere is a `200` with no row.
///
/// A query for `unknown`, a patient the cross-reference knows nowhere,
/// answers `200` with no row, every member not resolved or not
/// localized, and `complete` false exactly when a member is not resolved
/// (§11.3 not-resolved carve-out; §16.3 track 4).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn found_nowhere<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    unknown: &SyntheticPatient,
) -> Result<Federated, Failure> {
    let aql = ClientQuery::parse(COMPOSITION_UIDS)?
        .of_patient(unknown)?
        .to_aql();
    let reply = ask(gateway, post_aql(&aql, &[])?).await?;
    reply.expect(
        StatusCode::OK,
        "CP-12: found nowhere is a 200, never a 424 or a 404",
    )?;
    let answer = checked(gateway, &reply)?;
    ensure(answer.rows.is_empty(), || "CP-12: empty rows".to_owned())?;
    statuses(
        &answer,
        fixture,
        |_| UNRESOLVED,
        "CP-12: every member reports the patient unknown",
    )?;
    let unresolved = answer
        .statuses()
        .iter()
        .any(|(_, status)| *status == "not-resolved");
    ensure_eq(
        &!unresolved,
        &answer.meta.federation.complete,
        "CP-30: a not-resolved member clears complete, a not-localized one does not",
    )?;
    Ok(answer)
}
