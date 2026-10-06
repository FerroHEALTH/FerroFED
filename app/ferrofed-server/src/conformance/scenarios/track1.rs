// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 1, transparency (§16.3 track 1).
//!
//! An unmodified openEHR client sends a plain patient AQL and gets one
//! single-CDR-shaped `RESULT_SET`, with no endpoint column it did not
//! select, by `POST` and by the ITS-REST `GET` form (§16.3 track 1; N1, N2,
//! N17, N18; CP-1, CP-35).

use crate::conformance::aql::ClientQuery;
use crate::conformance::client::{Federated, Gateway, answered, get, percent_encoded, post_aql};
use crate::conformance::fixture::Fixture;
use crate::conformance::scenarios::{as_count, undirected};
use crate::conformance::{Failure, ensure, ensure_eq};

/// The plain patient query a client sends, its one column aliased.
///
/// # Errors
///
/// Returns [`Failure::Query`] when the query cannot be built from its
/// template, which a fixed template never gives cause for.
pub fn plain_query(fixture: &Fixture) -> Result<String, Failure> {
    Ok(ClientQuery::parse(
        "SELECT c/uid/value AS composition_uid FROM EHR e CONTAINS COMPOSITION c",
    )?
    .of_patient(&fixture.patient)?
    .to_aql())
}

/// Holds that the plain patient query answers one single-CDR-shaped result.
///
/// The result set has the client's columns, with the federation additions under `meta.federation`
/// only, one row per composition the patient holds, and the same rows by
/// `GET` as by `POST`; returns the `POST` answer.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn single_cdr_shaped<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
) -> Result<Federated, Failure> {
    let aql = plain_query(fixture)?;
    let (posted, answer) =
        answered(gateway, post_aql(&aql, &[])?, "CP-1: the patient query").await?;
    ensure_eq(
        &vec!["composition_uid"],
        &answer.names(),
        "CP-35: the client's columns and no endpoint column it did not select",
    )?;
    ensure_eq(
        &fixture.compositions(),
        &answer.rows.len(),
        "one row per composition the patient holds",
    )?;
    ensure(
        answer
            .rows
            .iter()
            .all(|row| row.len() == answer.columns.len()),
        || "CP-35: every row is an ordered array matching columns[]".to_owned(),
    )?;
    ensure(
        answer.meta.complete.is_none() && answer.meta.endpoints.is_none(),
        || "CP-35: no flat meta.complete or meta.endpoints".to_owned(),
    )?;
    for prefixed in ["\"_complete\"", "\"_endpoints\"", "\"_federation\""] {
        ensure(!posted.text.contains(prefixed), || {
            format!("CP-35: no prefixed {prefixed} member")
        })?;
    }
    undirected(
        &answer,
        fixture,
        "CP-35: the additions nested under meta.federation",
    )?;
    for (member, holding) in fixture.holding() {
        let expected = Some(as_count(holding.compositions)?);
        ensure_eq(
            &expected,
            &answer.endpoint(&member.endpoint)?.row_count,
            "N16: each endpoint's row count",
        )?;
    }
    let encoded = percent_encoded(&aql);
    let (_, fetched) = answered(
        gateway,
        get(&format!("/v1/query/aql?q={encoded}"), &[])?,
        "CP-1: the ITS-REST GET form",
    )
    .await?;
    ensure_eq(
        &answer.sorted_rows(),
        &fetched.sorted_rows(),
        "CP-1: the ITS-REST GET form answers as the POST form does",
    )?;
    Ok(answer)
}
