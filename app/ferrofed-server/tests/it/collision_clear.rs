// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's clear of a held `ehr_id` collision on the admin listener:
//! admitted only for the issuer's operator scope and recorded with the
//! operator, it forgets one `ehr_id` from this process's index and from every
//! session's resolution bindings. A later read probes again and is refused
//! `409` while the collision stands (§12.5.2, N42, CP-33), a write is never
//! probed for (§12.5.1, N41), and no patient identifier reaches the log or
//! the answer (§5.4.1, N33). What a federation does about a collision is its
//! governance (§12b.2, §18); the clear itself is our own design.
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
use ferrofed_registry::ehr_index::Indexed;
use ferrofed_registry::operator::{ClearScope, ClearedEhrId, HeldBefore, IncidentReport};
use ferrofed_server::admin::{self, Access, FINISHED};
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{DEFAULT_FILTER, Rendering, subscriber};
use ferrofed_testkit::mock::Server;
use http::{Method, Request, StatusCode, header};
use serde::Deserialize;
use wiremock::ResponseTemplate;

use crate::ehr_id_collision::{Answered, captured, incidents};
use crate::facade::{
    EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, body, crossref, gateway_within_state, node_answering,
    patient_query, post, registry,
};
use crate::path_ehr_id::holder;
use crate::support::{
    self, ISSUER, Logs, asked, bearer, bearer_as, error_body, mount, operator_bearer, send_as_is,
};

type TestResult = Result<(), Box<dyn Error>>;

/// A version uid node A minted, read under [`EHR_A`].
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// An `ehr_id` that is a valid `HIER_OBJECT_ID` and no UUID, the form a
/// patient identifier written in a path could take.
const OID_EHR: &str = "2.999.1.4417";

/// A peer off loopback, so only the gate admits it.
const REMOTE: ([u8; 4], u16) = ([192, 0, 2, 10], 40_000);

/// The admin listener's application over `state`, in production.
fn admin_over(state: &Arc<AppState>) -> Router {
    let access = Access::new(&support::auth(), Profile::Production, None);
    admin::router(Arc::clone(state), access).layer(MockConnectInfo(SocketAddr::from(REMOTE)))
}

/// The gateway over two mock nodes at `a` and `b`, with the state it serves
/// from.
fn over(
    dir: &std::path::Path,
    a: &Server,
    b: &Server,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    gateway_within_state(
        dir,
        &registry(&a.uri(), &b.uri(), ""),
        "",
        "",
        (2_000, 3_000),
    )
}

/// The clear of `ehr_id` as `authorization`, with its status and body text.
async fn clear(
    admin: &Router,
    ehr_id: &str,
    authorization: Option<&str>,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let mut request = Request::post(format!("/admin/ehr-ids/{ehr_id}/clear"));
    if let Some(value) = authorization {
        request = request.header(header::AUTHORIZATION, value);
    }
    let response = send_as_is(admin.clone(), request.body(Body::empty())?).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, String::from_utf8(bytes.to_vec())?))
}

/// The clear of `ehr_id` by the operator, which must succeed.
async fn cleared(admin: &Router, ehr_id: &str) -> Result<ClearedEhrId, Box<dyn Error>> {
    let (status, text) = clear(admin, ehr_id, Some(&operator_bearer()?)).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(serde_json::from_str(&text)?)
}

/// An untargeted read of the composition [`VERSION_A`] under [`EHR_A`].
fn read() -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}")).body(Body::empty())
}

/// A read of [`EHR_A`] targeted at `endpoint`, which teaches the index the
/// member that answers it.
fn targeted(endpoint: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/ehr/{EHR_A}"))
        .header("openEHR-federation-endpoint", endpoint)
        .body(Body::empty())
}

/// Two members holding [`EHR_A`], and the gateway over them whose index
/// holds the collision two targeted reads taught it.
async fn held_collision(
    dir: &std::path::Path,
) -> Result<(Router, Arc<AppState>, Server, Server), Box<dyn Error>> {
    let a = holder().await;
    let b = holder().await;
    let (app, state) = over(dir, &a, &b)?;
    let (answers, _) =
        captured(&app, vec![targeted("node-a-pub")?, targeted("node-b-pub")?]).await?;
    for (status, _, text) in &answers {
        assert_eq!(StatusCode::OK, *status, "{text}");
    }
    let federation = state.federation().ok_or("a federation")?;
    assert!(
        matches!(
            federation.index().lookup(&EHR_A.parse()?),
            Indexed::Several(_)
        ),
        "the index holds the collision"
    );
    Ok((app, state, a, b))
}

/// The status, the error code and the detection of the one collision
/// incident a refused read raised.
fn refusal(
    answered: &Answered,
    logs: &str,
) -> Result<(StatusCode, String, Option<String>), Box<dyn Error>> {
    let (status, _, text) = answered;
    let code = error_body(text).map(|body| body.code).unwrap_or_default();
    let detection = incidents(logs)?
        .into_iter()
        .rfind(|incident| incident.kind == "EhrIdCollision")
        .and_then(|incident| incident.detection);
    Ok((*status, code, detection))
}

