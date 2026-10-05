// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway half of track 11 against two FerroEHR nodes, behind the
//! `FERROFED_E2E` gate (§16.3 track 11; §12.5.1, §12.5.2; N41, N42; CP-33).
//!
//! The harness seeds one `ehr_id` at both nodes, for two different synthetic
//! patients: a fixture that does not represent a conformant node state,
//! which §12b.2 forbids a member to reach on its own and §16.3 makes the
//! harness operator's to create. A request no step of §12.5.1 can route to
//! one owner is refused `409` listing both claimants, with one integrity
//! incident, and no row is served and no write applied. A versioned write
//! no earlier step routes is refused `400` and never probed for. Every
//! assertion on what a node received reads the journal of the capturing
//! proxy in front of it, never the gateway's logs (§16.3).
//!
//! The refusal of a versioned write no step routes is the conformance run's
//! own check ([`ferrofed_server::conformance::scenarios::track11`]). The
//! admission half of the track scores CP-33a, an Operator point, so it
//! runs in the harness crate, where the operator's points are scored.

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_server::conformance::scenarios::track11;
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_testkit::containers::{self, API_PATH, ProxiedNode, TwoNodes};
use ferrofed_testkit::seed::{
    self, CompositionSeed, DemoComposition, EhrSeed, PatientId, SeedPlan, SeedReport,
};
use http::{Method, Request, StatusCode, header};
use uuid::Uuid;

use crate::e2e::scenario::in_process;
use crate::e2e::{TestResult, gateway_mounted, query};
use crate::ehr_id_collision::{Answered, Logged, captured, incidents};
use crate::support::{Logs, error_body};

/// The `ehr_id` both nodes hold, each for a patient of its own.
const DUPLICATE: Uuid = Uuid::from_u128(0x5555_5555_5555_4555_8555_5555_5555_5555);

/// An `ehr_id` node A alone holds.
const ONLY_AT_A: Uuid = Uuid::from_u128(0x6666_6666_6666_4666_8666_6666_6666_6666);

/// The patient node A holds under [`DUPLICATE`].
const PATIENT_AT_A: PatientId = PatientId::new(1, 111);

/// The patient node B holds under [`DUPLICATE`].
const PATIENT_AT_B: PatientId = PatientId::new(1, 112);

/// The patient node A holds under [`ONLY_AT_A`].
const PATIENT_ONLY_AT_A: PatientId = PatientId::new(1, 113);

/// The claimants as the `409` message and the incident list them.
const BOTH: &str = "[node-a-pub, node-b-pub]";

/// The two nodes holding the seeded duplicate, and the version of the
/// composition each `ehr_id` holds at node A.
struct Seeded {
    nodes: TwoNodes,
    duplicate_at_a: String,
    only_at_a: String,
}

/// Seeds [`DUPLICATE`] at both nodes and [`ONLY_AT_A`] at node A, each EHR
/// with one composition, and clears both journals.
async fn seeded() -> Result<Seeded, Box<dyn Error>> {
    let nodes = containers::two_nodes().await?;
    let at_a = SeedPlan {
        ehrs: vec![
            EhrSeed {
                ehr_id: DUPLICATE,
                subject: Some(PATIENT_AT_A),
            },
            EhrSeed {
                ehr_id: ONLY_AT_A,
                subject: Some(PATIENT_ONLY_AT_A),
            },
        ],
        template: true,
        compositions: vec![
            CompositionSeed {
                ehr_id: DUPLICATE,
                composition: DemoComposition::FirstHospital,
            },
            CompositionSeed {
                ehr_id: ONLY_AT_A,
                composition: DemoComposition::SecondHospital,
            },
        ],
    };
    let at_b = SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id: DUPLICATE,
            subject: Some(PATIENT_AT_B),
        }],
        template: true,
        compositions: vec![CompositionSeed {
            ehr_id: DUPLICATE,
            composition: DemoComposition::FirstClinic,
        }],
    };
    let report = seed::seed(&nodes.a.api_root(), &at_a).await?;
    seed::seed(&nodes.b.api_root(), &at_b).await?;
    let duplicate_at_a = version_in(&report, DUPLICATE)?;
    let only_at_a = version_in(&report, ONLY_AT_A)?;
    clear(&nodes);
    Ok(Seeded {
        nodes,
        duplicate_at_a,
        only_at_a,
    })
}

