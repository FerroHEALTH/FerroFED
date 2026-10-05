// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 9, the REST surface and the self-description (§16.3 track 9).
//!
//! A plain client given only the base URL reads and writes, both addressing
//! forms answer the same rows, `OPTIONS {base}/` lists the members and
//! DEMOGRAPHIC answers as it declares, a definition request routes to the one member named, and a
//! stored query the gateway holds runs by name over every member (§7a,
//! §12.6, §12.7, §16.3 track 9; N28 to N32, N43, N44; CP-21 to CP-25,
//! CP-34, CP-40).
//!
//! That the gateway's base never reaches a node, that a refused request
//! asks nobody, and the definition fan-out's per-node failure need
//! node-side capture or an injected fault, which the end-to-end suite
//! provides.

use std::collections::BTreeMap;

use http::{Method, StatusCode};
use openehr_federation::headers::ENDPOINT;
use openehr_federation::options::OptionsRoot;
use secrecy::ExposeSecret;
use serde::Serialize;

use crate::conformance::client::{Gateway, Reply, answered, ask, checked, get, post_aql, request};
use crate::conformance::fixture::{Fixture, Member};
use crate::conformance::scenarios::{directed_at, undirected};
use crate::conformance::{Failure, ensure, ensure_eq};

/// The declaration of a gateway that federates no DEMOGRAPHIC request.
pub const DEMOGRAPHIC_UNSUPPORTED: &str = "unsupported: 501";

/// The query of the EHR `ehr_id` in the `WHERE` form of N29.
#[must_use]
pub fn where_form(ehr_id: &str) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{ehr_id}'"
    )
}

/// The same query in the `FROM EHR` form of N29.
#[must_use]
pub fn from_form(ehr_id: &str) -> String {
    format!("SELECT c/uid/value FROM EHR e[ehr_id/value='{ehr_id}'] CONTAINS COMPOSITION c")
}

/// Returns the gateway's self-description, `OPTIONS {base}/`, held to
/// [`Gateway::check_options`] (§7a.2, N30).
///
/// # Errors
///
/// Returns [`Failure`] when it is not a `200` self-description.
pub async fn options<G: Gateway>(gateway: &G) -> Result<OptionsRoot, Failure> {
    let described = ask(gateway, request(Method::OPTIONS, "/", &[], None)?).await?;
    described.expect(StatusCode::OK, "CP-23: OPTIONS {base}/")?;
    gateway
        .check_options(&described.text)
        .map_err(|why| Failure::Check(format!("the OPTIONS body does not hold: {why}")))?;
    serde_json::from_str(&described.text).map_err(|source| Failure::Read {
        what: "the OPTIONS body".to_owned(),
        source,
    })
}

/// Holds that a plain client given only the base URL reads and writes.
///
/// The client reads the self-description, queries `member`'s EHR `ehr_id`
/// holding `compositions` in both addressing forms of N29 with the same
/// rows, reads that EHR acting as `member`, and commits `composition` there, answered with the node's own `Location`
/// (N28, N29, N31; CP-21, CP-22, CP-24); returns the commit's answer.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn plain_client<G: Gateway>(
    gateway: &G,
    member: &Member,
    (ehr_id, compositions): (&str, usize),
    composition: &str,
) -> Result<Reply, Failure> {
    options(gateway).await?;
    let (_, where_rows) = answered(
        gateway,
        post_aql(&where_form(ehr_id), &[])?,
        "N29: the WHERE form",
    )
    .await?;
    let (_, from_rows) = answered(
        gateway,
        post_aql(&from_form(ehr_id), &[])?,
        "N29: the FROM EHR form",
    )
    .await?;
    ensure_eq(
        &compositions,
        &where_rows.rows.len(),
        "the EHR's compositions",
    )?;
    ensure_eq(
        &where_rows.rows,
        &from_rows.rows,
        "N29: both forms answer the same rows",
    )?;

    let read = ask(gateway, get(&format!("/v1/ehr/{ehr_id}"), &[])?).await?;
    read.expect(StatusCode::OK, "the read at the canonical path")?;
    ensure_eq(
        &Some(member.endpoint.as_str()),
        &read.field(ENDPOINT),
        "N31: the acting endpoint",
    )?;

    commit(gateway, member, ehr_id, composition, &[]).await
}

