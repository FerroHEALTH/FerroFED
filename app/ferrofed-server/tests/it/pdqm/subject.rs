// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read of an EHR by subject through the demographics step
//! (`GET {base}/v1/ehr?subject_id=…&subject_namespace=…`): the master
//! identity resolves the holder, which is asked by its own `ehr_id` alone;
//! no match is the operation's `404`; several matches and an outage are a
//! `424` (Annex A §A.2, §5.2, §5.4.1, §11.2, §14.1, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdq::PdqSupplier;
use ferrofed_testkit::unreachable;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use super::{ASK_ALL, CLIENT_ID, DOMAIN_A, DOMAIN_B, LOCAL, MASTER, MASTER_ID, gateway, pdqm};
use crate::facade::{EHR_A, registry, wire};
use crate::support::{call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

/// A PIX Manager that resolves the master identifier at node A alone.
async fn manager_at_a() -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$ihe-pix"))
        .and(query_param("sourceIdentifier", format!("{MASTER}|{MASTER_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{EHR_A}"}}}}]}}"#
            )
            .into_bytes(),
            "application/fhir+json",
        ))
        .mount(&server)
        .await;
    server
}

/// A node answering `GET /v1/ehr/{ehr_id}` with its `EHR`.
async fn holder(ehr_id: &str) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{ehr_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"system_id":{{"value":"cdr-a.example.org"}},"ehr_id":{{"value":"{ehr_id}"}},"time_created":{{"value":"2026-01-01T00:00:00Z"}}}}"#
            )
            .into_bytes(),
            "application/json",
        ))
        .mount(&server)
        .await;
    server
}

/// The read of the EHR of the client's subject.
fn by_subject() -> Result<Request<Body>, http::Error> {
    Request::get(format!(
        "/v1/ehr?subject_id={CLIENT_ID}&subject_namespace={LOCAL}"
    ))
    .header(header::ACCEPT, "application/json")
    .body(Body::empty())
}

/// The status and the body of the read through a gateway over node A at `a`
/// and node B at `b`, the PIX Manager at `pix` and the Supplier at `pdq`.
async fn read(
    a: &Server,
    b: &Server,
    pix: &Server,
    pdq: &str,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let tables = format!(
        "[audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n\n{}",
        pix.uri(),
        pdqm(pdq, "iti-78")
    );
    let (app, _state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables,
    )?;
    call(app, by_subject()?).await
}

#[tokio::test]
async fn the_master_identity_routes_the_read_to_the_holder_by_its_ehr_id_alone() -> TestResult {
    let a = holder(EHR_A).await;
    let b = holder("1111bbbb-1111-4111-8111-111111111111").await;
    let pix = manager_at_a().await;
    let pdq = super::supplier().await?;
    let (status, text) = read(&a, &b, &pix, &pdq.base_url()).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let wire = wire(&a).await?;
    assert!(
        wire.contains(&format!("/v1/ehr/{EHR_A}")),
        "by its own ehr_id"
    );
    for value in [CLIENT_ID, MASTER_ID] {
        assert!(!wire.contains(value), "N33: {value} reaches no node");
    }
    assert!(!text.contains(MASTER_ID), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_subject_the_supplier_knows_no_patient_for_is_the_operations_404() -> TestResult {
    let a = holder(EHR_A).await;
    let b = holder("1111bbbb-1111-4111-8111-111111111111").await;
    let pix = manager_at_a().await;
    let pdq = PdqSupplier::start().await?;
    pdq.add(&[(LOCAL, "SENTINEL-OTHER-5k2p"), (MASTER, MASTER_ID)], true)?;
    let (status, text) = read(&a, &b, &pix, &pdq.base_url()).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!("no-destination", error_body(&text)?.code);
    Ok(())
}

#[tokio::test]
async fn several_matched_patients_are_a_424_and_no_node_is_asked() -> TestResult {
    let a = holder(EHR_A).await;
    let b = holder("1111bbbb-1111-4111-8111-111111111111").await;
    let pix = manager_at_a().await;
    let pdq = super::supplier().await?;
    pdq.add(
        &[(LOCAL, CLIENT_ID), (MASTER, "SENTINEL-MASTER-2b8m")],
        true,
    )?;
    let (status, text) = read(&a, &b, &pix, &pdq.base_url()).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    let error = error_body(&text)?;
    assert_eq!("resolution-unavailable", error.code);
    assert!(error.message.contains("more than one patient"), "{text}");
    assert!(wire(&a).await?.is_empty() && wire(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_supplier_outage_fails_the_read_closed_as_a_localizer_outage() -> TestResult {
    let a = holder(EHR_A).await;
    let b = holder("1111bbbb-1111-4111-8111-111111111111").await;
    let pix = manager_at_a().await;
    let (status, text) = read(&a, &b, &pix, &format!("{}/fhir/", unreachable::BASE)).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert_eq!("localization-unavailable", error_body(&text)?.code);
    assert!(wire(&a).await?.is_empty() && wire(&b).await?.is_empty());
    Ok(())
}
