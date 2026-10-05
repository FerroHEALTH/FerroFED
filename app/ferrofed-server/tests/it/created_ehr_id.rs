// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `PUT {base}/v1/ehr/{ehr_id}` against what the gateway already knows of the
//! `ehr_id` (§12.4, §12.5.2, N23, N42; ITS-REST 1.1.0 `ehr_create_with_id`):
//! a create targeting one member while a resolution binding or the `ehr_id`
//! index places the `ehr_id` at another is refused `409` (`ehr-id-held`) and
//! sent to no node; one targeting the member that holds it is forwarded, so
//! that node answers its own `409`; an `ehr_id` nothing places is forwarded
//! and, once created, indexed at the targeted member; two creates racing for
//! one `ehr_id` at two members raise the index-insert alarm of §12b.2.
//!
//! Every assertion on what a node received reads the node's own capture
//! (§16, track 6).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use ferrofed_identity::session::{ResolutionBindings, SessionKey};
use ferrofed_registry::ehr_index::EhrIndex;
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_registry::incident::{Detection, TARGET};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_server::facade::owner::{self, Held, HeldElsewhere};
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use serde::Deserialize;
use wiremock::ResponseTemplate;

use crate::facade::{EHR_A, EHR_B, registry};
use crate::path_ehr_id::{answer, holder, over};
use crate::support::{Logs, asked, error_body, mount};

type TestResult = Result<(), Box<dyn Error>>;

const ENDPOINT_A: &str = "node-a-pub";
const ENDPOINT_B: &str = "node-b-pub";

/// What a request answered: its status, the acting endpoint and the body
/// text.
type Answered = (StatusCode, Option<String>, String);

/// One log line, read for the fields these tests assert on.
#[derive(Debug, Deserialize)]
struct Line {
    target: String,
    level: String,
    message: Option<String>,
    kind: Option<String>,
    claimants: Option<String>,
    endpoint: Option<String>,
    holders: Option<String>,
    detection: Option<String>,
}

/// Every line of `text`.
fn lines(text: &str) -> Result<Vec<Line>, serde_json::Error> {
    text.lines().map(serde_json::from_str).collect()
}

/// The kind of every integrity incident in `text`, in order.
fn incidents(text: &str) -> Result<Vec<String>, serde_json::Error> {
    Ok(lines(text)?
        .into_iter()
        .filter(|line| line.target == TARGET)
        .filter_map(|line| line.kind)
        .collect())
}

/// `PUT {base}/v1/ehr/{ehr_id}` naming `endpoint` as its node.
fn create(ehr_id: &str, endpoint: &str) -> Result<Request<Body>, http::Error> {
    Request::put(format!("/v1/ehr/{ehr_id}"))
        .header("openEHR-federation-endpoint", endpoint)
        .body(Body::empty())
}

/// A read of `ehr_id` naming `endpoint`, which teaches the index its node on
/// success.
fn read_at(ehr_id: &str, endpoint: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/ehr/{ehr_id}"))
        .header("openEHR-federation-endpoint", endpoint)
        .body(Body::empty())
}