/// The version uid of the composition `report` committed in `ehr_id`, as
/// the node's `ETag` names it.
fn version_in(report: &SeedReport, ehr_id: Uuid) -> Result<String, Box<dyn Error>> {
    let committed = report
        .compositions
        .iter()
        .find(|committed| committed.ehr_id == ehr_id)
        .and_then(|committed| committed.version_uid.as_deref())
        .ok_or("the node names the committed version in its ETag")?;
    Ok(committed
        .trim_start_matches("W/")
        .trim_matches('"')
        .to_owned())
}

/// The versioned object a version uid is a version of.
fn object_of(version: &str) -> Result<&str, Box<dyn Error>> {
    Ok(version.split("::").next().ok_or("an object id")?)
}

/// Clears the journal of both nodes.
fn clear(nodes: &TwoNodes) {
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
}

/// The method and the path of every request `node` received since its
/// journal was last cleared.
fn steps(node: &ProxiedNode) -> Vec<(String, String)> {
    node.proxy
        .journal()
        .into_iter()
        .map(|capture| (capture.method, capture.path))
        .collect()
}

/// The ask-all probe of §12.5.1 step 4 for `ehr_id`, as a node receives it.
fn probe_of(ehr_id: Uuid) -> (String, String) {
    ("GET".to_owned(), format!("{API_PATH}/v1/ehr/{ehr_id}"))
}

/// A fresh gateway over both nodes, mounted at the root, with no
/// cross-reference: nothing it holds names an owner before a request does.
fn fresh_gateway(dir: &std::path::Path, nodes: &TwoNodes) -> Result<Router, Box<dyn Error>> {
    gateway_mounted(dir, (&nodes.a, &nodes.b), "", "/")
}

/// The query scoped to [`DUPLICATE`] in the `WHERE` form of N29.
fn scoped_to_the_duplicate() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{DUPLICATE}'"
    )
}

/// The vendored hospital composition, the body of a write.
fn composition() -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(
        DemoComposition::FirstHospital.path(),
    )?)
}

/// A write of `verb` to `uri` carrying `body`, naming `preceding` in
/// `If-Match` when given, and no target.
fn write(
    verb: Method,
    uri: &str,
    preceding: Option<&str>,
    body: String,
) -> Result<Request<Body>, http::Error> {
    let mut request = Request::builder()
        .method(verb)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(preceding) = preceding {
        request = request.header(header::IF_MATCH, format!("\"{preceding}\""));
    }
    request.body(Body::from(body))
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
    assert!(
        !text.contains(&DUPLICATE.to_string()),
        "the message quotes no path: {text}"
    );
    assert!(acting.is_none(), "no endpoint acted: {text}");
    Ok(())
}

/// The kind and the detecting step of each incident in `found`.
fn kinds(found: &[Logged]) -> Vec<(&str, Option<&str>)> {
    found
        .iter()
        .map(|incident| (incident.kind.as_str(), incident.detection.as_deref()))
        .collect()
}

// conformance: CP-33 track-11
#[tokio::test]
async fn a_read_and_a_query_of_the_duplicate_are_refused_and_only_the_probe_reaches_a_node()
-> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let seeded = seeded().await?;
    let nodes = &seeded.nodes;
    let dir = tempfile::tempdir()?;
    let read = Request::get(format!(
        "/v1/ehr/{DUPLICATE}/composition/{}",
        seeded.duplicate_at_a
    ))
    .body(Body::empty())?;
    let scoped = query(&scoped_to_the_duplicate())?;
    for (what, request) in [("the read", read), ("the scoped query", scoped)] {
        // NOTE: §12.5.1 step 4: a fresh gateway holds no index entry, so the
        // ask-all probe is the step that finds both claimants.
        let app = fresh_gateway(dir.path(), nodes)?;
        clear(nodes);
        let (answers, logs) = captured(&app, vec![request]).await?;
        let answered = answers.first().ok_or("one answer")?;
        refused_naming_both(answered)?;
        let found = incidents(&logs)?;
        assert_eq!(
            vec![("EhrIdCollision", Some("ask-all"))],
            kinds(&found),
            "{what}: one incident for one refusal (N42): {logs}"
        );
        let incident = found.first().ok_or("one incident")?;
        assert_eq!(
            ("ERROR", BOTH),
            (incident.level.as_str(), incident.claimants.as_str())
        );
        for node in [&nodes.a, &nodes.b] {
            assert_eq!(
                vec![probe_of(DUPLICATE)],
                steps(node),
                "{what}: {} is asked whether it holds the ehr_id and nothing more: no row is served (§12.5.2)",
                node.node.system_id()
            );
        }
    }
    Ok(())
}