/// Holds that a commit of `composition` to `ehr_id`, with the header
/// fields `fields`, is created at `member`.
///
/// The answer is a `201` naming `member` as the acting endpoint, with the
/// node's own `Location` under its base (§7a.3, N31; CP-24); returns it.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn commit<G: Gateway>(
    gateway: &G,
    member: &Member,
    ehr_id: &str,
    composition: &str,
    fields: &[(&str, &str)],
) -> Result<Reply, Failure> {
    let write = request(
        Method::POST,
        &format!("/v1/ehr/{ehr_id}/composition"),
        fields,
        Some(("application/json", composition.as_bytes().to_vec())),
    )?;
    let written = ask(gateway, write).await?;
    written.expect(StatusCode::CREATED, "the write")?;
    ensure_eq(
        &Some(member.endpoint.as_str()),
        &written.field(ENDPOINT),
        "N31: the acting endpoint of the write",
    )?;
    let location = written
        .field("location")
        .ok_or_else(|| Failure::Check("N31: the write carries the node's Location".to_owned()))?;
    ensure(location.contains(&member.path), || {
        format!(
            "N31: the node's own Location, unmodified, under {}: {location}",
            member.path
        )
    })?;
    Ok(written)
}

/// Holds the self-description and the DEMOGRAPHIC behaviour it declares.
///
/// `OPTIONS {base}/` lists every member with its `system_id`, in registry
/// order, asked for no patient, and that a DEMOGRAPHIC read
/// answers as `its_rest.demographic` declares (§7a.1, §7a.2, N30, N32;
/// CP-23, CP-25); returns the self-description.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn self_description<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
) -> Result<OptionsRoot, Failure> {
    let root = options(gateway).await?;
    let listed: Vec<(&str, Option<&str>)> = root
        .endpoints
        .iter()
        .map(|member| (member.id.as_str(), member.system_id.as_deref()))
        .collect();
    let members: Vec<(&str, Option<&str>)> = fixture
        .members
        .iter()
        .map(|member| (member.endpoint.as_str(), Some(member.system_id.as_str())))
        .collect();
    ensure_eq(
        &members,
        &listed,
        "CP-23: the member endpoints behind the gateway",
    )?;
    let read = ask(gateway, get("/v1/demographic/person/synthetic-party", &[])?).await?;
    if root.federation.its_rest.demographic.as_str() == DEMOGRAPHIC_UNSUPPORTED {
        read.expect(
            StatusCode::NOT_IMPLEMENTED,
            "CP-25: the DEMOGRAPHIC API is not federated, as declared",
        )?;
    } else {
        read.expect(
            StatusCode::BAD_REQUEST,
            "CP-25: a DEMOGRAPHIC request naming no endpoint, as declared",
        )?;
        ensure_eq(
            &"target-required".to_owned(),
            &read.code()?,
            "CP-25: the refusal's code",
        )?;
    }
    Ok(root)
}

/// The template list every definition scenario reads.
pub const TEMPLATES: &str = "/v1/definition/template/adl1.4";

/// Holds that the template list naming `endpoint` is that member's answer,
/// acting as it (§12.6, N43, CP-34).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn definition_named<G: Gateway>(gateway: &G, endpoint: &str) -> Result<(), Failure> {
    let listed = ask(gateway, get(TEMPLATES, &[(ENDPOINT, endpoint)])?).await?;
    listed.expect(
        StatusCode::OK,
        "CP-34: the template list of the named member",
    )?;
    ensure_eq(
        &Some(endpoint),
        &listed.field(ENDPOINT),
        "N31: the acting endpoint",
    )
}

/// Holds that the template list naming no member, or `target`, which
/// chooses no one member, is refused `400` (§12.6, N43, CP-34).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn definition_unnamed<G: Gateway>(
    gateway: &G,
    target: Option<&str>,
) -> Result<(), Failure> {
    let fields: Vec<(&str, &str)> = target
        .map(|target| (ENDPOINT, target))
        .into_iter()
        .collect();
    ask(gateway, get(TEMPLATES, &fields)?).await?.expect(
        StatusCode::BAD_REQUEST,
        &format!("CP-34: {target:?} chooses no one node"),
    )
}

