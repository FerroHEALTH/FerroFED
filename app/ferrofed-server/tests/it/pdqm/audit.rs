// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit records of the demographics step (PDQm §2:3.78.5.1 and
//! §2:3.119.5.1.1): each ITI-78 or ITI-119 exchange reaches the Audit Record
//! Repository through `[audit]` beside the ITI-83 record of the resolution
//! it feeds, the log destination records it without the patient, and an
//! exchange whose record the spool cannot take fails closed, under every
//! localization failure policy.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::time::Duration;

use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::StatusCode;

use super::{
    ASK_ALL, CLIENT_ID, MASTER_ID, demographics_state, gateway, local_query, manager, pdqm, pixm,
    supplier,
};
use crate::facade::{
    Answer, PATIENT, body, node_answering, patient_query, post, received, registry, statuses,
};
use crate::feed_audit::{audit_tables, names_the_default_caller, transactions};
use crate::metrics::{count, parse};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The `[pixm]` and `[pdqm]` tables, with `[audit]` to `repository` with the
/// `[audit.repository]` keys `extra`.
fn audited(pix: &Server, pdq: &str, repository: &FeedRepository, extra: &str) -> String {
    format!(
        "{}\n{}\n{}",
        pixm(&pix.uri()),
        pdqm(pdq, "iti-119"),
        audit_tables(repository, extra)
    )
}

#[tokio::test]
async fn each_exchange_reaches_the_repository_beside_the_resolution_it_feeds() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let (app, _state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &audited(&pix, &pdq.base_url(), &repository, ""),
    )?;
    let (status, text) = call(app, post(body(&local_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = repository.wait_for(2, Duration::from_secs(5)).await;
    let mut seen = Vec::new();
    for record in &records {
        seen.extend(transactions(record)?);
    }
    seen.sort();
    assert_eq!(vec!["ITI-119", "ITI-83"], seen, "one record per exchange");
    let matched = records
        .iter()
        .find(|record| record.contains("ITI-119"))
        .ok_or("the ITI-119 record")?;
    assert!(
        matched.contains("IHE.PDQm.Match.Audit.Consumer") && matched.contains(CLIENT_ID),
        "the Match Consumer record names the patient the input identifies: {matched}"
    );
    Ok(())
}

#[tokio::test]
async fn each_search_and_match_names_the_verified_caller_as_its_user_agent() -> TestResult {
    for transaction in ["iti-78", "iti-119"] {
        let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
        let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
        let pix = manager().await;
        let pdq = supplier().await?;
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let tables = format!(
            "{}\n{}\n{}",
            pixm(&pix.uri()),
            pdqm(&pdq.base_url(), transaction),
            audit_tables(&repository, "")
        );
        let (app, _state) = gateway(
            dir.path(),
            &registry(&a.uri(), &b.uri(), ""),
            ASK_ALL,
            &tables,
        )?;
        let (status, text) = call(app, post(body(&local_query())?)?).await?;
        assert_eq!(StatusCode::OK, status, "{transaction}: {text}");
        let records = repository.wait_for(2, Duration::from_secs(5)).await;
        let code = transaction.to_uppercase();
        let step = records
            .iter()
            .find(|record| transactions(record).is_ok_and(|seen| seen.contains(&code)))
            .ok_or_else(|| format!("the {code} record"))?;
        // NOTE: PDQm §2:3.78.5.1 and §2:3.119.5.1.1 build on BALP Query, whose agent:user
        // names the user BALP 1.1.4 §3:5.7.5.4 maps from the OAuth token.
        names_the_default_caller(step).map_err(|error| format!("{code}: {error}"))?;
    }
    Ok(())
}

#[tokio::test]
async fn the_log_destination_records_the_exchange_without_the_patient() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    let dir = tempfile::tempdir()?;
    let (app, _state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &super::tables(&pix.uri(), &pdq.base_url(), "iti-78"),
    )?;
    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app, post(body(&local_query())?)?).await?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    let log = logs.text();
    assert!(
        log.lines()
            .any(|line| line.contains("BALP audit record") && line.contains("ITI-78")),
        "the ITI-78 record is logged at the audit target: {log}"
    );
    for value in [CLIENT_ID, MASTER_ID] {
        assert!(!log.contains(value), "no identifier in the log: {log}");
    }
    Ok(())
}

#[tokio::test]
async fn an_exchange_the_spool_cannot_take_fails_closed_and_asks_no_member() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &audited(&pix, &pdq.base_url(), &repository, "spool_max_events = 1"),
    )?;
    // The master identity resolves without the step, and its ITI-83 record
    // fills the one place the spool has.
    let query = patient_query().replace(PATIENT, MASTER_ID);
    let (status, text) = call(app.clone(), post(body(&query)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let asked = received(&a).await?.len();
    let (status, text) = call(app.clone(), post(body(&local_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "an exchange that could not be audited fails closed (§14.1): {text}"
    );
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "not-localized"),
            ("node-b-pub", "not-localized")
        ],
        statuses(&answer)
    );
    assert!(text.contains("could not be audited"), "{text}");
    assert!(!text.contains(MASTER_ID), "{text}");
    for endpoint in &answer.meta.federation.endpoints {
        let error = serde_json::to_string(&endpoint.error)?;
        assert!(!error.contains(CLIENT_ID), "{error}");
    }
    assert_eq!(asked, received(&a).await?.len(), "no member is asked");
    assert_eq!(Some("failing"), demographics_state(&app).await?.as_deref());
    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(
            &samples,
            "ferrofed_demographics_requests_total",
            &[("outcome", "audit-failed")]
        )
    );
    Ok(())
}
