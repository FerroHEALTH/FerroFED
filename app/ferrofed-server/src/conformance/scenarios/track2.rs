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

use openehr_query::ast::Primitive;
use openehr_query::bind::Parameters;
use secrecy::ExposeSecret;

use crate::conformance::aql::ClientQuery;
use crate::conformance::client::{Federated, Gateway, answered, post_aql};
use crate::conformance::fixture::Fixture;
use crate::conformance::scenarios::undirected;
use crate::conformance::{Failure, ensure, ensure_eq};

/// Each composition holding an observation, before the query names the
/// patient.
const OBSERVED: &str =
    "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";

/// The patient's condition through an `ENTRY`-level `subject` of an
/// `OBSERVATION` bound to `o`.
const ENTRY_SUBJECT: &str = "SELECT o FROM OBSERVATION o \
     WHERE o/subject/identifiers/id = $patient_id \
     AND o/subject/identifiers/issuer = $patient_issuer";

/// The patient's compositions holding an observation, through
/// `EHR_STATUS.subject.external_ref`.
///
/// # Errors
///
/// Returns [`Failure::Query`] when the query cannot be built from its
/// template, which a fixed template never gives cause for.
pub fn via_external_ref(fixture: &Fixture) -> Result<String, Failure> {
    Ok(ClientQuery::parse(OBSERVED)?
        .of_patient(&fixture.patient)?
        .to_aql())
}

/// The same query through an `ENTRY`-level `subject` (§5.4.3, N33).
///
/// # Errors
///
/// Returns [`Failure::Query`] when the query cannot be built from its
/// template, which a fixed template never gives cause for.
pub fn via_entry(fixture: &Fixture) -> Result<String, Failure> {
    let mut parameters = Parameters::new();
    parameters.insert(
        "patient_id",
        Primitive::String(fixture.patient.value().expose_secret().to_owned()),
    );
    parameters.insert(
        "patient_issuer",
        Primitive::String(fixture.patient.namespace().to_owned()),
    );
    Ok(ClientQuery::parse(OBSERVED)?
        .and_where(ENTRY_SUBJECT, &parameters)?
        .to_aql())
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
    for aql in [via_external_ref(fixture)?, via_entry(fixture)?] {
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
    let aql = ClientQuery::parse(
        "SELECT e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value AS uid \
         FROM EHR e CONTAINS COMPOSITION c",
    )?
    .of_patient(&fixture.patient)?
    .to_aql();
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
