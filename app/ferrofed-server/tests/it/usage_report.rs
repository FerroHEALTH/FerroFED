// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The installation report to `FerroPULSE`: the reports the gateway builds,
//! each validated against the vendored envelope v2 schema, the window the
//! request log and the node requests fill, the sends to a mock collector,
//! and the configuration that switches it.
//!
//! The contract is `FerroPULSE` `docs/specs/ferropulse/docs/report-v2.md` at the commit
//! `docs/VERSIONS.md` pins (`docs/specs/ferropulse/`); no other
//! specification governs the report.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "a test reads the report as the collector's schema reads it, an untyped JSON document"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::state::AppState;
use ferrofed_server::usage_report::status::{Facts, ProfileToken, Service};
use ferrofed_server::usage_report::{Fed, identity};
use ferrohealth_usage_report::reporter::{Reporter, StartOutcome, prepare};
use ferrohealth_usage_report::section::{Deployment, Endpoint, UsageReportSection};
use http::{Request, StatusCode};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::facade::{
    EHR_A, EHR_B, PATIENT, PER_NODE_TIMEOUT_MS, body, crossref, gateway_within_state,
    node_answering, node_failing, patient_query, post, registry,
};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The vendored contract, `scripts/vendor/ferropulse.sh`.
fn contract() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/ferropulse/schemas")
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

/// The report envelope v2 validator.
fn validator() -> Result<jsonschema::Validator, Box<dyn Error>> {
    let schema = read_json(&contract().join("report-envelope.v2.json"))?;
    Ok(jsonschema::validator_for(&schema)?)
}

