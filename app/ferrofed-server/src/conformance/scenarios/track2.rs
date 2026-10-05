// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 2, `subject` to `ehr_id` resolution (§16.3 track 2).
//!
//! The same query in both patient carriers resolves at every member that
//! holds the patient and returns the same rows, and a selected subject
//! column is the client's own input (§16.3 track 2; N3, N5, N33; CP-3, CP-7,
//! CP-38).
//!
//! That each node is asked by its own `ehr_id` and never receives `subject`
//! is judged on node-side capture (CP-2, CP-4), which the end-to-end suite
//! reads after these checks.

use secrecy::ExposeSecret;

use crate::conformance::client::{Federated, Gateway, answered, post_aql};
use crate::conformance::fixture::Fixture;
use crate::conformance::scenarios::undirected;
use crate::conformance::{Failure, ensure, ensure_eq};

/// The patient's compositions holding an observation, through
/// `EHR_STATUS.subject.external_ref`.
#[must_use]
pub fn via_external_ref(fixture: &Fixture) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o WHERE {}",
        fixture.patient.predicate()
    )
}

/// The same query through an `ENTRY`-level `subject` (§5.4.3, N33).
#[must_use]
pub fn via_entry(fixture: &Fixture) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o \
         WHERE o/subject/identifiers/id = '{}' \
         AND o/subject/identifiers/issuer = '{}'",
        fixture.patient.value().expose_secret(),
        fixture.patient.namespace()
    )
}

/// Holds that the query through either carrier resolves at every member
/// that holds the patient and that both return the same, non-empty rows;
/// returns both answers, the `external_ref` carrier's first.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn both_carriers<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
) -> Result<Vec<Federated>, Failure> {
    let mut rows = Vec::new();
    let mut answers = Vec::new();
    for aql in [via_external_ref(fixture), via_entry(fixture)] {
        let (_, answer) =
            answered(gateway, post_aql(&aql, &[])?, "CP-38: a patient carrier").await?;
        undirected(
            &answer,
            fixture,
            "CP-3: the subject resolved at every member holding it",
        )?;
        rows.push(answer.sorted_rows());
        answers.push(answer);
    }
    ensure(rows.first().is_some_and(|first| !first.is_empty()), || {
        "the seeded compositions hold observations, so the comparison is not vacuous".to_owned()
    })?;
    ensure(rows.first() == rows.get(1), || {
        "CP-38: both carriers return the same rows".to_owned()
    })?;
    Ok(answers)
}

/// Holds that a selected subject column answers the client's own input in
/// every row, one row per composition (§5.4.2, CP-7).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn subject_column<G: Gateway>(gateway: &G, fixture: &Fixture) -> Result<(), Failure> {
    let aql = format!(
        "SELECT e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value AS uid \
         FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        fixture.patient.predicate()
    );
    let (_, answer) = answered(gateway, post_aql(&aql, &[])?, "CP-7: the subject column").await?;
    ensure_eq(&vec!["patient", "uid"], &answer.names(), "the columns")?;
    ensure_eq(
        &fixture.compositions(),
        &answer.rows.len(),
        "one row per composition the patient holds",
    )?;
    let value = fixture.patient.value().expose_secret();
    ensure(
        answer
            .rows
            .iter()
            .all(|row| row.first().map(String::as_str) == Some(value)),
        || "CP-7: the subject column is the re-injected input in every row".to_owned(),
    )
}
