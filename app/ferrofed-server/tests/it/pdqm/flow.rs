// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated query through the PDQm demographics step: the master
//! identity resolved at the PIX Manager and each node asked by its own
//! `ehr_id`, no match, several matches, and a Supplier outage under each
//! localization failure policy (Annex A §A.2, §5.2, §11.1, §14.1, N3, N4,
//! N6, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use ferrofed_server::federation::Federation;
use ferrofed_server::federation::registry::read_registry;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdq::PdqSupplier;
use ferrofed_testkit::unreachable;
use http::StatusCode;

use super::{
    ASK_ALL, CLIENT_ID, LOCAL, MASTER, MASTER_ID, demographics_state, gateway, local_query,
    manager, pdqm, pixm, settings, supplier, tables,
};
use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, body, node_answering, patient_query, post, received, registry,
    schema, settings_with_room, statuses, wire,
};
use crate::metrics::{count, parse};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

const DEMOGRAPHICS_CALLS: &str = "ferrofed_demographics_requests_total";

/// Every request `server` received, URL and body, as text.
async fn everything(server: &Server) -> Result<String, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut text = String::new();
    for request in &requests {
        text.push_str(request.url.as_str());
        text.push(' ');
        text.push_str(&String::from_utf8_lossy(&request.body));
    }
    Ok(text)
}

/// How many requests `server` received.
async fn asked(server: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .len())
}

/// The query through the step asked with `transaction`: the master identity
/// is resolved and each node is asked by its own `ehr_id` alone.
async fn resolves_through(transaction: &str) -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables(&pix.uri(), &pdq.base_url(), transaction),
    )?;

    let (status, text) = call(app.clone(), post(body(&local_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{transaction}: {text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(answer.meta.federation.complete, "{text}");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "{text}"
    );
    assert_eq!(2, answer.rows.len(), "{text}");
    for (node, ehr_id) in [(&a, EHR_A), (&b, EHR_B)] {
        let wire = wire(node).await?;
        assert!(
            wire.contains(ehr_id),
            "each node is asked by its own ehr_id"
        );
        assert!(
            !wire.contains(CLIENT_ID) && !wire.contains(MASTER_ID),
            "N33: neither identifier reaches a node"
        );
    }
    let at_pix = everything(&pix).await?;
    assert!(
        at_pix.contains(MASTER_ID),
        "the master identity is resolved"
    );
    assert!(
        !at_pix.contains(CLIENT_ID),
        "the client's identifier reaches the Supplier only: {at_pix}"
    );
    assert!(
        pdq.bodies().iter().all(|sent| !sent.contains(MASTER_ID)),
        "the Supplier is asked by the client's identifier"
    );
    assert!(!text.contains(MASTER_ID), "the master identity stays in");
    assert_eq!(
        Some("up"),
        demographics_state(&app).await?.as_deref(),
        "the service answered"
    );
    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, DEMOGRAPHICS_CALLS, &[("outcome", "identified")]),
        "one identification is counted"
    );
    Ok(())
}

// conformance: CP-3
#[tokio::test]
async fn an_iti_78_search_finds_the_master_identity_the_pix_manager_resolves() -> TestResult {
    resolves_through("iti-78").await
}

// conformance: CP-3
#[tokio::test]
async fn an_iti_119_match_finds_the_master_identity_the_pix_manager_resolves() -> TestResult {
    resolves_through("iti-119").await
}

#[tokio::test]
async fn an_identifier_in_a_namespace_the_step_does_not_handle_skips_it() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    let dir = tempfile::tempdir()?;
    let (app, _state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables(&pix.uri(), &pdq.base_url(), "iti-78"),
    )?;
    assert_eq!(
        MASTER, NAMESPACE,
        "the shared fixture namespace is the master"
    );
    let query = patient_query().replace(crate::facade::PATIENT, MASTER_ID);
    let (status, text) = call(app.clone(), post(body(&query)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        0,
        pdq.searches() + pdq.matches(),
        "the Supplier is not asked"
    );
    assert_eq!(
        Some("unknown"),
        demographics_state(&app).await?.as_deref(),
        "the configured service was never asked"
    );
    Ok(())
}

// conformance: CP-12 CP-36
#[tokio::test]
async fn an_identifier_the_supplier_knows_no_patient_for_is_not_resolved_anywhere() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = PdqSupplier::start().await?;
    pdq.add(&[(LOCAL, "SENTINEL-OTHER-5k2p"), (MASTER, MASTER_ID)], true)?;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables(&pix.uri(), &pdq.base_url(), "iti-78"),
    )?;

    let (status, text) = call(app.clone(), post(body(&local_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "no match fails nothing (N6): {text}"
    );
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "not-resolved"),
            ("node-b-pub", "not-resolved")
        ],
        statuses(&answer)
    );
    assert!(answer.rows.is_empty(), "{text}");
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "no node is asked");
    assert_eq!(0, asked(&pix).await?, "nothing is resolved");
    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, DEMOGRAPHICS_CALLS, &[("outcome", "no-match")])
    );
    Ok(())
}