#[tokio::test]
async fn the_clear_is_refused_without_the_operator_scope_and_forgets_nothing() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_app, state, _a, _b) = held_collision(dir.path()).await?;
    let admin = admin_over(&state);
    for (authorization, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some(bearer()?), StatusCode::FORBIDDEN),
    ] {
        let (status, text) = clear(&admin, EHR_A, authorization.as_deref()).await?;
        assert_eq!(expected, status, "{text}");
    }
    let federation = state.federation().ok_or("a federation")?;
    assert!(
        matches!(
            federation.index().lookup(&EHR_A.parse()?),
            Indexed::Several(_)
        ),
        "a refused clear forgets nothing"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_cleared_collision_that_still_stands_is_probed_and_refused_409_again() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, state, a, b) = held_collision(dir.path()).await?;
    let (answers, logs) = captured(&app, vec![read()?]).await?;
    assert_eq!(
        (
            StatusCode::CONFLICT,
            "ehr-id-collision".to_owned(),
            Some("index".to_owned())
        ),
        refusal(answers.first().ok_or("an answer")?, &logs)?,
        "the held collision answers at step 3 (N42): {logs}"
    );
    let answer = cleared(&admin_over(&state), EHR_A).await?;
    assert_eq!(
        ClearedEhrId {
            ehr_id: Some(EHR_A.to_owned()),
            held: HeldBefore::Collision,
            claimants: vec!["node-a".to_owned(), "node-b".to_owned()],
            indexed: true,
            sessions: 0,
            scope: ClearScope::Replica,
        },
        answer
    );
    let (answers, logs) = captured(&app, vec![read()?]).await?;
    assert_eq!(
        (
            StatusCode::CONFLICT,
            "ehr-id-collision".to_owned(),
            Some("ask-all".to_owned())
        ),
        refusal(answers.first().ok_or("an answer")?, &logs)?,
        "the read probes again and finds the collision standing (§12.5.1 step 4, N42): {logs}"
    );
    for server in [&a, &b] {
        let probes = asked(server)
            .await?
            .into_iter()
            .filter(|(verb, at)| verb == "GET" && *at == format!("/v1/ehr/{EHR_A}"))
            .count();
        assert_eq!(
            2, probes,
            "one targeted read and one probe, and no claimant is read"
        );
    }
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_remedied_collision_reads_from_the_claimant_left_once_cleared() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, state, _a, b) = held_collision(dir.path()).await?;
    b.reset().await;
    let (answers, _) = captured(&app, vec![read()?]).await?;
    let (status, _, text) = answers.first().ok_or("an answer")?;
    assert_eq!(
        StatusCode::CONFLICT,
        *status,
        "nothing the gateway observes shows the remedy, so the collision is held: {text}"
    );
    let answer = cleared(&admin_over(&state), EHR_A).await?;
    assert_eq!(HeldBefore::Collision, answer.held);
    let (answers, _) = captured(&app, vec![read()?]).await?;
    let (status, acting, text) = answers.first().ok_or("an answer")?;
    assert_eq!(StatusCode::OK, *status, "{text}");
    assert_eq!(
        Some("node-a-pub"),
        acting.as_deref(),
        "the probe finds the one member left holding it (§12.5.1 step 4)"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_write_after_the_clear_is_never_probed_for() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, state, a, b) = held_collision(dir.path()).await?;
    cleared(&admin_over(&state), EHR_A).await?;
    let before = (asked(&a).await?.len(), asked(&b).await?.len());
    let write = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/ehr/{EHR_A}/composition"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"_type":"COMPOSITION"}"#))?;
    let (answers, _) = captured(&app, vec![write]).await?;
    let (status, acting, text) = answers.first().ok_or("an answer")?;
    assert_eq!(
        (StatusCode::BAD_REQUEST, "target-required".to_owned()),
        (*status, error_body(text)?.code),
        "§12.5.1, N41: {text}"
    );
    assert!(acting.is_none(), "{text}");
    assert_eq!(
        before,
        (asked(&a).await?.len(), asked(&b).await?.len()),
        "no probe and no write reaches a node (N41)"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn the_clear_forgets_the_binding_of_every_session() -> TestResult {
    let a = node_answering(VERSION_A).await;
    let b = node_answering("2c1fd7e4-6a43-4d0e-9a5e-6f1b2f1d6a01::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway_within_state(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &crossref(&[("node-a", EHR_A), ("node-b", EHR_A)]),
        (2_000, 3_000),
    )?;
    let mut another = post(body(&patient_query())?)?;
    another
        .headers_mut()
        .insert(header::AUTHORIZATION, bearer_as("another-caller")?.parse()?);
    let (answers, logs) =
        captured(&app, vec![post(body(&patient_query())?)?, another, read()?]).await?;
    let mut answers = answers.into_iter();
    for _ in 0..2 {
        let (status, _, text) = answers.next().ok_or("the query answered")?;
        assert_eq!(StatusCode::OK, status, "{text}");
    }
    assert_eq!(
        (
            StatusCode::CONFLICT,
            "ehr-id-collision".to_owned(),
            Some("binding".to_owned())
        ),
        refusal(&answers.next().ok_or("the read answered")?, &logs)?,
        "{logs}"
    );
    let answer = cleared(&admin_over(&state), EHR_A).await?;
    assert_eq!(
        (HeldBefore::Collision, true, 2),
        (answer.held, answer.indexed, answer.sessions),
        "both callers' bindings and the index entry are forgotten"
    );
    for server in [&a, &b] {
        mount(
            server,
            "GET",
            format!("/v1/ehr/{EHR_A}"),
            ResponseTemplate::new(200).set_body_raw(
                format!(r#"{{"ehr_id":{{"value":"{EHR_A}"}}}}"#).into_bytes(),
                "application/json",
            ),
        )
        .await;
    }
    let (answers, logs) = captured(&app, vec![read()?]).await?;
    assert_eq!(
        (
            StatusCode::CONFLICT,
            "ehr-id-collision".to_owned(),
            Some("ask-all".to_owned())
        ),
        refusal(answers.first().ok_or("an answer")?, &logs)?,
        "the querying caller's binding is gone, so the read probes (N42): {logs}"
    );
    Ok(())
}

#[tokio::test]
async fn an_ehr_id_held_nowhere_answers_that_nothing_was_held() -> TestResult {
    let a = holder().await;
    let b = holder().await;
    let dir = tempfile::tempdir()?;
    let (_app, state) = over(dir.path(), &a, &b)?;
    let answer = cleared(&admin_over(&state), EHR_A).await?;
    assert_eq!(
        (HeldBefore::Nothing, false, 0, true),
        (
            answer.held,
            answer.indexed,
            answer.sessions,
            answer.claimants.is_empty()
        )
    );
    assert_eq!(
        ClearScope::Replica,
        answer.scope,
        "the answer says it cleared this replica"
    );
    Ok(())
}

#[tokio::test]
async fn a_value_that_is_no_ehr_id_and_a_body_are_refused_400() -> TestResult {
    let a = holder().await;
    let b = holder().await;
    let dir = tempfile::tempdir()?;
    let (_app, state) = over(dir.path(), &a, &b)?;
    let admin = admin_over(&state);
    let (status, text) = clear(&admin, "not%20an%20ehr_id", Some(&operator_bearer()?)).await?;
    assert_eq!(
        (StatusCode::BAD_REQUEST, "ehr-id-invalid".to_owned()),
        (status, error_body(&text)?.code),
        "{text}"
    );
    assert!(
        !text.contains("not an ehr_id"),
        "the value is not echoed: {text}"
    );
    let (status, _) = clear(&admin, "%FF%FE", Some(&operator_bearer()?)).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status);
    let request = Request::post(format!("/admin/ehr-ids/{EHR_A}/clear"))
        .header(header::AUTHORIZATION, operator_bearer()?)
        .body(Body::from(PATIENT))?;
    let response = send_as_is(admin, request).await?;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    assert_eq!("body-invalid", error_body(&text)?.code, "{text}");
    Ok(())
}

/// The fields of an admin action record the test reads.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Record {
    event: String,
    issuer: Option<String>,
    subject: Option<String>,
    action: Option<String>,
    outcome: Option<String>,
}

