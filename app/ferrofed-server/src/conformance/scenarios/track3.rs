// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 3, the directed query and the endpoint pin (§16.3 track 3).
//!
//! `FROM ENDPOINT` and `FROM ORGANISATION` select the node set and report
//! the rest excluded, ENDPOINT attributes selected are each row's
//! provenance, a named member where the patient does not resolve is
//! reported and not errored, the endpoint header selects what the directive selects, a
//! conflict between them is refused, and `?endpoint=` targets nothing (§8,
//! §9.4, §16.3 track 3; N10, N11, N12, N35; CP-6, CP-28, CP-35, CP-37).
//!
//! That a node not named is never asked, and that no directive or targeting
//! header reaches a node, is judged on node-side capture, which the
//! end-to-end suite reads after these checks.

use http::StatusCode;
use openehr_federation::headers::ENDPOINT;

use crate::conformance::client::{Federated, Gateway, answered, ask, post_aql};
use crate::conformance::fixture::Fixture;
use crate::conformance::scenarios::{Expected, directed_at, held_by, statuses, undirected};
use crate::conformance::{Failure, ensure, ensure_eq};

/// The patient's compositions from the members `directive` selects.
#[must_use]
pub fn directed(fixture: &Fixture, directive: &str) -> String {
    format!(
        "SELECT c/uid/value AS uid FROM {directive} CONTAINS EHR e CONTAINS COMPOSITION c WHERE {}",
        fixture.patient.predicate()
    )
}

/// The undirected form of [`directed`].
#[must_use]
pub fn undirected_query(fixture: &Fixture) -> String {
    format!(
        "SELECT c/uid/value AS uid FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        fixture.patient.predicate()
    )
}

/// The `ENDPOINT` selector naming `endpoints` (§8.1).
#[must_use]
pub fn endpoint_selector(endpoints: &[&str]) -> String {
    let named: Vec<String> = endpoints.iter().map(|id| format!("\"{id}\"")).collect();
    format!("ENDPOINT [{}]", named.join(", "))
}

/// Holds that `FROM ENDPOINT [endpoint]` asks that member alone, reports
/// every other one excluded, and leaves the row shape the client's (CP-6,
/// CP-37); returns the answer.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn directive_endpoint<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    endpoint: &str,
) -> Result<Federated, Failure> {
    let aql = directed(fixture, &endpoint_selector(&[endpoint]));
    let (_, answer) = answered(gateway, post_aql(&aql, &[])?, "CP-6: FROM ENDPOINT").await?;
    directed_at(
        &answer,
        fixture,
        &[endpoint],
        "CP-6: the directive selects the member and reports the rest excluded",
    )?;
    ensure_eq(
        &held_by(fixture, endpoint)?,
        &answer.rows.len(),
        "the named member's compositions alone",
    )?;
    ensure_eq(
        &vec!["uid"],
        &answer.names(),
        "CP-37: no ENDPOINT attribute selected, so the row shape is the client's",
    )?;
    Ok(answer)
}

/// Holds that selected ENDPOINT attributes are each row's provenance.
///
/// `p/id` and `p/system_id` of `FROM ENDPOINT p [named]` are added to every
/// row, in the client's column order: one row per composition each named
/// member holds, carrying that member's endpoint id and `system_id`, every
/// other member reported excluded (§9.4, N12; CP-35, CP-37); returns the
/// answer.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn endpoint_attributes<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    named: &[&str],
) -> Result<Federated, Failure> {
    let listed: Vec<String> = named.iter().map(|id| format!("\"{id}\"")).collect();
    let aql = format!(
        "SELECT p/id AS endpoint_id, p/system_id AS system_id, c/uid/value AS composition_id \
         FROM ENDPOINT p [{}] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {}",
        listed.join(", "),
        fixture.patient.predicate()
    );
    let (_, answer) = answered(
        gateway,
        post_aql(&aql, &[])?,
        "CP-37: the ENDPOINT attributes selected",
    )
    .await?;
    ensure_eq(
        &vec!["endpoint_id", "system_id", "composition_id"],
        &answer.names(),
        "CP-37: the selected attributes added to the row, in the client's order",
    )?;
    directed_at(&answer, fixture, named, "CP-37: the directive's node set")?;
    let mut expected = 0_usize;
    for endpoint in named {
        let member = fixture
            .member(endpoint)
            .ok_or_else(|| Failure::Check(format!("{endpoint} is a member")))?;
        let held = held_by(fixture, endpoint)?;
        expected = expected.saturating_add(held);
        let carried: Vec<&Vec<String>> = answer
            .rows
            .iter()
            .filter(|row| row.first().map(String::as_str) == Some(*endpoint))
            .collect();
        ensure_eq(
            &held,
            &carried.len(),
            &format!("N12: one row per composition {endpoint} holds, each naming it"),
        )?;
        ensure(
            carried
                .iter()
                .all(|row| row.get(1).map(String::as_str) == Some(member.system_id.as_str())),
            || {
                format!(
                    "§9.4: each row of {endpoint} carries its system_id {}",
                    member.system_id
                )
            },
        )?;
    }
    ensure_eq(
        &expected,
        &answer.rows.len(),
        "every row comes from a named member",
    )?;
    Ok(answer)
}

