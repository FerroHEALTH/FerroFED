// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-83 audit record of the PIXm resolver (PIXm §2:3.83.5.1.1): each
//! resolution reaches the repository naming the patient, and no log line,
//! metric or node request names it; a repository that is down holds the
//! records and the queries go on; and a spool that cannot take a record
//! fails the query closed before any member is asked.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;
use std::time::Duration;

use axum::Router;
use ferrofed_server::metrics::Metrics;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{audit_tables, await_feed_state, spool_key, spooled, transactions};
use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, PATIENT, body, gateway, node_answering, patient_query, post,
    received, registry,
};
use crate::metrics::{count, parse};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";

/// A PIX Manager that resolves the patient at node A and node B.
async fn manager() -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$ihe-pix"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{EHR_A}"}}}},{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_B}","value":"{EHR_B}"}}}}]}}"#
            )
            .into_bytes(),
            "application/fhir+json",
        ))
        .mount(&server)
        .await;
    server
}

/// A development gateway over node A at `a` and node B at `b`, resolving at
/// the PIX Manager at `pix` and recording to `repository` with the
/// `[audit.repository]` keys `extra`.
fn audited_gateway(
    dir: &Path,
    [a, b, pix]: [&str; 3],
    repository: &FeedRepository,
    extra: &str,
) -> Result<Router, Box<dyn Error>> {
    gateway(
        dir,
        &registry(a, b, ""),
        "profile = \"development\"",
        &format!(
            "[[pixm.manager]]\nurl = \"{pix}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n{}",
            audit_tables(repository, extra)
        ),
    )
}

#[tokio::test]
async fn each_resolution_reaches_the_repository_and_no_log_metric_or_node_names_the_patient()
-> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = audited_gateway(
        dir.path(),
        [&a.uri(), &b.uri(), &pix.uri()],
        &repository,
        "",
    )?;
    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    let records = repository.wait_for(1, Duration::from_secs(5)).await;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, records.len(), "one record per ITI-83 exchange");
    assert_eq!(vec!["ITI-83"], transactions(&records[0])?);
    assert!(
        records[0].contains(PATIENT) && records[0].contains(NAMESPACE),
        "the patient entity names the source identifier toward the repository"
    );
    assert_eq!(Some("up"), await_feed_state(&app, "up").await?.as_deref());
    let exposition = Metrics::default().render()?;
    let samples = parse(&exposition)?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, "ferrofed_audit_delivered_total", &[])
    );
    assert!(!exposition.contains(PATIENT), "no identifier in a metric");
    let log = logs.text();
    assert!(!log.contains(PATIENT), "no identifier in the log: {log}");
    for node in [&a, &b] {
        for request in received(node).await? {
            assert!(!request.contains(PATIENT), "N33: {request}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_repository_that_is_down_holds_the_records_and_the_queries_go_on() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let (key, spool) = spool_key(dir.path());
    let app = audited_gateway(
        dir.path(),
        [&a.uri(), &b.uri(), &pix.uri()],
        &repository,
        &key,
    )?;
    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "an outage of the repository is stored, not refused (ITI TF-2 §3.20.4.1.1): {text}"
    );
    assert_eq!(
        Some("degraded"),
        await_feed_state(&app, "degraded").await?.as_deref()
    );
    assert_eq!(1, spooled(&spool)?, "the record is on disk");
    repository.set_up(true);
    let records = repository.wait_for(1, Duration::from_secs(10)).await;
    assert_eq!(
        1,
        records.len(),
        "the spool drains once the repository is back"
    );
    assert_eq!(Some("up"), await_feed_state(&app, "up").await?.as_deref());
    assert_eq!(0, spooled(&spool)?);
    Ok(())
}

#[tokio::test]
async fn a_record_the_spool_cannot_take_fails_the_query_closed_and_asks_no_member() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let app = audited_gateway(
        dir.path(),
        [&a.uri(), &b.uri(), &pix.uri()],
        &repository,
        "spool_max_events = 1",
    )?;
    let (status, _) = call(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status);
    let asked = received(&a).await?.len();
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "the resolution failed closed: {text}"
    );
    let answer: Answer = serde_json::from_str(&text)?;
    for endpoint in &answer.meta.federation.endpoints {
        assert_eq!("not-resolved", endpoint.status, "{text}");
        let error = serde_json::to_string(&endpoint.error)?;
        assert!(!error.contains(PATIENT), "{error}");
    }
    assert_eq!(
        asked,
        received(&a).await?.len(),
        "no member is asked without the audit record"
    );
    Ok(())
}

#[tokio::test]
async fn the_log_destination_records_each_resolution_without_the_patient() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &format!(
            "[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n\n[audit]\ndestination = \"log\"\n",
            pix.uri()
        ),
    )?;
    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    let log = logs.text();
    let event = log
        .lines()
        .find(|line| line.contains("BALP audit record"))
        .ok_or("the record is logged at the audit target")?;
    assert!(event.contains("ferrofed::audit"), "{event}");
    assert!(event.contains("ITI-83"), "{event}");
    assert!(!log.contains(PATIENT), "no identifier in the log: {log}");
    Ok(())
}