// conformance: CP-26 CP-33
#[tokio::test]
async fn the_clear_is_recorded_with_its_operator_listed_and_names_no_patient_identifier()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let (_app, state, _a, _b) = held_collision(dir.path()).await?;
    let federation = state.federation().ok_or("a federation")?;
    federation
        .index()
        .learn(&OID_EHR.parse()?, &"node-a".parse()?);
    federation
        .index()
        .learn(&OID_EHR.parse()?, &"node-b".parse()?);
    let admin = admin_over(&state);
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, DEFAULT_FILTER, false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let uuid = cleared(&admin, EHR_A).await?;
    let oid = cleared(&admin, OID_EHR).await?;
    drop(guard);
    let text = logs.text();
    let mut finished = Vec::new();
    for line in text.lines() {
        let record: Record = serde_json::from_str(line)?;
        if record.event == FINISHED {
            finished.push(record);
        }
    }
    assert_eq!(2, finished.len(), "{text}");
    for record in &finished {
        assert_eq!(Some(ISSUER), record.issuer.as_deref());
        assert_eq!(
            Some(support::claims().sub.as_str()),
            record.subject.as_deref()
        );
        assert_eq!(
            Some(format!("POST {}", admin::CLEAR_EHR_ID).as_str()),
            record.action.as_deref()
        );
        assert_eq!(Some("200"), record.outcome.as_deref());
    }
    assert_eq!(
        (HeldBefore::Collision, None),
        (oid.held, oid.ehr_id.as_deref())
    );
    for hidden in [OID_EHR, PATIENT, PATIENT_TAIL, NAMESPACE] {
        assert!(!text.contains(hidden), "the log carries {hidden}: {text}");
    }
    let listed = IncidentReport::current().cleared;
    assert!(
        listed
            .iter()
            .any(|clear| clear.ehr_id.as_deref() == uuid.ehr_id.as_deref()
                && clear.claimants == uuid.claimants),
        "the incident report lists the clear beside the incidents: {listed:?}"
    );
    Ok(())
}
