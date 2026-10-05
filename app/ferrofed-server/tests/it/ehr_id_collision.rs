// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An `ehr_id` more than one member claims (§12.5.2, N42, CP-33), the gateway
//! half of track 11 over two mock nodes:
//! the request is refused `409` listing the claiming endpoints, on a write as
//! on a read, no claimant is sent it, and the integrity incident is emitted
//! once under `ferrofed::integrity`; the `ehr_id` index raises the
//! index-insert alarm of §12b.2 once when it learns a second claimant, and then
//! routes neither. No incident and no log line names a patient identifier
//! (§5.4.1, N33).
//!
//! The duplicate `ehr_id` is a fixture the test injects at two mock nodes, a
//! state §12b.2 forbids a member to reach on its own (§16.3, track 11).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use ferrofed_identity::session::{ResolutionBindings, SessionKey};
use ferrofed_registry::ehr_index::EhrIndex;
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_registry::incident::{Detection, TARGET};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_server::facade::owner::{self, Claimed, Held, Located};
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_testkit::mock::Server;
use http::{HeaderMap, Method, Request, StatusCode, header};
use serde::Deserialize;
use serde::de::IgnoredAny;
use wiremock::ResponseTemplate;

use crate::declared::composition_at;
use crate::facade::{
    EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, body, dev_gateway, node_answering, patient_query,
    post, registry,
};
use crate::path_ehr_id::{answer, holder, over, probe_at};
use crate::support::{Logs, asked, bearer_as, error_body, mount};

type TestResult = Result<(), Box<dyn Error>>;

const ENDPOINT_A: &str = "node-a-pub";
const ENDPOINT_B: &str = "node-b-pub";

/// The claimants as the `409` message and the incident list them.
const BOTH: &str = "[node-a-pub, node-b-pub]";

/// A version uid node A minted.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// An `ehr_id` that is a valid `HIER_OBJECT_ID` and no UUID: an ISO OID, the
/// form a patient identifier written in a path could take.
const OID_EHR: &str = "2.999.1.4417";

/// One integrity incident as the JSON log records it.
#[derive(Debug, Deserialize)]
pub(crate) struct Logged {
    pub(crate) level: String,
    pub(crate) kind: String,
    pub(crate) ehr_id: Option<String>,
    pub(crate) detection: Option<String>,
    pub(crate) claimants: String,
}

/// The target of one JSON log line.
#[derive(Debug, Deserialize)]
struct Targeted {
    target: String,
}

/// Every integrity incident in `text`, in order.
pub(crate) fn incidents(text: &str) -> Result<Vec<Logged>, serde_json::Error> {
    let mut found = Vec::new();
    for line in text.lines() {
        let targeted: Targeted = serde_json::from_str(line)?;
        if targeted.target == TARGET {
            found.push(serde_json::from_str(line)?);
        }
    }
    Ok(found)
}

/// The field names of every integrity incident in `text`.
#[expect(
    clippy::zero_sized_map_values,
    reason = "only the field names of a line are read, never their values"
)]
fn incident_fields(text: &str) -> Result<Vec<Vec<String>>, serde_json::Error> {
    let mut found = Vec::new();
    for line in text.lines() {
        let targeted: Targeted = serde_json::from_str(line)?;
        if targeted.target == TARGET {
            let fields: BTreeMap<String, IgnoredAny> = serde_json::from_str(line)?;
            found.push(fields.into_keys().collect());
        }
    }
    Ok(found)
}

/// What a request answered: its status, the acting endpoint and the body
/// text.
pub(crate) type Answered = (StatusCode, Option<String>, String);

/// Sends `requests` through `app` one after another under a capturing JSON
/// subscriber, and returns each answer and everything logged.
pub(crate) async fn captured(
    app: &Router,
    requests: Vec<Request<Body>>,
) -> Result<(Vec<Answered>, String), Box<dyn Error>> {
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let mut answers = Vec::new();
    for request in requests {
        answers.push(answer(app.clone(), request).await?);
    }
    drop(guard);
    Ok((answers, logs.text()))
}

