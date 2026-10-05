// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 5, de-duplication and `DISTINCT` (§16.3 track 5).
//!
//! The default passes duplicates through, `DISTINCT`, `ORDER BY` and
//! `LIMIT` are applied at the Tier over the union, and an undirected
//! aggregate is recombined or refused while a directed one is answered (§9.5, §10, §11.6, §16.3 track 5; N13, N14,
//! N15, N39; CP-8, CP-9, CP-10, CP-32).
//!
//! The seeded compositions share their name, so a projection of the name
//! is a duplicate across the members. That `OFFSET` never reaches a node is
//! judged on node-side capture, and the same top row whichever node answers
//! last needs a delay injected at a node; the end-to-end suite provides
//! both.

use http::StatusCode;

use crate::conformance::client::{Counted, Gateway, Reply, answered, ask, post_aql};
use crate::conformance::fixture::Fixture;
use crate::conformance::scenarios::{as_count, held_by};
use crate::conformance::{Failure, ensure, ensure_eq};

/// The patient's compositions, projecting `select`, followed by `tail`.
#[must_use]
pub fn compositions(fixture: &Fixture, select: &str, tail: &str) -> String {
    format!(
        "SELECT {select} FROM EHR e CONTAINS COMPOSITION c WHERE {} {tail}",
        fixture.patient.predicate()
    )
}

/// Holds that a projection every member answers alike passes one row per
/// member through by default, and that `DISTINCT` folds them into one row
/// at the Tier (CP-8, CP-9).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn duplicates_and_distinct<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
) -> Result<(), Failure> {
    let plain = compositions(fixture, "c/name/value AS name", "");
    let (_, plain) = answered(
        gateway,
        post_aql(&plain, &[])?,
        "CP-9: the plain projection",
    )
    .await?;
    ensure_eq(
        &fixture.compositions(),
        &plain.rows.len(),
        "CP-9: one row per composition",
    )?;
    ensure(plain.rows.len() > 1, || {
        "CP-9: needs a duplicate across the members to pass through".to_owned()
    })?;
    let first = plain.rows.first().cloned();
    ensure(
        plain.rows.iter().all(|row| Some(row) == first.as_ref()),
        || "CP-9: the default passes the duplicate through".to_owned(),
    )?;
    let distinct = compositions(fixture, "DISTINCT c/name/value AS name", "");
    let (_, distinct) = answered(
        gateway,
        post_aql(&distinct, &[])?,
        "CP-8: the DISTINCT projection",
    )
    .await?;
    ensure_eq(
        &first.into_iter().collect::<Vec<_>>(),
        &distinct.rows,
        "CP-8: DISTINCT folds the copies into one row at the Tier",
    )
}

/// Holds that `ORDER BY` with `LIMIT`, and with `LIMIT` and `OFFSET`,
/// return the global rows of the union, never a per-node page; returns the
/// sorted union (CP-8, CP-32).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn order_and_limit<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
) -> Result<Vec<Vec<String>>, Failure> {
    let uid = "c/uid/value AS uid";
    let union = compositions(fixture, uid, "");
    let (_, union) = answered(gateway, post_aql(&union, &[])?, "the union").await?;
    let union = union.sorted_rows();
    ensure(union.len() > 1, || {
        "CP-32: needs rows from more than one member".to_owned()
    })?;
    for (tail, expected) in [
        ("ORDER BY c/uid/value ASC LIMIT 1", union.first()),
        ("ORDER BY c/uid/value DESC LIMIT 1", union.last()),
        ("ORDER BY c/uid/value ASC LIMIT 1 OFFSET 1", union.get(1)),
    ] {
        let aql = compositions(fixture, uid, tail);
        let (_, page) = answered(gateway, post_aql(&aql, &[])?, tail).await?;
        ensure_eq(
            &expected.cloned().into_iter().collect::<Vec<_>>(),
            &page.rows,
            &format!("CP-32: {tail} over the union of every member"),
        )?;
    }
    Ok(union)
}

/// The undirected `COUNT` of the patient's compositions.
#[must_use]
pub fn count(fixture: &Fixture) -> String {
    compositions(fixture, "COUNT(c/uid/value) AS n", "")
}

/// Returns the count rows of a `200` answer.
fn counts<G: Gateway>(gateway: &G, reply: &Reply) -> Result<Vec<Vec<u64>>, Failure> {
    gateway
        .check_result_set(&reply.text)
        .map_err(|why| Failure::Check(format!("the RESULT_SET does not hold: {why}")))?;
    serde_json::from_str::<Counted>(&reply.text)
        .map(|counted| counted.rows)
        .map_err(|source| Failure::Read {
            what: "the counted RESULT_SET".to_owned(),
            source,
        })
}

/// Holds that an undirected `COUNT` the gateway declares decomposable is
/// one cross-node-correct row, never one row per node (CP-10, CP-32).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn aggregate_recombined<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
) -> Result<(), Failure> {
    let reply = ask(gateway, post_aql(&count(fixture), &[])?).await?;
    reply.expect(StatusCode::OK, "CP-10: a declared undirected aggregate")?;
    let total = as_count(fixture.compositions())?;
    ensure_eq(
        &vec![vec![total]],
        &counts(gateway, &reply)?,
        "CP-10, CP-32: one cross-node-correct row",
    )
}

/// Holds that an undirected `COUNT` the gateway does not declare
/// decomposable is refused `400 undirected-aggregate` (CP-10, CP-32).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn aggregate_refused<G: Gateway>(gateway: &G, fixture: &Fixture) -> Result<(), Failure> {
    let refused = ask(gateway, post_aql(&count(fixture), &[])?).await?;
    refused.expect(
        StatusCode::BAD_REQUEST,
        "CP-10: an undirected aggregate that is not cross-node-correct",
    )?;
    ensure_eq(
        &"undirected-aggregate".to_owned(),
        &refused.code()?,
        "CP-32: refused with a reason",
    )
}

/// Holds that a `COUNT` directed at `endpoint` alone is answered with that
/// member's count (CP-10).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn aggregate_directed<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    endpoint: &str,
) -> Result<(), Failure> {
    let aql = format!(
        "SELECT COUNT(c/uid/value) AS n FROM ENDPOINT [\"{endpoint}\"] CONTAINS EHR e \
         CONTAINS COMPOSITION c WHERE {}",
        fixture.patient.predicate()
    );
    let single = ask(gateway, post_aql(&aql, &[])?).await?;
    single.expect(StatusCode::OK, "CP-10: a directed single-node aggregate")?;
    let held = as_count(held_by(fixture, endpoint)?)?;
    ensure_eq(
        &vec![vec![held]],
        &counts(gateway, &single)?,
        "CP-10: the directed member's count",
    )
}