/// The examples of one validity, by file name.
fn examples(validity: &str) -> Result<Vec<(String, Value)>, Box<dyn Error>> {
    let dir = contract()
        .join("examples/report-envelope.v2")
        .join(validity);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .ok_or("a name")?
            .to_string_lossy()
            .into_owned();
        out.push((name, read_json(&path)?));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// `report` with the members that depend on the host or the build replaced
/// by `*`, after checking it against the schema.
fn stable(mut report: Value) -> Result<Value, Box<dyn Error>> {
    let errors: Vec<String> = validator()?
        .iter_errors(&report)
        .map(|error| error.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}\n{report:#}");
    let object = report.as_object_mut().ok_or("an object")?;
    let id = object["instance_id"].as_str().ok_or("a string")?;
    assert_eq!(id.len(), 36, "hyphenated: {id}");
    for member in [
        "instance_id",
        "git_sha",
        "uptime_s",
        "cpu_bucket",
        "memory_bucket",
    ] {
        object.insert(member.to_owned(), json!("*"));
    }
    Ok(report)
}

/// A report configuration over `state_dir` posting to `endpoint`.
fn config(state_dir: &Path, endpoint: &str) -> Result<UsageReportSection, Box<dyn Error>> {
    Ok(UsageReportSection {
        enabled: true,
        endpoint: Endpoint::new(endpoint)?,
        state_dir: Some(state_dir.to_path_buf()),
        deployment: Deployment::Compose,
    })
}

/// The facts of the development gateway [`gateway`] starts.
fn facts() -> Facts {
    Facts {
        profile: ProfileToken::Development,
        services: vec![Service::Pixm],
        stored_queries: false,
        fhir: false,
    }
}

/// A reporter of `state` under `config`.
fn reporter_over(
    config: &UsageReportSection,
    state: Arc<AppState>,
) -> Result<Reporter<Fed>, Box<dyn Error>> {
    let prepared = prepare(config, None).map_err(|off| format!("{off:?}"))?;
    let window = Arc::clone(state.metrics().usage());
    Ok(Reporter::new(
        config,
        identity(),
        prepared,
        Arc::new(Fed::new(facts(), state)),
        window,
        Instant::now(),
    )?)
}

/// The development gateway over node A at `a` and node B at `b`, resolving
/// the patient at both, with the state it serves from.
fn gateway(
    dir: &Path,
    (a, b): (&str, &str),
) -> Result<(axum::Router, Arc<AppState>), Box<dyn Error>> {
    gateway_within_state(
        dir,
        &registry(a, b, ""),
        "profile = \"development\"",
        &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        (PER_NODE_TIMEOUT_MS, 3_000),
    )
}

#[test]
fn the_validator_admits_every_valid_example_and_refuses_every_invalid_one() -> TestResult {
    let validator = validator()?;
    let valid = examples("valid")?;
    let invalid = examples("invalid")?;
    assert!(
        !valid.is_empty() && !invalid.is_empty(),
        "the examples are vendored"
    );
    for (name, example) in valid {
        assert!(validator.is_valid(&example), "{name} should be valid");
    }
    for (name, example) in invalid {
        assert!(!validator.is_valid(&example), "{name} should be refused");
    }
    Ok(())
}

#[tokio::test]
async fn the_start_report_is_exactly_the_v2_envelope() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let (_, state) = gateway(dir.path(), (&a.uri(), &b.uri()))?;
    let config = config(
        &dir.path().join("state"),
        "https://report.ferropulse.eu/v1/report",
    )?;
    let report = serde_json::to_value(reporter_over(&config, state)?.start_report().await)?;
    assert_eq!(
        stable(report)?,
        json!({
            "schema": 2,
            "event": "start",
            "first_start": true,
            "product": "ferrofed",
            "instance_id": "*",
            "version": env!("CARGO_PKG_VERSION"),
            "git_sha": "*",
            "licence": "non-commercial",
            "deployment": "compose",
            "uptime_s": "*",
            "cpu_bucket": "*",
            "memory_bucket": "*",
            "status": {
                "profile": "development",
                "members": "1-2",
                "services": ["pixm"],
                "stored_queries": false,
                "fhir": false
            }
        })
    );
    Ok(())
}

#[tokio::test]
async fn the_daily_report_counts_requests_by_route_group_and_node_outcomes_and_never_a_value()
-> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_failing(500).await;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(dir.path(), (&a.uri(), &b.uri()))?;
    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    assert_ne!(
        StatusCode::OK,
        status,
        "node B fails the all-or-nothing query: {text}"
    );
    let read = Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?;
    call(app.clone(), read).await?;
    let probe = Request::get("/health/liveness").body(Body::empty())?;
    assert_eq!(StatusCode::OK, call(app.clone(), probe).await?.0);
    let nowhere = Request::get("/SYNTHETIC-UNMATCHED").body(Body::empty())?;
    assert_eq!(StatusCode::NOT_FOUND, call(app, nowhere).await?.0);

    let config = config(
        &dir.path().join("state"),
        "https://report.ferropulse.eu/v1/report",
    )?;
    let reporter = reporter_over(&config, Arc::clone(&state))?;
    let report = serde_json::to_value(reporter.daily_report().await)?;
    let rendered = report.to_string();
    for value in [PATIENT, EHR_A, "SYNTHETIC", "node-a", "127.0.0.1"] {
        assert!(
            !rendered.contains(value),
            "§5.4.1, N33: {value} reaches no report: {rendered}"
        );
    }
    let report = stable(report)?;
    assert!(report.get("first_start").is_none(), "start only");
    let window = &report["metrics"]["window_24h"];
    assert_eq!(window["requests_bucket"], json!("1-100"));
    let counters = &window["counters"];
    assert!(
        counters["node_errors"].as_u64() >= Some(1),
        "node B answers 500: {counters}"
    );
    assert!(
        counters["node_requests"].as_u64() >= Some(2),
        "the query asks both members: {counters}"
    );
    assert!(
        counters.get("node_timeouts").is_none(),
        "no node request ran past its budget, so the counter is not sent: {counters}"
    );
    let groups = window["groups"].as_object().ok_or("an object")?;
    assert_eq!(
        groups.keys().map(String::as_str).collect::<Vec<_>>(),
        ["ehr", "other", "query"],
        "the probe is not counted and a group without requests is left out"
    );
    let total = |group: &str| -> u64 {
        groups[group]["histogram"]
            .as_array()
            .map_or(0, |counts| counts.iter().filter_map(Value::as_u64).sum())
    };
    assert_eq!((total("query"), total("ehr"), total("other")), (1, 1, 1));
    Ok(())
}