/// Sends `requests` through `app` one after another under a capturing JSON
/// subscriber, and returns each answer and everything logged.
async fn captured(
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

/// Asserts that `answered` is the `ehr-id-held` refusal naming `holder` and
/// `at`, quoting no `ehr_id`, with no endpoint acting.
fn refused_held_at(answered: &Answered, holder: &str, at: &str, ehr_id: &str) -> TestResult {
    let (status, acting, text) = answered;
    assert_eq!(
        (StatusCode::CONFLICT, "ehr-id-held".to_owned()),
        (*status, error_body(text)?.code),
        "§12.4, §12.5.2; ITS-REST 1.1.0 ehr_create_with_id: {text}"
    );
    assert!(
        text.contains(&format!("[{holder}]")),
        "the holder is named: {text}"
    );
    assert!(
        text.contains(&format!("endpoint {at}")),
        "the target is named: {text}"
    );
    assert!(!text.contains(ehr_id), "no request value is quoted: {text}");
    assert!(acting.is_none(), "no endpoint acted: {text}");
    Ok(())
}

// conformance: CP-15 CP-33
#[tokio::test]
async fn a_create_at_b_of_an_ehr_id_the_index_holds_at_a_is_refused_and_sends_nothing() -> TestResult
{
    let a = holder().await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let (answers, logs) = captured(
        &app,
        vec![read_at(EHR_A, ENDPOINT_A)?, create(EHR_A, ENDPOINT_B)?],
    )
    .await?;
    let mut answers = answers.into_iter();
    let (read, _, text) = answers.next().ok_or("the read answered")?;
    assert_eq!(StatusCode::OK, read, "the index now holds node A: {text}");
    refused_held_at(
        &answers.next().ok_or("the create answered")?,
        ENDPOINT_A,
        ENDPOINT_B,
        EHR_A,
    )?;
    assert!(asked(&b).await?.is_empty(), "node B is sent nothing");
    assert_eq!(
        vec![("GET".to_owned(), format!("/v1/ehr/{EHR_A}"))],
        asked(&a).await?,
        "node A is sent only the read"
    );
    assert!(
        incidents(&logs)?.is_empty(),
        "a refused create makes no collision, so no incident is raised (N42): {logs}"
    );
    let warned: Vec<Line> = lines(&logs)?
        .into_iter()
        .filter(|line| line.level == "WARN" && line.holders.is_some())
        .collect();
    let line = warned.first().ok_or("the refusal is logged")?;
    assert_eq!(
        (
            Some(ENDPOINT_B),
            Some("[node-a-pub]"),
            Some("index"),
            Some("a new EHR was refused: another member holds its ehr_id"),
        ),
        (
            line.endpoint.as_deref(),
            line.holders.as_deref(),
            line.detection.as_deref(),
            line.message.as_deref(),
        )
    );
    assert!(!logs.contains(EHR_A), "the log names ids only: {logs}");
    Ok(())
}

// conformance: CP-15 CP-33
#[test]
fn a_binding_placing_the_ehr_id_at_a_refuses_a_create_at_b() -> TestResult {
    let snapshot = RegistrySnapshot::from_toml_str(&registry(
        "http://127.0.0.1:9/a",
        "http://127.0.0.1:9/b",
        "",
    ))?;
    let ehr_id: EhrId = EHR_A.parse()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&"node-a".parse()?, &ehr_id)]);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let index = EhrIndex::new(NonZeroUsize::MIN);
    let endpoint = |id: &str| -> Result<_, Box<dyn Error>> {
        snapshot
            .endpoint(&id.parse::<EndpointId>()?)
            .ok_or_else(|| format!("{id} is in the registry").into())
    };
    let refused = owner::held_elsewhere(
        &snapshot,
        Some(held),
        &index,
        &ehr_id,
        endpoint(ENDPOINT_B)?,
    );
    assert_eq!(
        Some(HeldElsewhere {
            holders: vec![ENDPOINT_A.parse()?],
            detection: Detection::Binding,
            at: ENDPOINT_B.parse()?,
        }),
        refused,
        "a binding is step 2 of §12.5.1, read before the index"
    );
    let text = refused.ok_or("refused")?.to_string();
    assert!(!text.contains(EHR_A), "{text}");
    assert_eq!(
        None,
        owner::held_elsewhere(
            &snapshot,
            Some(held),
            &index,
            &ehr_id,
            endpoint(ENDPOINT_A)?
        ),
        "the member the binding names takes the create, and answers its own 409"
    );
    Ok(())
}

