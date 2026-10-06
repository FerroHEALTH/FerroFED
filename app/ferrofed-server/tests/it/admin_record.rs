// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The record of each admin listener write action an admitted operator runs:
//! one line under `ferrofed::security` before the action has any effect,
//! counted as `admin-write-admitted`, and one with its outcome once it ends.
//! Each names the operator's issuer and subject, the action and the time,
//! and nothing else, no log filter quiets them, and a refused request leaves
//! neither. No specification governs the admin listener: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use ferrofed_identity::dev::Profile;
use ferrofed_server::admin::{self, Access, DISTRIBUTE, FINISHED};
use ferrofed_server::metrics::security::Event;
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{DEFAULT_FILTER, Rendering, subscriber};
use http::{Request, StatusCode, header};
use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::support::{self, ISSUER, Logs, OPERATOR_SCOPE, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// The operator's distribution of a stored-query version.
const WRITE: &str = "/admin/stored-queries/example::query/1.0.0/distribute";

/// The synthetic subject of the operator the tests mint a token for.
const OPERATOR: &str = "synthetic-operator-7c2e";

/// The event that opens an admitted write action.
const ADMITTED: &str = "admin-write-admitted";

/// The fields an action record may carry: the formatter's own, then the
/// event, the operator, the action, the outcome and the time.
const ALLOWED: [&str; 10] = [
    "timestamp",
    "level",
    "target",
    "message",
    "event",
    "issuer",
    "subject",
    "action",
    "outcome",
    "at",
];

/// The fields of an action record the tests read.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Record {
    /// The security event.
    event: String,
    /// The operator's issuer.
    issuer: Option<String>,
    /// The operator's subject.
    subject: Option<String>,
    /// The method and route template of the action.
    action: Option<String>,
    /// The outcome, on the closing record.
    outcome: Option<String>,
    /// When the record was made.
    at: Option<String>,
}

/// The `Authorization` value of the operator [`OPERATOR`].
fn operator() -> Result<String, Box<dyn Error>> {
    let mut claims = support::claims();
    OPERATOR.clone_into(&mut claims.sub);
    let scope = claims.scope.take().unwrap_or_default();
    claims.scope = Some(format!("{scope} {OPERATOR_SCOPE}").trim().to_owned());
    Ok(format!("Bearer {}", support::issuer().mint(&claims)?))
}

/// The admin listener's application under `profile`, as a request from
/// `peer` reaches it.
fn listener(profile: Profile, peer: SocketAddr) -> Router {
    let access = Access::new(&support::auth(), profile, None);
    admin::router(Arc::new(AppState::default()), access).layer(MockConnectInfo(peer))
}

/// Sends a `POST` of [`WRITE`] with `authorization` to `app`, logging
/// through `filter`, and returns the status and the captured text.
async fn write(
    app: Router,
    authorization: Option<&str>,
    filter: &str,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let mut request = Request::post(WRITE);
    if let Some(value) = authorization {
        request = request.header(header::AUTHORIZATION, value);
    }
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, filter, false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let response = send_as_is(app, request.body(Body::empty())?).await?;
    drop(guard);
    Ok((response.status(), logs.text()))
}

/// The action records in `text`, in order, each with the names of its
/// fields.
fn records(text: &str) -> Result<Vec<(Record, BTreeSet<String>)>, serde_json::Error> {
    let mut found = Vec::new();
    for line in text.lines() {
        let record: Record = serde_json::from_str(line)?;
        if record.event == ADMITTED || record.event == FINISHED {
            #[expect(
                clippy::zero_sized_map_values,
                reason = "only the names of a line's fields are read, and serde reads an object into a map"
            )]
            let fields: BTreeMap<String, IgnoredAny> = serde_json::from_str(line)?;
            found.push((record, fields.into_keys().collect()));
        }
    }
    Ok(found)
}