/// Holds that `FROM ORGANISATION [organisation]` asks the endpoints that
/// organisation manages and reports every other member excluded (CP-6);
/// returns the answer.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn directive_organisation<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    organisation: &str,
) -> Result<Federated, Failure> {
    let aql = directed(fixture, &format!("ORGANISATION [\"{organisation}\"]"));
    let (_, answer) = answered(gateway, post_aql(&aql, &[])?, "CP-6: FROM ORGANISATION").await?;
    statuses(
        &answer,
        fixture,
        |member| match (member.organisation == organisation, &member.holding) {
            (true, Some(_)) => Expected::Is("active"),
            (true, None) => Expected::NotActive,
            (false, _) => Expected::Is("excluded"),
        },
        "CP-6: the organisation's endpoints are the node set",
    )?;
    Ok(answer)
}

/// Holds that a named member where the patient does not resolve is reported.
///
/// A directive naming `holding`, which holds the patient, and `unresolved`,
/// where the patient does not resolve, answers `200` with the first
/// member's rows and the second reported `not-resolved` (§16.3 track 3,
/// CP-6).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn named_unresolved<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    holding: &str,
    unresolved: &str,
) -> Result<Federated, Failure> {
    let aql = directed(fixture, &endpoint_selector(&[holding, unresolved]));
    let (_, answer) = answered(
        gateway,
        post_aql(&aql, &[])?,
        "CP-6: a named member where the patient is unknown is reported, not an error",
    )
    .await?;
    statuses(
        &answer,
        fixture,
        |member| {
            if member.endpoint == holding {
                Expected::Is("active")
            } else if member.endpoint == unresolved {
                Expected::Is("not-resolved")
            } else {
                Expected::Is("excluded")
            }
        },
        "CP-6: the unknown member is not-resolved",
    )?;
    ensure_eq(
        &held_by(fixture, holding)?,
        &answer.rows.len(),
        "the holding member's rows",
    )?;
    Ok(answer)
}

/// Holds that the endpoint header naming `endpoint` selects the node set
/// and rows the directive naming it selects (§8.4, CP-28).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn header_selects<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    endpoint: &str,
) -> Result<(), Failure> {
    let by_directive = directed(fixture, &endpoint_selector(&[endpoint]));
    let (_, by_directive) = answered(
        gateway,
        post_aql(&by_directive, &[])?,
        "CP-28: the directive",
    )
    .await?;
    let (_, by_header) = answered(
        gateway,
        post_aql(&undirected_query(fixture), &[(ENDPOINT, endpoint)])?,
        "CP-28: the endpoint header",
    )
    .await?;
    ensure_eq(
        &by_directive.statuses(),
        &by_header.statuses(),
        "CP-28: the header and the directive select the same node set",
    )?;
    ensure_eq(
        &by_directive.sorted_rows(),
        &by_header.sorted_rows(),
        "CP-28: the same rows",
    )?;
    directed_at(
        &by_header,
        fixture,
        &[endpoint],
        "CP-28: the header's node set",
    )
}

/// Holds that a directive naming `directed` and a header naming `header`
/// in one request is refused `400 targeting-conflict` (§8.4, CP-28).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn conflict_refused<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    directed_to: &str,
    header: &str,
) -> Result<(), Failure> {
    let aql = directed(fixture, &endpoint_selector(&[directed_to]));
    let conflicting = ask(gateway, post_aql(&aql, &[(ENDPOINT, header)])?).await?;
    conflicting.expect(
        StatusCode::BAD_REQUEST,
        "CP-28: conflicting node sets in one request",
    )?;
    ensure_eq(
        &"targeting-conflict".to_owned(),
        &conflicting.code()?,
        "CP-28: the refusal's code",
    )
}

/// Holds that `?endpoint=` on the query is not a targeting mechanism: the
/// query asks every member as an undirected one does (§8.4, CP-28).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn parameter_targets_nothing<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    endpoint: &str,
) -> Result<(), Failure> {
    let mut request = post_aql(
        &crate::conformance::scenarios::patient_compositions(fixture),
        &[],
    )?;
    let uri = format!("/v1/query/aql?endpoint={endpoint}");
    *request.uri_mut() = uri
        .parse()
        .map_err(|error| Failure::Check(format!("{uri} is no request target: {error}")))?;
    let (_, unpinned) = answered(gateway, request, "CP-28: ?endpoint=").await?;
    undirected(
        &unpinned,
        fixture,
        "CP-28: ?endpoint= is not a targeting mechanism",
    )
}