/// The single answer of a run of one request.
fn only(answers: Vec<Answered>) -> Result<Answered, Box<dyn Error>> {
    let mut answers = answers.into_iter();
    let first = answers.next().ok_or("one answer")?;
    assert!(answers.next().is_none(), "one request, one answer");
    Ok(first)
}

/// Asserts that `answered` is the `409` of N42 listing both claimants,
/// quoting no `ehr_id`, with no endpoint acting.
fn refused_naming_both(answered: &Answered) -> TestResult {
    let (status, acting, text) = answered;
    assert_eq!(
        (StatusCode::CONFLICT, "ehr-id-collision".to_owned()),
        (*status, error_body(text)?.code),
        "§12.5.2, N42: {text}"
    );
    assert!(text.contains(BOTH), "the claimants are listed: {text}");
    assert!(!text.contains(EHR_A), "the message quotes no path: {text}");
    assert!(acting.is_none(), "no endpoint acted: {text}");
    Ok(())
}

/// The gateway over two mock nodes that both answer the patient query, and
/// a development cross-reference resolving the patient to [`EHR_A`] at both:
/// two members claim one `ehr_id`.
async fn resolving_at_both(
    dir: &std::path::Path,
) -> Result<(Router, Server, Server), Box<dyn Error>> {
    let a = node_answering("8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1").await;
    let b = node_answering("2c1fd7e4-6a43-4d0e-9a5e-6f1b2f1d6a01::cdr-b.example.org::1").await;
    let app = dev_gateway(
        dir,
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_A)],
    )?;
    Ok((app, a, b))
}

// conformance: CP-33
#[tokio::test]
async fn a_probe_collision_is_a_409_naming_both_with_one_incident_and_neither_is_read() -> TestResult
{
    let a = holder().await;
    let b = holder().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (answers, logs) =
        captured(&app, vec![Request::get(&resource).body(Body::empty())?]).await?;
    refused_naming_both(&only(answers)?)?;
    for server in [&a, &b] {
        assert_eq!(
            vec![probe_at()],
            asked(server).await?,
            "no claimant is read (§12.5.2)"
        );
    }
    let found = incidents(&logs)?;
    assert_eq!(1, found.len(), "one incident for one refusal: {logs}");
    let incident = found.first().ok_or("one incident")?;
    assert_eq!("ERROR", incident.level);
    assert_eq!("EhrIdCollision", incident.kind);
    assert_eq!(Some("ask-all"), incident.detection.as_deref());
    assert_eq!(BOTH, incident.claimants);
    assert_eq!(
        Some(EHR_A),
        incident.ehr_id.as_deref(),
        "a UUID ehr_id is node-local and named (§5.2)"
    );
    Ok(())
}