#[tokio::test]
async fn an_admitted_write_is_recorded_before_it_runs_and_with_its_outcome() -> TestResult {
    let before = Event::AdminWriteAdmitted.counted();
    let credential = operator()?;
    let remote = SocketAddr::from(([192, 0, 2, 10], 40_000));
    let app = listener(Profile::Production, remote);
    let (status, text) = write(app, Some(&credential), DEFAULT_FILTER).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "the action ran: {text}");
    let found = records(&text)?;
    let [(opened, _), (closed, _)] = found.as_slice() else {
        return Err(format!("an opening and a closing record: {text}").into());
    };
    assert_eq!(ADMITTED, opened.event, "the opening record comes first");
    assert_eq!(FINISHED, closed.event);
    let action = format!("POST {DISTRIBUTE}");
    for record in [opened, closed] {
        assert_eq!(Some(ISSUER), record.issuer.as_deref());
        assert_eq!(Some(OPERATOR), record.subject.as_deref());
        assert_eq!(Some(action.as_str()), record.action.as_deref());
        let at = record.at.as_deref().ok_or("the time is recorded")?;
        at.parse::<jiff::Timestamp>()?;
    }
    assert_eq!(None, opened.outcome, "the outcome is not known yet");
    assert_eq!(Some("404"), closed.outcome.as_deref());
    assert_eq!(before + 1, Event::AdminWriteAdmitted.counted());
    Ok(())
}

#[tokio::test]
async fn a_record_carries_the_operator_the_action_the_outcome_and_the_time_alone() -> TestResult {
    let credential = operator()?;
    let remote = SocketAddr::from(([192, 0, 2, 10], 40_000));
    let app = listener(Profile::Production, remote);
    let (_, text) = write(app, Some(&credential), "trace").await?;
    let found = records(&text)?;
    assert_eq!(2, found.len(), "{text}");
    let allowed: BTreeSet<String> = ALLOWED.iter().map(|&name| name.to_owned()).collect();
    for (_, fields) in &found {
        let extra: Vec<&String> = fields.difference(&allowed).collect();
        assert!(extra.is_empty(), "a record carries {extra:?}: {text}");
    }
    let token = credential.trim_start_matches("Bearer ");
    for hidden in [token, "Bearer", "example::query", "1.0.0"] {
        assert!(!text.contains(hidden), "the log carries {hidden}: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn no_log_filter_quiets_the_record() -> TestResult {
    let remote = SocketAddr::from(([192, 0, 2, 10], 40_000));
    for filter in [
        DEFAULT_FILTER,
        "warn",
        "error",
        "off",
        "ferrofed::security=off",
        "off,ferrofed::security=error",
    ] {
        let app = listener(Profile::Production, remote);
        let (_, text) = write(app, Some(&operator()?), filter).await?;
        let events: Vec<String> = records(&text)?
            .into_iter()
            .map(|(record, _)| record.event)
            .collect();
        assert_eq!(
            vec![ADMITTED.to_owned(), FINISHED.to_owned()],
            events,
            "{filter}: {text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_write_leaves_no_action_record() -> TestResult {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 40_000));
    for authorization in [None, Some(support::bearer()?)] {
        let before = Event::AdminWriteAdmitted.counted();
        let app = listener(Profile::Production, loopback);
        let (status, text) = write(app, authorization.as_deref(), DEFAULT_FILTER).await?;
        assert!(
            status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
            "{status}: {text}"
        );
        assert!(records(&text)?.is_empty(), "{text}");
        assert_eq!(before, Event::AdminWriteAdmitted.counted());
    }
    Ok(())
}

#[tokio::test]
async fn a_development_loopback_write_is_recorded_without_an_operator() -> TestResult {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 40_000));
    let app = listener(Profile::Development, loopback);
    let (status, text) = write(app, None, DEFAULT_FILTER).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "the action ran: {text}");
    let found = records(&text)?;
    assert_eq!(2, found.len(), "{text}");
    for (record, _) in &found {
        assert_eq!(None, record.issuer, "no issuer vouched for it");
        assert_eq!(None, record.subject);
    }
    Ok(())
}

#[tokio::test]
async fn neither_the_issuer_nor_the_subject_is_a_metric_label() -> TestResult {
    let remote = SocketAddr::from(([192, 0, 2, 10], 40_000));
    let app = listener(Profile::Production, remote);
    let (status, _) = write(app.clone(), Some(&operator()?), DEFAULT_FILTER).await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    let response = send_as_is(app, Request::get("/metrics").body(Body::empty())?).await?;
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await?;
    let exposition = String::from_utf8(bytes.to_vec())?;
    assert!(
        exposition.contains("event=\"admin-write-admitted\""),
        "the record is counted by its event alone: {exposition}"
    );
    for hidden in [OPERATOR, ISSUER, "issuer.example.test"] {
        assert!(!exposition.contains(hidden), "the metrics carry {hidden}");
    }
    Ok(())
}
