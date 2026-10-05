// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 6, follow-up read and write routing (§16.3 track 6).
//!
//! A follow-up read of a row reaches the member that holds it with its uid
//! unchanged, an `ehr_id` the gateway has not seen is found by the ask-all
//! probe, a write to one is refused, and a new EHR is created at the one member the client names
//! (§12, §12a.1, §16.3 track 6; N21, N22, N23, N41; CP-13, CP-15, CP-33).
//!
//! That the probe is read-only, that a refused write probes nobody, and that
//! no other node is asked is judged on node-side capture, which the
//! end-to-end suite reads after these checks.

use http::{Method, StatusCode};
use openehr_federation::headers::{ENDPOINT, SYSTEM_ID};
use uuid::Uuid;

use crate::conformance::client::{Gateway, answered, ask, get, post_aql, request};
use crate::conformance::fixture::{Fixture, Member};
use crate::conformance::scenarios::patient_compositions;
use crate::conformance::{Failure, ensure, ensure_eq};

/// Returns the uid of every composition the patient query answers, one per
/// composition the patient holds.
///
/// # Errors
///
/// Returns [`Failure`] when the query does not answer them.
pub async fn row_uids<G: Gateway>(gateway: &G, fixture: &Fixture) -> Result<Vec<String>, Failure> {
    let (_, answer) = answered(
        gateway,
        post_aql(&patient_compositions(fixture), &[])?,
        "the patient query",
    )
    .await?;
    let uids: Vec<String> = answer.rows.into_iter().flatten().collect();
    ensure_eq(
        &fixture.compositions(),
        &uids.len(),
        "one uid per composition the patient holds",
    )?;
    Ok(uids)
}

/// Returns the member whose `system_id` the version uid `uid` names
/// (`{object_id}::{creating_system_id}::{version}`, RM `OBJECT_VERSION_ID`).
///
/// # Errors
///
/// Returns [`Failure::Check`] when no holding member's `system_id` is in it.
pub fn owner<'f>(fixture: &'f Fixture, uid: &str) -> Result<&'f Member, Failure> {
    fixture
        .holding()
        .map(|(member, _)| member)
        .find(|member| uid.contains(&format!("::{}::", member.system_id)))
        .ok_or_else(|| Failure::Check(format!("{uid} names no holding member's system_id")))
}

/// Holds that a follow-up read of the composition `uid` reaches the member
/// that holds it, acting as that endpoint, with the uid unchanged in its
/// `ETag` (CP-13, CP-33); returns that member.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn follow_up_read<'f, G: Gateway>(
    gateway: &G,
    fixture: &'f Fixture,
    uid: &str,
) -> Result<&'f Member, Failure> {
    let member = owner(fixture, uid)?;
    let ehr_id = member
        .holding
        .as_ref()
        .map(|holding| holding.ehr_id.as_str())
        .ok_or_else(|| Failure::Check(format!("{} holds the patient", member.endpoint)))?;
    let followed = ask(
        gateway,
        get(&format!("/v1/ehr/{ehr_id}/composition/{uid}"), &[])?,
    )
    .await?;
    followed.expect(StatusCode::OK, "CP-33: the follow-up read")?;
    ensure_eq(
        &Some(member.endpoint.as_str()),
        &followed.field(ENDPOINT),
        "CP-13, CP-33: routed by the ehr_id the resolution bound to the member",
    )?;
    let etag = followed
        .field("etag")
        .ok_or_else(|| Failure::Check("the follow-up read carries the node's ETag".to_owned()))?;
    ensure(etag.contains(uid), || {
        format!("the uid comes back as the node wrote it: {etag}")
    })?;
    Ok(member)
}

/// Holds that a read of `ehr_id`, which only `endpoint` of `system_id`
/// holds and the gateway has not seen, is found by the ask-all probe and
/// answered by that member (§12.5.1 step 4, CP-33).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn probe_read<G: Gateway>(
    gateway: &G,
    endpoint: &str,
    system_id: &str,
    ehr_id: &str,
) -> Result<(), Failure> {
    let found = ask(gateway, get(&format!("/v1/ehr/{ehr_id}"), &[])?).await?;
    found.expect(StatusCode::OK, "CP-33: an ehr_id found by the probe")?;
    ensure_eq(
        &Some(endpoint),
        &found.field(ENDPOINT),
        "N31: the acting endpoint",
    )?;
    ensure_eq(
        &Some(system_id),
        &found.field(SYSTEM_ID),
        "N31: its system_id",
    )
}

/// Holds that a write to `ehr_id`, which no target, binding or index
/// routes, is refused `400` (§12.5.1, N41, CP-33).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn unrouted_write<G: Gateway>(gateway: &G, ehr_id: &str) -> Result<(), Failure> {
    let write = request(
        Method::POST,
        &format!("/v1/ehr/{ehr_id}/composition"),
        &[],
        Some(("application/json", b"{}".to_vec())),
    )?;
    ask(gateway, write).await?.expect(
        StatusCode::BAD_REQUEST,
        "CP-33: a write no target, binding or index routes",
    )
}

/// Holds that a write to an `ehr_id` no member holds is refused `400`, as
/// [`unrouted_write`] does.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn write_nowhere<G: Gateway>(gateway: &G) -> Result<(), Failure> {
    unrouted_write(gateway, &Uuid::new_v4().to_string()).await
}

/// Holds that a new EHR naming no member is refused `400 target-required`
/// (§12.4, CP-15).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn new_ehr_untargeted<G: Gateway>(gateway: &G) -> Result<(), Failure> {
    let untargeted = ask(gateway, request(Method::POST, "/v1/ehr", &[], None)?).await?;
    untargeted.expect(StatusCode::BAD_REQUEST, "CP-15: a new object names no node")?;
    ensure_eq(
        &"target-required".to_owned(),
        &untargeted.code()?,
        "CP-15: the refusal's code",
    )
}

/// Holds that a new EHR naming `member` is created there, acting as that
/// endpoint, with the node's own `Location` (§12.4, N31, CP-15); returns
/// the `Location`.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn new_ehr_at<G: Gateway>(gateway: &G, member: &Member) -> Result<String, Failure> {
    let named = request(
        Method::POST,
        "/v1/ehr",
        &[(ENDPOINT, member.endpoint.as_str())],
        None,
    )?;
    let created = ask(gateway, named).await?;
    created.expect(StatusCode::CREATED, "CP-15: a new EHR at the named member")?;
    ensure_eq(
        &Some(member.endpoint.as_str()),
        &created.field(ENDPOINT),
        "N31: the acting endpoint",
    )?;
    let location = created
        .field("location")
        .ok_or_else(|| Failure::Check("N31: the create carries the node's Location".to_owned()))?
        .to_owned();
    ensure(location.contains(&member.path), || {
        format!(
            "N31: the node's own Location, under {}: {location}",
            member.path
        )
    })?;
    Ok(location)
}
