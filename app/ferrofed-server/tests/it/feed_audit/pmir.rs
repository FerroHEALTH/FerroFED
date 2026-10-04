// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-93 and ITI-94 audit records of the PMIR identity feed (PMIR
//! §2:3.93.5.1, §2:3.94.5.1): each exchange with the Registry and each
//! message received is recorded, the message naming its patients toward the
//! repository only; a message whose record the spool cannot take is neither
//! applied nor answered as processed, so the Registry sends it again.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::time::Duration;

use axum::body::Body;
use ferrofed_server::metrics::Metrics;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::pmir::{PatientIdentityRegistry, merge_message};
use http::{Request, StatusCode, header};

use super::{audit_tables, transactions};
use crate::pmir::{DOMAIN_A, EHR_A, EHR_A2, PATH, TOKEN, gateway, text};
use crate::support::send_as_is;

type TestResult = Result<(), Box<dyn Error>>;

/// The Registry's id of the merged Patient, which only the repository may
/// learn.
const SUBSUMED: &str = "qz7-subsumed-4713";

/// The ITI-93 request carrying `body` with the feed token.
fn message(body: String) -> Result<Request<Body>, http::Error> {
    Request::post(PATH)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::from(body))
}

#[tokio::test]
async fn each_subscription_exchange_is_recorded() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let gateway = gateway(&text(
        dir.path(),
        &registry.base_url(),
        "http://127.0.0.1:9/pmir/feed",
        &audit_tables(&repository, ""),
    )?)?;
    let feed = gateway.state.identity_feed().ok_or("[pmir] is set")?;
    let _created = feed.check().await;
    let _read = feed.check().await;
    feed.unsubscribe().await;
    let records = repository.wait_for(4, Duration::from_secs(5)).await;
    let mut seen = Vec::new();
    for record in &records {
        seen.extend(transactions(record)?);
    }
    assert!(
        seen.iter().filter(|code| *code == "ITI-94").count() >= 3,
        "the create, the read and the delete are each ITI-94: {seen:?}"
    );
    assert_eq!(
        4,
        records.len(),
        "and the search before the create is recorded too"
    );
    Ok(())
}

#[tokio::test]
async fn a_received_message_is_recorded_naming_its_patient_toward_the_repository_only() -> TestResult
{
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let gateway = gateway(&text(
        dir.path(),
        "http://127.0.0.1:9/fhir/",
        "http://127.0.0.1:9/pmir/feed",
        &audit_tables(&repository, ""),
    )?)?;
    gateway.bind("caller-1", &[EHR_A, EHR_A2])?;
    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let body = merge_message(SUBSUMED, &[(DOMAIN_A, EHR_A)], "patient-new")?;
    let response = send_as_is(gateway.app.clone(), message(body)?).await?;
    let records = repository.wait_for(1, Duration::from_secs(5)).await;
    drop(guard);
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(1, gateway.bound()?, "the merge was applied");
    let [record] = <[String; 1]>::try_from(records).map_err(|_all| "one record")?;
    assert_eq!(vec!["ITI-93"], transactions(&record)?);
    assert!(
        record.contains(&format!("Patient/{SUBSUMED}")),
        "the patient entity names the merged Patient: {record}"
    );
    let log = logs.text();
    assert!(!log.contains(SUBSUMED), "no patient in the log: {log}");
    let exposition = Metrics::default().render()?;
    assert!(!exposition.contains(SUBSUMED), "no patient in a metric");
    Ok(())
}

#[tokio::test]
async fn a_message_whose_record_the_spool_cannot_take_is_not_applied() -> TestResult {
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let gateway = gateway(&text(
        dir.path(),
        "http://127.0.0.1:9/fhir/",
        "http://127.0.0.1:9/pmir/feed",
        &audit_tables(&repository, "spool_max_events = 1"),
    )?)?;
    gateway.bind("caller-1", &[EHR_A, EHR_A2])?;
    let first = merge_message("qz7-first", &[("urn:oid:2.999.1.999", "SYNTHETIC-1")], "x")?;
    let response = send_as_is(gateway.app.clone(), message(first)?).await?;
    assert_eq!(
        StatusCode::OK,
        response.status(),
        "the first record is spooled"
    );
    gateway.bind("caller-2", &[EHR_A])?;
    let bound = gateway.bound()?;
    let second = merge_message(SUBSUMED, &[(DOMAIN_A, EHR_A)], "patient-new")?;
    let response = send_as_is(gateway.app.clone(), message(second)?).await?;
    assert_eq!(
        StatusCode::SERVICE_UNAVAILABLE,
        response.status(),
        "the Registry is told to send it again"
    );
    assert_eq!(bound, gateway.bound()?, "nothing was applied");
    Ok(())
}

#[tokio::test]
async fn a_create_whose_record_is_refused_is_adopted_by_the_next_check_and_never_made_twice()
-> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let gateway = gateway(&text(
        dir.path(),
        &registry.base_url(),
        "http://127.0.0.1:9/pmir/feed",
        &audit_tables(&repository, "spool_max_events = 1"),
    )?)?;
    let feed = gateway.state.identity_feed().ok_or("[pmir] is set")?;
    assert_eq!(
        Some(ferrofed_server::pmir::subscription::RegistryFault::AuditFailed),
        {
            let _failed = feed.check().await;
            feed.fault()
        },
        "the search's record filled the spool, so the create's answer is set aside"
    );
    assert_eq!(1, registry.creates(), "the Registry made the subscription");
    repository.set_up(true);
    let drained = repository.wait_for(1, Duration::from_secs(5)).await;
    assert_eq!(
        1,
        drained.len(),
        "the spool drains once the repository is back"
    );
    assert_eq!(
        ferrofed_server::health::dependencies::Observed::Up,
        feed.check().await,
        "the next check's search adopts the subscription the Registry holds"
    );
    assert_eq!(1, registry.creates(), "no second subscription is created");
    assert_eq!(1, registry.subscriptions().len());
    Ok(())
}