#[tokio::test]
async fn the_start_report_reaches_the_collector_once_per_ten_minutes() -> TestResult {
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/report"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&collector)
        .await;
    let dir = tempfile::tempdir()?;
    let state = Arc::new(AppState::with_health(
        ferrohealth_health::registry::Registry::default(),
    ));
    let config = config(dir.path(), &format!("{}/v1/report", collector.uri()))?;
    let reporter = reporter_over(&config, state)?;
    assert_eq!(
        StartOutcome::Sent(StatusCode::NO_CONTENT),
        reporter.send_start().await?
    );
    assert_eq!(StartOutcome::RateLimited, reporter.send_start().await?);
    let sent = collector.received_requests().await.ok_or("recorded")?;
    let [request] = sent.as_slice() else {
        return Err(format!("one start report, not {}", sent.len()).into());
    };
    let report: Value = serde_json::from_slice(&request.body)?;
    stable(report.clone())?;
    assert_eq!(report["event"], json!("start"));
    assert_eq!(report["status"]["members"], json!("0"));
    Ok(())
}

#[tokio::test]
async fn a_daily_report_that_does_not_reach_the_collector_keeps_its_window() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway(dir.path(), (&a.uri(), &b.uri()))?;
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let unreachable = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let config = config(
        &dir.path().join("state"),
        &format!("http://{unreachable}/v1/report"),
    )?;
    let reporter = reporter_over(&config, Arc::clone(&state))?;
    assert!(reporter.send_daily().await.is_err(), "nothing listens");
    let kept = state.metrics().usage().take();
    assert_eq!(kept.requests(), 1, "the retry carries the window");
    let node_requests = kept
        .counters
        .iter()
        .find(|(key, _)| key.as_str() == "node_requests")
        .map(|(_, count)| *count);
    assert_eq!(Some(2), node_requests);
    Ok(())
}

/// The configuration of a gateway with `table` as its `[usage_report]`.
fn resolved(table: &str) -> Result<ferrofed_server::config::settings::Settings, ConfigError> {
    let text = crate::support::signed(&format!("profile = \"development\"\n{table}"));
    Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()
}

#[test]
fn the_report_is_on_by_default_and_the_switch_turns_it_off() -> TestResult {
    let default = resolved("")?.usage_report;
    assert!(default.enabled, "on by default");
    assert_eq!(
        None, default.state_dir,
        "no instance id without a state directory"
    );
    assert_eq!(Deployment::Unknown, default.deployment);
    assert!(
        !resolved("[usage_report]\nenabled = false\n")?
            .usage_report
            .enabled
    );
    let text = crate::support::signed("profile = \"development\"\n");
    let environment = BTreeMap::from([(
        String::from("FERROFED__USAGE_REPORT__ENABLED"),
        String::from("false"),
    )]);
    let switched = Config::from_sources(Some(&text), &environment)?.resolve()?;
    assert!(
        !switched.usage_report.enabled,
        "the variable the boot line names"
    );
    Ok(())
}

#[test]
fn a_collector_that_is_not_https_or_carries_credentials_is_refused() {
    for endpoint in [
        "http://report.example.org/v1/report",
        "https://user:secret@report.example.org/v1/report",
    ] {
        let table = format!("[usage_report]\nendpoint = \"{endpoint}\"\n");
        assert!(
            matches!(resolved(&table), Err(ConfigError::Parse { ref fault }) if fault.key() == Some("usage_report.endpoint")),
            "{endpoint}"
        );
    }
    assert!(resolved("[usage_report]\nendpoint = \"http://127.0.0.1:9/v1/report\"\n").is_ok());
}