// conformance: CP-33 track-11
#[tokio::test]
async fn once_the_index_holds_both_claimants_no_read_or_write_reaches_either_node() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let seeded = seeded().await?;
    let nodes = &seeded.nodes;
    let dir = tempfile::tempdir()?;
    let app = fresh_gateway(dir.path(), nodes)?;
    let targeted = |endpoint: &str| {
        Request::get(format!("/v1/ehr/{DUPLICATE}"))
            .header("openEHR-federation-endpoint", endpoint)
            .body(Body::empty())
    };
    let (answers, logs) =
        captured(&app, vec![targeted("node-a-pub")?, targeted("node-b-pub")?]).await?;
    let statuses: Vec<StatusCode> = answers.iter().map(|(status, _, _)| *status).collect();
    assert_eq!(
        vec![StatusCode::OK, StatusCode::OK],
        statuses,
        "an explicit target routes each read to its node (§12.5.1 step 1)"
    );
    let found = incidents(&logs)?;
    assert_eq!(
        vec![("IndexInsertCollision", None)],
        kinds(&found),
        "the second member the index learns raises the index-insert alarm once (§12b.2): {logs}"
    );
    let alarm = found.first().ok_or("the alarm")?;
    assert_eq!("[node-a, node-b]", alarm.claimants, "the claiming members");

    clear(nodes);
    let version = seeded.duplicate_at_a.as_str();
    let requests = vec![
        Request::get(format!("/v1/ehr/{DUPLICATE}/composition/{version}")).body(Body::empty())?,
        write(
            Method::POST,
            &format!("/v1/ehr/{DUPLICATE}/composition"),
            None,
            composition()?,
        )?,
        write(
            Method::PUT,
            &format!("/v1/ehr/{DUPLICATE}/composition/{}", object_of(version)?),
            Some(version),
            composition()?,
        )?,
    ];
    let (answers, logs) = captured(&app, requests).await?;
    assert_eq!(3, answers.len(), "a read and two writes answered");
    for answered in &answers {
        refused_naming_both(answered)?;
    }
    assert_eq!(
        vec![("EhrIdCollision", Some("index")); 3],
        kinds(&incidents(&logs)?),
        "one incident per refusal, found at §12.5.1 step 3 (N42): {logs}"
    );
    for node in [&nodes.a, &nodes.b] {
        assert!(
            node.proxy.journal().is_empty(),
            "{} received nothing: no row served, no write applied, nothing probed (§12.5.2): {:?}",
            node.node.system_id(),
            steps(node)
        );
    }
    Ok(())
}

// conformance: CP-33 track-11
#[tokio::test]
async fn a_versioned_write_no_earlier_step_routes_is_refused_and_no_node_is_asked() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let seeded = seeded().await?;
    let nodes = &seeded.nodes;
    let dir = tempfile::tempdir()?;
    let app = fresh_gateway(dir.path(), nodes)?;
    let gateway = in_process(&app);
    let captured_logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, captured_logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let mut answers = Vec::new();
    for (ehr_id, version) in [
        (DUPLICATE, seeded.duplicate_at_a.as_str()),
        (ONLY_AT_A, seeded.only_at_a.as_str()),
    ] {
        // NOTE: §12.4, §12.5.1: the preceding version names node A's
        // system_id, and the path ehr_id still routes first, so no owner is guessed.
        answers.extend(
            track11::versioned_writes_unrouted(
                &gateway,
                &ehr_id.to_string(),
                version,
                &composition()?,
            )
            .await?,
        );
    }
    drop(guard);
    let logs = captured_logs.text();
    assert_eq!(4, answers.len(), "four writes answered");
    for answer in &answers {
        assert_eq!(
            "target-required",
            error_body(&answer.text)?.code,
            "§12.5.1, N41: the ITS-REST Error carries the code and the request id alone"
        );
    }
    assert!(
        incidents(&logs)?.is_empty(),
        "no step found a claimant, so no incident is raised: {logs}"
    );
    for node in [&nodes.a, &nodes.b] {
        assert!(
            node.proxy.journal().is_empty(),
            "{} was never asked, not even probed (N41): {:?}",
            node.node.system_id(),
            steps(node)
        );
    }
    Ok(())
}
