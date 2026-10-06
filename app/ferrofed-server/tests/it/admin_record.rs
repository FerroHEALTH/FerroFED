// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The record of each admin listener write action an admitted operator ran:
//! one line under `ferrofed::security` naming the operator's issuer and
//! subject, the action, its outcome and the time, counted as
//! `admin-write-admitted`, and never a credential. A refused request leaves
//! no such record. No specification governs the admin listener: our own
//! design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use ferrofed_identity::dev::Profile;
use ferrofed_server::admin::{self, Access, DISTRIBUTE};
use ferrofed_server::metrics::security::Event;
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::{Request, StatusCode, header};
use serde::Deserialize;

use crate::support::{self, ISSUER, Logs, OPERATOR_SCOPE, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// The operator's distribution of a stored-query version.
const WRITE: &str = "/admin/stored-queries/example::query/1.0.0/distribute";

/// The synthetic subject of the operator the tests mint a token for.
const OPERATOR: &str = "synthetic-operator-7c2e";

/// The event of an admitted write action.
const ADMITTED: &str = "admin-write-admitted";

/// The fields of an action record the tests read.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Record {
    /// The security event.
    event: String,
    /// How the operator was admitted.
    admitted_by: Option<String>,
    /// The operator's issuer.
    issuer: Option<String>,
    /// The operator's subject.
    subject: Option<String>,
    /// The method of the action.
    method: Option<String>,
    /// The route template of the action.
    action: Option<String>,
    /// The status the action was answered.
    status: Option<u16>,
    /// When the action ran.
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

/// Sends a `POST` of [`WRITE`] with `authorization` to `app`, capturing the
/// log, and returns the status and the captured text.
async fn write(
    app: Router,
    authorization: Option<&str>,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let mut request = Request::post(WRITE);
    if let Some(value) = authorization {
        request = request.header(header::AUTHORIZATION, value);
    }
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let response = send_as_is(app, request.body(Body::empty())?).await?;
    drop(guard);
    Ok((response.status(), logs.text()))
}

/// The action records in `text`.
fn records(text: &str) -> Result<Vec<Record>, serde_json::Error> {
    Ok(text
        .lines()
        .map(serde_json::from_str::<Record>)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|record| record.event == ADMITTED)
        .collect())
}

#[tokio::test]
async fn an_admitted_write_is_recorded_with_its_operator_and_no_credential() -> TestResult {
    let before = Event::AdminWriteAdmitted.counted();
    let credential = operator()?;
    let remote = SocketAddr::from(([192, 0, 2, 10], 40_000));
    let (status, text) = write(listener(Profile::Production, remote), Some(&credential)).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "the action ran: {text}");
    let found = records(&text)?;
    let [record] = found.as_slice() else {
        return Err(format!("one record: {text}").into());
    };
    assert_eq!(Some("token"), record.admitted_by.as_deref());
    assert_eq!(Some(ISSUER), record.issuer.as_deref());
    assert_eq!(Some(OPERATOR), record.subject.as_deref());
    assert_eq!(Some("POST"), record.method.as_deref());
    assert_eq!(Some(DISTRIBUTE), record.action.as_deref());
    assert_eq!(Some(404), record.status, "the outcome is recorded");
    let at = record.at.as_deref().ok_or("the time is recorded")?;
    at.parse::<jiff::Timestamp>()?;
    let token = credential.trim_start_matches("Bearer ");
    assert!(!text.contains(token), "the token is never logged: {text}");
    assert!(
        !text.contains("example::query"),
        "the record names the route template, never a path value: {text}"
    );
    assert_eq!(before + 1, Event::AdminWriteAdmitted.counted());
    Ok(())
}

#[tokio::test]
async fn a_refused_write_leaves_no_action_record() -> TestResult {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 40_000));
    for authorization in [None, Some(support::bearer()?)] {
        let before = Event::AdminWriteAdmitted.counted();
        let app = listener(Profile::Production, loopback);
        let (status, text) = write(app, authorization.as_deref()).await?;
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
async fn a_development_loopback_write_is_recorded_as_such() -> TestResult {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 40_000));
    let (status, text) = write(listener(Profile::Development, loopback), None).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "the action ran: {text}");
    let found = records(&text)?;
    let [record] = found.as_slice() else {
        return Err(format!("one record: {text}").into());
    };
    assert_eq!(Some("development-loopback"), record.admitted_by.as_deref());
    assert_eq!(None, record.issuer, "no issuer vouched for it");
    assert_eq!(None, record.subject);
    assert_eq!(Some(404), record.status);
    Ok(())
}
