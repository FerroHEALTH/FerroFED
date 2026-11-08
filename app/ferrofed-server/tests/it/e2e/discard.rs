// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's discard of a dispensation, held to the
//! dispenser FerroFED's own store keeps for 24 hours, on a real PostgreSQL
//! 18 serving TLS (MyHealth@EU Requirements Catalogue 10.0.0 07.04; `NCPeH`
//! Components Specification 10.0.0 §3.1.6.4.2, "issued by the same HCPO").
//!
//! A contact point's dispensation is recorded with the identity assertion's
//! `organization-id` and the facility it names on `performer.actor` (HL7
//! Europe MPD 1.0.0). Its discard from that organisation and facility
//! reaches the member; one from another organisation or another facility,
//! one after its 24 hours, one of a dispensation that named no facility and
//! one of a dispensation no contact point wrote are refused
//! `discard-refused` with `ERROR_ED_DISCARD_FAILED`, and the member is never
//! asked.

use std::path::Path;

use ferrofed_testkit::containers::database::PostgresTls;
use ferrofed_testkit::containers::{self, database};
use http::StatusCode;

use super::TestResult;
use super::store::{column, executed, store_toml};
use crate::fhir::prescriptions::dispenser::{
    Asserted, Dispensing, FACILITY, assert_discard_refused,
};

/// Another provider's `organization-id`.
const OTHER_ORGANIZATION: &str = "urn:oid:2.999.9.67";

/// Another facility's identifier.
const OTHER_FACILITY: (&str, &str) = ("urn:oid:2.999.8", "SYNTHETIC-PHARMACY-2");

/// The scene over the store database `name` on `server`.
async fn scene(server: &PostgresTls, name: &str) -> Result<Dispensing, Box<dyn std::error::Error>> {
    Dispensing::start(|dir: &Path| store_toml(server, name, dir)).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_discard_from_the_dispenser_within_24_hours_reaches_the_member() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("discard_same", &[]).await?;
    let scene = scene(&server, "discard_same").await?;
    let (status, text) = scene.dispense(Asserted::PERMITTED, Some(FACILITY)).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let url = server.plain_url("discard_same");
    assert_eq!(
        vec![String::from(
            "urn:oid:2.999.9.66 urn:oid:2.999.8 SYNTHETIC-PHARMACY-1 within"
        )],
        column(
            &url,
            "SELECT organization || ' ' || facility_system || ' ' || facility_value || ' ' || \
             CASE WHEN expires_at BETWEEN now() + interval '23 hours 59 minutes' \
             AND now() + interval '24 hours' THEN 'within' ELSE 'outside' END \
             FROM ferrofed.contact_point_dispensation"
        )
        .await?,
        "RC 07.04: the dispenser is kept for 24 hours"
    );
    let (status, text) = scene.discard(Asserted::PERMITTED, FACILITY).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, scene.updated().await?, "the discard reaches the member");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_discard_from_another_organisation_or_facility_is_refused() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("discard_other", &[]).await?;
    let scene = scene(&server, "discard_other").await?;
    let (status, text) = scene.dispense(Asserted::PERMITTED, Some(FACILITY)).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let other = Asserted {
        organization: OTHER_ORGANIZATION,
        ..Asserted::PERMITTED
    };
    let answer = scene.discard(other, FACILITY).await?;
    assert_discard_refused("another organisation", &answer)?;
    let answer = scene.discard(Asserted::PERMITTED, OTHER_FACILITY).await?;
    assert_discard_refused("another facility", &answer)?;
    assert_eq!(0, scene.updated().await?, "the member is never asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_discard_after_24_hours_is_refused() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("discard_late", &[]).await?;
    let scene = scene(&server, "discard_late").await?;
    let (status, text) = scene.dispense(Asserted::PERMITTED, Some(FACILITY)).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    executed(
        &server.plain_url("discard_late"),
        "UPDATE ferrofed.contact_point_dispensation SET expires_at = now() - interval '1 second'",
    )
    .await?;
    let answer = scene.discard(Asserted::PERMITTED, FACILITY).await?;
    assert_discard_refused("after 24 hours", &answer)?;
    assert_eq!(0, scene.updated().await?, "the member is never asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_discard_of_a_dispensation_with_no_recorded_dispenser_is_refused() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("discard_unrecorded", &[]).await?;
    let scene = scene(&server, "discard_unrecorded").await?;
    let answer = scene.discard(Asserted::PERMITTED, FACILITY).await?;
    assert_discard_refused("no contact point wrote it", &answer)?;
    let (status, text) = scene.dispense(Asserted::PERMITTED, None).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(
        Vec::<String>::new(),
        column(
            &server.plain_url("discard_unrecorded"),
            "SELECT organization FROM ferrofed.contact_point_dispensation"
        )
        .await?,
        "a dispensation that names no facility is recorded nowhere"
    );
    let answer = scene.discard(Asserted::PERMITTED, FACILITY).await?;
    assert_discard_refused("no facility was named", &answer)?;
    assert_eq!(0, scene.updated().await?, "the member is never asked");
    Ok(())
}