#[tokio::test]
async fn several_matched_patients_refuse_the_resolution_and_fail_the_query() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    pdq.add(
        &[(LOCAL, CLIENT_ID), (MASTER, "SENTINEL-MASTER-2b8m")],
        true,
    )?;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables(&pix.uri(), &pdq.base_url(), "iti-78"),
    )?;

    let (status, text) = call(app.clone(), post(body(&local_query())?)?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "the gateway never picks one of several (§2:3.78.4.1.3 Case 1): {text}"
    );
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "not-resolved"),
            ("node-b-pub", "not-resolved")
        ],
        statuses(&answer)
    );
    assert!(text.contains("more than one patient"), "{text}");
    assert!(!text.contains("SENTINEL-MASTER"), "{text}");
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "no node is asked");
    assert_eq!(0, asked(&pix).await?, "nothing is resolved");
    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, DEMOGRAPHICS_CALLS, &[("outcome", "ambiguous")])
    );
    Ok(())
}

#[tokio::test]
async fn a_supplier_outage_fails_closed_with_every_member_not_localized() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables(
            &pix.uri(),
            &format!("{}/fhir/", unreachable::BASE),
            "iti-78",
        ),
    )?;

    let (status, text) = call(app.clone(), post(body(&local_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "§14.1: a fail-closed outage leaves no node in scope: {text}"
    );
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "not-localized"),
            ("node-b-pub", "not-localized")
        ],
        statuses(&answer)
    );
    assert!(
        text.contains("the demographics service could not answer"),
        "§14.1: the outage is the error of each endpoint: {text}"
    );
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "no node is asked");
    assert_eq!(0, asked(&pix).await?, "nothing is resolved");
    assert_eq!(Some("down"), demographics_state(&app).await?.as_deref());
    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, DEMOGRAPHICS_CALLS, &[("outcome", "unavailable")])
    );
    Ok(())
}

#[tokio::test]
async fn a_supplier_outage_under_ask_all_leaves_every_member_not_resolved() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let dir = tempfile::tempdir()?;
    let (app, _state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "node_selection = \"localized\"\n\n[federation.localization]\ntimeout_ms = 1000\non_failure = \"ask-all\"",
        &format!(
            "[audit]\ndestination = \"log\"\n\n{}\n{}",
            pixm(&pix.uri()),
            pdqm(&format!("{}/fhir/", unreachable::BASE), "iti-78")
        ),
    )?;

    let (status, text) = call(app, post(body(&local_query())?)?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "ask-all keeps every member in scope, and none can be resolved: {text}"
    );
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "not-resolved"),
            ("node-b-pub", "not-resolved")
        ],
        statuses(&answer)
    );
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "no node is asked");
    Ok(())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn neither_identifier_reaches_a_log_line_a_metric_or_a_node() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables(&pix.uri(), &pdq.base_url(), "iti-119"),
    )?;
    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app, post(body(&local_query())?)?).await?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    let log = logs.text();
    let exposition = state.metrics().render()?;
    for value in [CLIENT_ID, MASTER_ID] {
        assert!(!log.contains(value), "no identifier in the log: {log}");
        assert!(!exposition.contains(value), "no identifier in a metric");
        for node in [&a, &b] {
            for request in received(node).await? {
                assert!(!request.contains(value), "N33: {request}");
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_reload_that_changes_the_supplier_url_asks_the_new_supplier() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let first = supplier().await?;
    let second = supplier().await?;
    let dir = tempfile::tempdir()?;
    let registry = registry(&a.uri(), &b.uri(), "");
    let boot = settings(
        dir.path(),
        &registry,
        ASK_ALL,
        &tables(&pix.uri(), &first.base_url(), "iti-78"),
    )?;
    let running = Federation::load(&boot)?.ok_or("a registry is configured")?;
    let fresh = settings(
        dir.path(),
        &registry,
        ASK_ALL,
        &tables(&pix.uri(), &second.base_url(), "iti-78"),
    )?;
    let next = running
        .reloaded(&fresh, read_registry(&fresh))?
        .ok_or("a registry is configured")?;
    let app = ferrofed_server::router(
        Arc::new(AppState::with_federation(next)),
        &settings_with_room(),
    );
    let (status, text) = call(app, post(body(&local_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        1,
        second.searches(),
        "the reloaded step asks the new Supplier"
    );
    assert_eq!(0, first.searches(), "the old Supplier is no longer asked");
    Ok(())
}

#[tokio::test]
async fn options_declares_the_steps_budget_beside_the_n38_timeouts() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    let dir = tempfile::tempdir()?;
    let (app, _state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables(&pix.uri(), &pdq.base_url(), "iti-78"),
    )?;
    let request = http::Request::options("/").body(axum::body::Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate_options(&text)?;
    assert!(
        text.contains(r#""demographics_ms":1000"#),
        "§11.5: the budget is declared with the others: {text}"
    );
    Ok(())
}
