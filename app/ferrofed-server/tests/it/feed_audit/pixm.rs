// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-83 audit record of the PIXm resolver (PIXm §2:3.83.5.1.1): each
//! resolution reaches the repository naming the patient, and no log line,
//! metric or node request names it; a repository that is down holds the
//! records and the queries go on; a repository that never answers holds no
//! query, because only the spool write sits on the request's path (ITI TF-2
//! §3.20.4.1.1); and a spool that cannot take a record fails the query closed
//! before any member is asked.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;

use axum::Router;
use ferrofed_server::metrics::Metrics;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    SETTLE, audit_tables, await_feed_state, feed_state, names_the_default_caller, spool_key,
    spooled, transactions,
};
use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, PATIENT, body, gateway, gateway_within, node_answering,
    patient_query, post, received, registry, statuses,
};
use crate::metrics::{count, parse};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";

/// The per-node and overall budget of [`audited_gateway`], in milliseconds.
///
/// The nodes answer at once, so the budget only has to outlast a stall of a
/// loaded host, as far as the ten-second request timeout of
/// `settings_with_room` less the one-second combining margin allows.
const BUDGET_MS: u64 = 8_000;

/// The feed's per-request timeout toward a repository that never answers:
/// an hour, past the life of the test, so a delivery on the request's path
/// would hold the query past every budget.
const NEVER_MS: u64 = 3_600_000;

/// Both nodes answered.
const BOTH_ACTIVE: [(&str, &str); 2] = [("node-a-pub", "active"), ("node-b-pub", "active")];

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
/// `[audit.repository]` keys `extra`, within [`BUDGET_MS`].
fn audited_gateway(
    dir: &Path,
    [a, b, pix]: [&str; 3],
    repository: &FeedRepository,
    extra: &str,
) -> Result<Router, Box<dyn Error>> {
    gateway_within(
        dir,
        &registry(a, b, ""),
        "profile = \"development\"",
        &format!(
            "[[pixm.manager]]\nurl = \"{pix}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n{}",
            audit_tables(repository, extra)
        ),
        (BUDGET_MS, BUDGET_MS),
    )
}

/// Posts the patient query to `app`, failing unless the answer is a `200`
/// in which both nodes answered.
async fn answered_by_both(app: &Router, why: &str) -> TestResult {
    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{why}: {text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(BOTH_ACTIVE.to_vec(), statuses(&answer), "{why}: {text}");
    Ok(())
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
    let records = repository.wait_for(1, SETTLE).await;
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
    answered_by_both(
        &app,
        "an outage of the repository is stored, not refused (ITI TF-2 §3.20.4.1.1)",
    )
    .await?;
    assert_eq!(
        Some("degraded"),
        await_feed_state(&app, "degraded").await?.as_deref()
    );
    assert_eq!(1, spooled(&spool)?, "the record is on disk");
    repository.set_up(true);
    let records = repository.wait_for(1, SETTLE).await;
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
async fn a_repository_that_never_answers_holds_no_query_and_the_records_are_spooled() -> TestResult
{
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let repository = FeedRepository::start().await;
    repository.go_silent();
    let dir = tempfile::tempdir()?;
    let (key, spool) = spool_key(dir.path());
    let app = audited_gateway(
        dir.path(),
        [&a.uri(), &b.uri(), &pix.uri()],
        &repository,
        &format!("{key}\ntimeout_ms = {NEVER_MS}"),
    )?;
    answered_by_both(
        &app,
        "the first record is stored, not delivered, on the path",
    )
    .await?;
    assert_eq!(
        1,
        repository.wait_asked(1, SETTLE).await,
        "the forwarder's delivery is in flight, and is never answered"
    );
    answered_by_both(&app, "a delivery in flight holds no query").await?;
    assert_eq!(2, spooled(&spool)?, "both records are on disk");
    assert_eq!(
        1,
        repository.asked(),
        "the forwarder still waits on the first"
    );
    assert!(repository.records().is_empty(), "nothing was delivered");
    assert_eq!(Some("degraded"), feed_state(&app).await?.as_deref());
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
    assert!(
        event.contains(r#""on_behalf":"caller""#),
        "the event says whose behalf: {event}"
    );
    let claims = crate::support::claims();
    for value in [&claims.sub, &claims.client_id] {
        assert!(!log.contains(value.as_str()), "no caller in the log: {log}");
    }
    Ok(())
}

#[tokio::test]
async fn each_resolution_names_the_verified_caller_as_its_user_agent() -> TestResult {
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
    answered_by_both(&app, "a patient query of the default caller").await?;
    let records = repository.wait_for(1, SETTLE).await;
    assert_eq!(1, records.len(), "one record per ITI-83 exchange");
    // NOTE: PIXm §2:3.83.5.2.1 augments the record "following IHE-BALP" with the agent
    // details of the OAuth token, which BALP 1.1.4 §3:5.7.5.4 maps.
    names_the_default_caller(&records[0])
}

#[tokio::test]
async fn a_header_naming_another_user_never_reaches_the_record() -> TestResult {
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
    let mut request = post(body(&patient_query())?)?;
    for name in ["x-forwarded-user", "x-user-id", "x-remote-user", "from"] {
        request
            .headers_mut()
            .insert(name, http::HeaderValue::from_static("Qz7-forged-user"));
    }
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = repository.wait_for(1, SETTLE).await;
    assert_eq!(1, records.len(), "one record per ITI-83 exchange");
    // NOTE: PIXm §2:3.83.5.2.1 takes the agent details from the OAuth token; a header the
    // gate did not verify names no one.
    assert!(!records[0].contains("Qz7-forged-user"), "{}", records[0]);
    names_the_default_caller(&records[0])
}

#[tokio::test]
async fn the_caller_reaches_the_repository_and_no_log_line_or_metric() -> TestResult {
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
        "trace",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    let records = repository.wait_for(1, SETTLE).await;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    let claims = crate::support::claims();
    assert!(
        records.iter().any(|record| record.contains(&claims.sub)),
        "the caller reaches the repository"
    );
    let log = logs.text();
    let exposition = Metrics::default().render()?;
    for value in [&claims.sub, &claims.client_id] {
        assert!(!log.contains(value.as_str()), "no caller in the log: {log}");
        assert!(
            !exposition.contains(value.as_str()),
            "no caller in a metric"
        );
    }
    Ok(())
}