// conformance: CP-33
#[test]
fn a_binding_collision_names_both_claimants_and_raises_one_incident() -> TestResult {
    let snapshot = RegistrySnapshot::from_toml_str(&registry(
        "http://127.0.0.1:9/a",
        "http://127.0.0.1:9/b",
        "",
    ))?;
    let ehr_id: EhrId = EHR_A.parse()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(
        &session,
        now,
        [(&"node-a".parse()?, &ehr_id), (&"node-b".parse()?, &ehr_id)],
    );
    let index = EhrIndex::new(std::num::NonZeroUsize::MIN);
    index.learn(&ehr_id, &"node-a".parse()?);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(&snapshot, &HeaderMap::new(), Some(held), &index, &ehr_id)?;
    let Located::Collision(Claimed {
        claimants,
        detection,
    }) = located
    else {
        return Err(format!("a collision, never a pick of one claimant (N42): {located:?}").into());
    };
    assert_eq!(Detection::Binding, detection);
    let listed: Vec<&str> = claimants.iter().map(EndpointId::as_str).collect();
    assert_eq!(vec![ENDPOINT_A, ENDPOINT_B], listed);
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    tracing::subscriber::with_default(capture, || {
        owner::collided(&ehr_id, detection, &claimants);
    });
    let text = logs.text();
    let found = incidents(&text)?;
    assert_eq!(1, found.len(), "{text}");
    let logged = found.first().ok_or("one incident")?;
    assert_eq!(
        (Some("binding"), BOTH),
        (logged.detection.as_deref(), logged.claimants.as_str())
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn the_index_insert_alarm_fires_once_and_the_index_then_routes_neither() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, a, b) = resolving_at_both(dir.path()).await?;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    // NOTE: §12.5.1 step 2 answers the querying caller first, so another caller,
    // who holds no binding, reads through the index (step 3).
    let (answers, logs) = captured(
        &app,
        vec![
            post(body(&patient_query())?)?,
            post(body(&patient_query())?)?,
            Request::get(&resource)
                .header(header::AUTHORIZATION, bearer_as("another-caller")?)
                .body(Body::empty())?,
        ],
    )
    .await?;
    let mut answers = answers.into_iter();
    for _ in 0..2 {
        let (status, _, text) = answers.next().ok_or("the query answered")?;
        assert_eq!(StatusCode::OK, status, "{text}");
    }
    refused_naming_both(&answers.next().ok_or("the read answered")?)?;
    let found = incidents(&logs)?;
    let kinds: Vec<(&str, Option<&str>)> = found
        .iter()
        .map(|incident| (incident.kind.as_str(), incident.detection.as_deref()))
        .collect();
    assert_eq!(
        vec![
            ("IndexInsertCollision", None),
            ("EhrIdCollision", Some("index"))
        ],
        kinds,
        "the alarm fires once, at the insert (§12b.2), and the refused read raises its own (N42): {logs}"
    );
    let alarm = found.first().ok_or("the alarm")?;
    assert_eq!("[node-a, node-b]", alarm.claimants, "the claiming members");
    for server in [&a, &b] {
        let paths: Vec<(String, String)> = asked(server).await?;
        assert!(
            paths
                .iter()
                .all(|(verb, at)| verb == "POST" && at == "/v1/query/aql"),
            "the index routes neither, and nothing is probed: {paths:?}"
        );
    }
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn the_querying_callers_binding_finds_the_collision_first_and_routes_neither() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, a, b) = resolving_at_both(dir.path()).await?;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (answers, logs) = captured(
        &app,
        vec![
            post(body(&patient_query())?)?,
            Request::get(&resource).body(Body::empty())?,
        ],
    )
    .await?;
    let mut answers = answers.into_iter();
    let (status, _, text) = answers.next().ok_or("the query answered")?;
    assert_eq!(StatusCode::OK, status, "{text}");
    refused_naming_both(&answers.next().ok_or("the read answered")?)?;
    let found = incidents(&logs)?;
    let refused = found.last().ok_or("the refusal's incident")?;
    assert_eq!(
        ("EhrIdCollision", Some("binding")),
        (refused.kind.as_str(), refused.detection.as_deref()),
        "step 2 holds both claimants for the caller who queried (§12.5.1, N42): {logs}"
    );
    for server in [&a, &b] {
        let paths: Vec<(String, String)> = asked(server).await?;
        assert!(
            paths
                .iter()
                .all(|(verb, at)| verb == "POST" && at == "/v1/query/aql"),
            "no claimant is read, and nothing is probed: {paths:?}"
        );
    }
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_write_in_collision_is_refused_409_and_reaches_no_node() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, a, b) = resolving_at_both(dir.path()).await?;
    let mut requests = vec![post(body(&patient_query())?)?];
    for (verb, at) in [
        (Method::POST, format!("/v1/ehr/{EHR_A}/composition")),
        (Method::PUT, composition_at(&Method::PUT, VERSION_A)),
        (
            Method::DELETE,
            format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ),
    ] {
        requests.push(
            Request::builder()
                .method(verb)
                .uri(at)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"_type":"COMPOSITION"}"#))?,
        );
    }
    let (answers, logs) = captured(&app, requests).await?;
    let mut answers = answers.into_iter();
    let (queried, _, text) = answers.next().ok_or("the query answered")?;
    assert_eq!(StatusCode::OK, queried, "{text}");
    for answered in answers {
        refused_naming_both(&answered)?;
    }
    let refusals = incidents(&logs)?
        .into_iter()
        .filter(|incident| incident.kind == "EhrIdCollision")
        .count();
    assert_eq!(3, refusals, "one incident per refused write: {logs}");
    for server in [&a, &b] {
        let paths = asked(server).await?;
        assert_eq!(
            vec![("POST".to_owned(), "/v1/query/aql".to_owned())],
            paths,
            "no write is applied, and nothing is probed (§12.5.2, N41)"
        );
    }
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn an_explicit_target_routes_an_ehr_id_in_collision_to_the_named_node() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, a, _b) = resolving_at_both(dir.path()).await?;
    mount(
        &a,
        "GET",
        format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ResponseTemplate::new(200),
    )
    .await;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (answers, logs) = captured(
        &app,
        vec![
            post(body(&patient_query())?)?,
            Request::get(&resource)
                .header("openEHR-federation-endpoint", ENDPOINT_A)
                .body(Body::empty())?,
        ],
    )
    .await?;
    let (status, acting, text) = answers.into_iter().nth(1).ok_or("the read answered")?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_A),
        acting.as_deref(),
        "step 1 is unambiguous, so no later step is taken (§12.5.1, N41)"
    );
    let refusals = incidents(&logs)?
        .into_iter()
        .filter(|incident| incident.kind == "EhrIdCollision")
        .count();
    assert_eq!(0, refusals, "nothing was refused: {logs}");
    Ok(())
}