/// The patient's compositions over `from`, the identifier bound through
/// `$patient` so the definition the registry holds carries none (§5.4.1).
#[must_use]
pub fn parameterised(fixture: &Fixture, from: &str) -> String {
    format!(
        "SELECT c/uid/value AS uid FROM {from} CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        fixture.patient.namespace()
    )
}

/// Returns `PUT {base}/v1/definition/query/{name}/{version}` with `aql`.
///
/// # Errors
///
/// Returns [`Failure::Check`] when the name is no path.
pub fn put(name: &str, version: &str, aql: &str) -> Result<http::Request<Vec<u8>>, Failure> {
    request(
        Method::PUT,
        &format!("/v1/definition/query/{name}/{version}"),
        &[],
        Some(("text/plain", aql.as_bytes().to_vec())),
    )
}

/// Returns `POST {base}/v1/query/{name}` binding `$patient` to the patient.
///
/// # Errors
///
/// Returns [`Failure`] when the body cannot be written.
pub fn invoke(fixture: &Fixture, name: &str) -> Result<http::Request<Vec<u8>>, Failure> {
    #[derive(Serialize)]
    struct Invocation<'a> {
        query_parameters: BTreeMap<&'a str, &'a str>,
    }
    let body = serde_json::to_vec(&Invocation {
        query_parameters: BTreeMap::from([("patient", fixture.patient.value().expose_secret())]),
    })
    .map_err(|source| Failure::Read {
        what: "the invocation body".to_owned(),
        source,
    })?;
    request(
        Method::POST,
        &format!("/v1/query/{name}"),
        &[],
        Some(("application/json", body)),
    )
}

/// Holds that the undirected definition stored at the gateway as `name` at
/// `version` runs by name over every member holding the patient, its answer
/// naming the query (N44, CP-40).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn store_and_run<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    (name, version): (&str, &str),
) -> Result<(), Failure> {
    let stored = ask(
        gateway,
        put(name, version, &parameterised(fixture, "EHR e"))?,
    )
    .await?;
    stored.expect(StatusCode::OK, "CP-40: the definition is stored")?;
    let ran = ask(gateway, invoke(fixture, name)?).await?;
    ran.expect(StatusCode::OK, "CP-40: invoked by name")?;
    let answer = checked(gateway, &ran)?;
    ensure_eq(
        &Some(name),
        &answer.name.as_deref(),
        "CP-40: the ITS-REST name member names the gateway's query",
    )?;
    undirected(&answer, fixture, "CP-40: invoked by name, it fans out")?;
    ensure_eq(
        &fixture.compositions(),
        &answer.rows.len(),
        "rows from every holding member",
    )
}

/// Holds that a second `PUT` of `name` at `version` is refused `409`
/// (N44, CP-40).
///
/// # Errors
///
/// Returns [`Failure`] naming the expectation that did not hold.
pub async fn second_put_refused<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    (name, version): (&str, &str),
) -> Result<(), Failure> {
    ask(
        gateway,
        put(name, version, &parameterised(fixture, "EHR e"))?,
    )
    .await?
    .expect(
        StatusCode::CONFLICT,
        "CP-40: a second PUT to the same name and version is refused",
    )
}

/// Holds that a definition directed at `endpoint`, stored as `name` at
/// `version`, stays executable federated: run by name, it asks that member
/// alone and reports the rest excluded (N44, CP-40).
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn directed_store_and_run<G: Gateway>(
    gateway: &G,
    fixture: &Fixture,
    (name, version): (&str, &str),
    endpoint: &str,
) -> Result<(), Failure> {
    let from = format!("ENDPOINT [\"{endpoint}\"] CONTAINS EHR e");
    let stored = ask(gateway, put(name, version, &parameterised(fixture, &from))?).await?;
    stored.expect(StatusCode::OK, "CP-40: a directed definition is storable")?;
    let ran = ask(gateway, invoke(fixture, name)?).await?;
    ran.expect(
        StatusCode::OK,
        "CP-40: the directed definition runs by name",
    )?;
    directed_at(
        &checked(gateway, &ran)?,
        fixture,
        &[endpoint],
        "CP-40: a directed definition stays federated-executable",
    )
}
