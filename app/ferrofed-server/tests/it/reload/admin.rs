// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST /admin/reload` on the admin listener: the reload `SIGHUP` asks for,
//! for a container no signal reaches. It is admitted only for the operator
//! scope, recorded with its operator, answers what it reloaded, or `422`
//! with the class of a refusal, and changes the gate of the client face it
//! shares. No specification governs the admin listener: our own design.

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use ferrofed_identity::profile::Profile;
use ferrofed_server::admin::{self, Access, FINISHED, RELOAD};
use ferrofed_server::telemetry::{DEFAULT_FILTER, Rendering, subscriber};
use http::{Request, StatusCode, header};
use serde::Deserialize;

use super::TestResult;
use super::auth::{Trusting, auth_a, auth_a_and_b, bearer_b, status_of};
use crate::support::{self, Logs, error_body, operator_bearer, send_as_is};

/// The fields of an answer the tests read.
#[derive(Debug, Deserialize)]
struct Answer {
    members: usize,
    auth_changed: bool,
    certificates: Vec<String>,
    needs_restart: Vec<String>,
    scope: String,
}

/// The fields of an action record the tests read.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Record {
    event: String,
    action: Option<String>,
    outcome: Option<String>,
    issuer: Option<String>,
}

/// The admin listener of `gateway`, sharing its gate, as a remote peer
/// reaches it under the production profile.
fn listener(gateway: &Trusting) -> Router {
    let access = Access::sharing(Arc::clone(&gateway.trust), Profile::Production, Vec::new());
    let remote = SocketAddr::from(([192, 0, 2, 10], 40_000));
    admin::serving_router(
        Arc::clone(&gateway.state),
        access,
        None,
        Some(Arc::clone(&gateway.reloader)),
    )
    .layer(MockConnectInfo(remote))
}

/// Sends `POST` [`RELOAD`] with `authorization` and returns the status, the
/// body and the captured log text.
async fn reload(
    app: Router,
    authorization: Option<&str>,
) -> Result<(StatusCode, String, String), Box<dyn Error>> {
    let mut request = Request::post(RELOAD);
    if let Some(value) = authorization {
        request = request.header(header::AUTHORIZATION, value);
    }
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, DEFAULT_FILTER, false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let response = send_as_is(app, request.body(Body::empty())?).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    drop(guard);
    Ok((status, String::from_utf8(bytes.to_vec())?, logs.text()))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_reload_needs_the_operator_scope() -> TestResult {
    let gateway = Trusting::start(&auth_a())?;
    let app = listener(&gateway);
    let (status, _, _) = reload(app.clone(), None).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "no credential");
    let (status, _, _) = reload(app, Some(&support::bearer()?)).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "a token without the scope");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_reload_applies_what_sighup_applies_and_is_recorded() -> TestResult {
    let gateway = Trusting::start(&auth_a())?;
    gateway.write_auth(&auth_a_and_b()?)?;

    let (status, body, logs) = reload(listener(&gateway), Some(&operator_bearer()?)).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let answer: Answer = serde_json::from_str(&body)?;
    assert_eq!(1, answer.members);
    assert!(answer.auth_changed, "{body}");
    assert!(answer.certificates.is_empty(), "{body}");
    assert!(answer.needs_restart.is_empty(), "{body}");
    assert_eq!("replica", answer.scope);

    assert_eq!(
        StatusCode::OK,
        status_of(&gateway.app, &bearer_b()?).await?,
        "the client face shares the reloaded gate"
    );

    let records: Vec<Record> = logs
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let action = format!("POST {RELOAD}");
    let admitted = records
        .iter()
        .find(|record| record.event == "admin-write-admitted")
        .ok_or("the action is recorded before it runs")?;
    assert_eq!(Some(action.as_str()), admitted.action.as_deref());
    assert_eq!(Some(support::ISSUER), admitted.issuer.as_deref());
    let finished = records
        .iter()
        .find(|record| record.event == FINISHED)
        .ok_or("the action is recorded with its outcome")?;
    assert_eq!(Some("200"), finished.outcome.as_deref());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_reload_answers_422_with_its_class_and_changes_nothing() -> TestResult {
    let gateway = Trusting::start(&auth_a())?;
    gateway.write_auth("")?;

    let (status, body, _) = reload(listener(&gateway), Some(&operator_bearer()?)).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    let error = error_body(&body)?;
    assert_eq!("configuration-invalid", error.code);
    assert!(error.message.contains("(auth)"), "{}", error.message);
    assert_eq!(
        StatusCode::OK,
        status_of(&gateway.app, &operator_bearer()?).await?,
        "the running issuers stay"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reload_with_a_body_is_refused() -> TestResult {
    let gateway = Trusting::start(&auth_a())?;
    let request = Request::post(RELOAD)
        .header(header::AUTHORIZATION, operator_bearer()?)
        .body(Body::from("{}"))?;
    let response = send_as_is(listener(&gateway), request).await?;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    Ok(())
}