// conformance: CP-26 CP-33
#[tokio::test]
async fn no_incident_or_log_line_names_a_patient_identifier() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, _a, _b) = resolving_at_both(dir.path()).await?;
    let (answers, logs) = captured(
        &app,
        vec![
            post(body(&patient_query())?)?,
            Request::get(format!("/v1/ehr/{EHR_A}"))
                .header("x-request-id", format!("req-{PATIENT}"))
                .body(Body::empty())?,
        ],
    )
    .await?;
    refused_naming_both(answers.get(1).ok_or("the read answered")?)?;
    for needle in [PATIENT, PATIENT_TAIL, NAMESPACE] {
        assert!(!logs.contains(needle), "{needle} reached the log: {logs}");
    }
    let expected = |extra: &[&str]| {
        let mut fields: Vec<String> = ["claimants", "ehr_id", "kind", "level", "message"]
            .into_iter()
            .chain(["target", "timestamp"])
            .chain(extra.iter().copied())
            .map(str::to_owned)
            .collect();
        fields.sort();
        fields
    };
    assert_eq!(
        vec![expected(&[]), expected(&["detection"])],
        incident_fields(&logs)?,
        "an incident carries its kind and routing ids only: {logs}"
    );
    Ok(())
}

// conformance: CP-26 CP-33
#[tokio::test]
async fn an_ehr_id_that_is_no_uuid_is_never_named_by_an_incident() -> TestResult {
    let a = Server::start().await;
    let b = Server::start().await;
    for server in [&a, &b] {
        mount(
            server,
            "GET",
            format!("/v1/ehr/{OID_EHR}"),
            ResponseTemplate::new(200),
        )
        .await;
    }
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let targeted = |endpoint: &str| {
        Request::get(format!("/v1/ehr/{OID_EHR}"))
            .header("openEHR-federation-endpoint", endpoint)
            .body(Body::empty())
    };
    let (answers, logs) = captured(
        &app,
        vec![
            targeted(ENDPOINT_A)?,
            targeted(ENDPOINT_B)?,
            Request::get(format!("/v1/ehr/{OID_EHR}")).body(Body::empty())?,
        ],
    )
    .await?;
    let statuses: Vec<StatusCode> = answers.iter().map(|(status, _, _)| *status).collect();
    assert_eq!(
        vec![StatusCode::OK, StatusCode::OK, StatusCode::CONFLICT],
        statuses,
        "two targeted reads teach the index both members, and an untargeted one is refused"
    );
    let found = incidents(&logs)?;
    let kinds: Vec<&str> = found
        .iter()
        .map(|incident| incident.kind.as_str())
        .collect();
    assert_eq!(
        vec!["IndexInsertCollision", "EhrIdCollision"],
        kinds,
        "{logs}"
    );
    assert!(
        found.iter().all(|incident| incident.ehr_id.is_none()),
        "an ehr_id that could be a patient identifier is not named (§5.4.1, N33): {logs}"
    );
    assert!(!logs.contains(OID_EHR), "{logs}");
    Ok(())
}