// conformance: CP-15
#[tokio::test]
async fn a_create_at_the_member_holding_the_ehr_id_is_forwarded_for_its_own_409() -> TestResult {
    let a = holder().await;
    mount(
        &a,
        "PUT",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(409),
    )
    .await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let (answers, logs) = captured(
        &app,
        vec![read_at(EHR_A, ENDPOINT_A)?, create(EHR_A, ENDPOINT_A)?],
    )
    .await?;
    let (status, acting, text) = answers.into_iter().nth(1).ok_or("the create answered")?;
    assert_eq!(
        StatusCode::CONFLICT,
        status,
        "node A's own ITS-REST 409 passes through: {text}"
    );
    assert_eq!(Some(ENDPOINT_A), acting.as_deref(), "N31");
    assert!(
        text.is_empty(),
        "the node's body, not the gateway's: {text}"
    );
    assert_eq!(
        vec![
            ("GET".to_owned(), format!("/v1/ehr/{EHR_A}")),
            ("PUT".to_owned(), format!("/v1/ehr/{EHR_A}")),
        ],
        asked(&a).await?
    );
    assert!(asked(&b).await?.is_empty());
    assert!(incidents(&logs)?.is_empty(), "{logs}");
    Ok(())
}

// conformance: CP-15 CP-33
#[tokio::test]
async fn an_unknown_ehr_id_is_created_where_named_and_then_indexed_there() -> TestResult {
    let a = Server::start().await;
    let b = Server::start().await;
    mount(
        &b,
        "PUT",
        format!("/v1/ehr/{EHR_B}"),
        ResponseTemplate::new(201),
    )
    .await;
    mount(
        &b,
        "GET",
        format!("/v1/ehr/{EHR_B}"),
        ResponseTemplate::new(200),
    )
    .await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let (answers, _) = captured(
        &app,
        vec![
            create(EHR_B, ENDPOINT_B)?,
            create(EHR_B, ENDPOINT_A)?,
            Request::get(format!("/v1/ehr/{EHR_B}")).body(Body::empty())?,
        ],
    )
    .await?;
    let mut answers = answers.into_iter();
    let (created, acting, text) = answers.next().ok_or("the create answered")?;
    assert_eq!(StatusCode::CREATED, created, "{text}");
    assert_eq!(Some(ENDPOINT_B), acting.as_deref());
    refused_held_at(
        &answers.next().ok_or("the second create answered")?,
        ENDPOINT_B,
        ENDPOINT_A,
        EHR_B,
    )?;
    let (read, acting, text) = answers.next().ok_or("the read answered")?;
    assert_eq!(
        (StatusCode::OK, Some(ENDPOINT_B)),
        (read, acting.as_deref()),
        "the index routes the untargeted read (§12.5.1 step 3): {text}"
    );
    assert!(
        asked(&a).await?.is_empty(),
        "node A is never sent the create, and the read is never probed"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn two_creates_racing_for_one_ehr_id_at_two_members_raise_the_index_insert_alarm()
-> TestResult {
    let a = Server::start().await;
    let b = Server::start().await;
    for server in [&a, &b] {
        mount(
            server,
            "PUT",
            format!("/v1/ehr/{EHR_B}"),
            ResponseTemplate::new(201).set_delay(Duration::from_millis(300)),
        )
        .await;
    }
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let (at_a, at_b) = tokio::join!(
        answer(app.clone(), create(EHR_B, ENDPOINT_A)?),
        answer(app.clone(), create(EHR_B, ENDPOINT_B)?),
    );
    let after = answer(
        app,
        Request::get(format!("/v1/ehr/{EHR_B}")).body(Body::empty())?,
    )
    .await?;
    drop(guard);
    let text = logs.text();
    assert_eq!(
        (StatusCode::CREATED, StatusCode::CREATED),
        (at_a?.0, at_b?.0),
        "both creates passed the check before either landed"
    );
    let alarms: Vec<Line> = lines(&text)?
        .into_iter()
        .filter(|line| line.target == TARGET)
        .collect();
    let kinds: Vec<&str> = alarms
        .iter()
        .filter_map(|line| line.kind.as_deref())
        .collect();
    assert_eq!(
        vec!["IndexInsertCollision", "EhrIdCollision"],
        kinds,
        "the second insert raises the alarm once (§12b.2), and the later read is refused (N42): {text}"
    );
    assert_eq!(
        Some("[node-a, node-b]"),
        alarms.first().and_then(|line| line.claimants.as_deref())
    );
    assert_eq!(StatusCode::CONFLICT, after.0, "{}", after.2);
    Ok(())
}
