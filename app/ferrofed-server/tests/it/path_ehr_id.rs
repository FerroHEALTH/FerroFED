// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Routing a path `ehr_id` in the order of §12.5.1 (N41, CP-33): the
//! targeting headers, a resolution binding the client session holds, the
//! `ehr_id` index, and for a read only, the ask-all probe of every member.
//!
//! The order itself is read through [`owner::located`], where a test can hold
//! a session's bindings; the probe, the index learning and the refusals are
//! driven over HTTP against two mock nodes, and every assertion on what a
//! node received reads the node's own capture (§16, track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use ferrofed_identity::binding::{ResolutionBindings, SessionKey};
use ferrofed_registry::ehr_index::EhrIndex;
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::incident::Detection;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_server::config::{Config, error};
use ferrofed_server::facade::owner::{self, Held, Located, Step};
use http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::facade::{
    EHR_A, PATIENT, body, dev_gateway, gateway, patient_query, post, registry, wire,
};
use crate::support::{error_body, send};

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// The endpoints of node A and node B in [`registry`].
pub(crate) const ENDPOINT_A: &str = "node-a-pub";
const ENDPOINT_B: &str = "node-b-pub";

/// A version uid node A minted.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The client's own credential, which no node ever sees.
const CLIENT_TOKEN: &str = "synthetic-client-token";

/// The registry of node A and node B, at addresses nothing listens on.
fn snapshot() -> Result<RegistrySnapshot, Box<dyn Error>> {
    Ok(RegistrySnapshot::from_toml_str(&registry(
        "http://127.0.0.1:9/a",
        "http://127.0.0.1:9/b",
        "",
    ))?)
}

fn ehr() -> Result<EhrId, Box<dyn Error>> {
    Ok(EHR_A.parse()?)
}

fn node(id: &str) -> Result<NodeId, Box<dyn Error>> {
    Ok(id.parse()?)
}

/// Headers naming `endpoint` as the explicit target.
fn targeting(endpoint: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "openEHR-federation-endpoint",
        HeaderValue::from_static(endpoint),
    );
    headers
}

/// The endpoint and the step `located` names, or `None` for no owner.
fn named(located: &Located<'_>) -> Option<(String, Step)> {
    match located {
        Located::At { endpoint, step } => Some((endpoint.id().as_str().to_owned(), *step)),
        Located::Unreachable { .. } | Located::Collision(_) | Located::Unknown => None,
    }
}

/// The claiming endpoints and the step `located` names, or `None` for no
/// collision.
pub(crate) fn collision(located: &Located<'_>) -> Option<(Vec<String>, Detection)> {
    match located {
        Located::Collision(claimed) => Some((
            claimed
                .claimants
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
            claimed.detection,
        )),
        Located::At { .. } | Located::Unreachable { .. } | Located::Unknown => None,
    }
}

// conformance: CP-33
#[test]
fn the_explicit_target_wins_over_a_binding_and_the_index() -> TestResult {
    let snapshot = snapshot()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-b")?, &ehr()?)]);
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-b")?);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(
        &snapshot,
        &targeting(ENDPOINT_A),
        Some(held),
        &index,
        &ehr()?,
    )?;
    assert_eq!(
        Some((ENDPOINT_A.to_owned(), Step::Target)),
        named(&located),
        "step 1 answers, so no later step is taken (§12.5.1, N41)"
    );
    Ok(())
}

// conformance: CP-33
#[test]
fn a_held_binding_wins_over_the_index() -> TestResult {
    let snapshot = snapshot()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-a")?, &ehr()?)]);
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-b")?);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(&snapshot, &HeaderMap::new(), Some(held), &index, &ehr()?)?;
    assert_eq!(
        Some((ENDPOINT_A.to_owned(), Step::Binding)),
        named(&located),
        "step 2 answers before step 3 (§12.5.1, N41)"
    );
    Ok(())
}

// conformance: CP-33
#[test]
fn the_index_answers_when_no_target_and_no_binding_does() -> TestResult {
    let snapshot = snapshot()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let now = Instant::now();
    bindings.record(
        &SessionKey::new("session-2"),
        now,
        [(&node("node-a")?, &ehr()?)],
    );
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-b")?);
    let session = SessionKey::new("session-1");
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(&snapshot, &HeaderMap::new(), Some(held), &index, &ehr()?)?;
    assert_eq!(
        Some((ENDPOINT_B.to_owned(), Step::Index)),
        named(&located),
        "another session's binding is never routed on (§12.5.1 step 2), so step 3 answers"
    );
    Ok(())
}

// conformance: CP-33
#[test]
fn a_step_naming_two_members_names_no_owner_and_no_later_step_picks_one() -> TestResult {
    let snapshot = snapshot()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(
        &session,
        now,
        [(&node("node-a")?, &ehr()?), (&node("node-b")?, &ehr()?)],
    );
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-a")?);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(&snapshot, &HeaderMap::new(), Some(held), &index, &ehr()?)?;
    assert_eq!(
        Some((
            vec![ENDPOINT_A.to_owned(), ENDPOINT_B.to_owned()],
            Detection::Binding
        )),
        collision(&located),
        "two bound members are a collision, and the index never picks one of them (§12.5.2, N42)"
    );
    assert!(named(&located).is_none());
    index.learn(&ehr()?, &node("node-a")?);
    index.learn(&ehr()?, &node("node-b")?);
    let located = owner::located(&snapshot, &HeaderMap::new(), None, &index, &ehr()?)?;
    assert_eq!(
        Some((
            vec![ENDPOINT_A.to_owned(), ENDPOINT_B.to_owned()],
            Detection::Index
        )),
        collision(&located),
        "two indexed members are never narrowed to one (§12.5.2, N42)"
    );
    assert!(named(&located).is_none());
    Ok(())
}

#[test]
fn a_member_the_registry_no_longer_holds_names_nothing() -> TestResult {
    let snapshot = snapshot()?;
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-gone")?);
    let located = owner::located(&snapshot, &HeaderMap::new(), None, &index, &ehr()?)?;
    assert!(named(&located).is_none());
    Ok(())
}

/// A node answering `verb` at `at` with `answer`, and `404` to the rest.
pub(crate) async fn mount(server: &MockServer, verb: &str, at: String, answer: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(answer)
        .mount(server)
        .await;
}

/// A node that holds the EHR of [`EHR_A`] and the composition of
/// [`VERSION_A`] in it.
pub(crate) async fn holder() -> MockServer {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"ehr_id":{{"value":"{EHR_A}"}}}}"#).into_bytes(),
            "application/json",
        ),
    )
    .await;
    mount(
        &server,
        "GET",
        format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ResponseTemplate::new(200)
            .set_body_raw(br#"{"_type":"COMPOSITION"}"#.to_vec(), "application/json"),
    )
    .await;
    mount(
        &server,
        "POST",
        format!("/v1/ehr/{EHR_A}/composition"),
        ResponseTemplate::new(201),
    )
    .await;
    server
}

/// A node that holds no EHR at all: it answers `404` to everything.
pub(crate) async fn stranger() -> MockServer {
    MockServer::start().await
}

/// The gateway over node A at `a` and node B at `b`.
pub(crate) fn over(
    dir: &std::path::Path,
    a: &MockServer,
    b: &MockServer,
) -> Result<Router, Box<dyn Error>> {
    gateway(dir, &registry(&a.uri(), &b.uri(), ""), "", "")
}

/// The method and path of every request `server` received, in order.
pub(crate) async fn asked(server: &MockServer) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .into_iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect())
}

pub(crate) fn probe_at() -> (String, String) {
    ("GET".to_owned(), format!("/v1/ehr/{EHR_A}"))
}

/// The status, the acting endpoint and the body text of `request` sent to
/// `app`.
pub(crate) async fn answer(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, Option<String>, String), Box<dyn Error>> {
    let response = send(app, request).await?;
    let status = response.status();
    let acting = response
        .headers()
        .get("openEHR-federation-endpoint")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, acting, String::from_utf8(bytes.to_vec())?))
}

// conformance: CP-33
#[tokio::test]
async fn a_read_no_earlier_step_routes_goes_to_the_one_member_the_probe_finds() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (status, acting, text) = answer(
        over(dir.path(), &a, &b)?,
        Request::get(&resource).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_A),
        acting.as_deref(),
        "the acting endpoint (N31)"
    );
    assert_eq!(r#"{"_type":"COMPOSITION"}"#, text, "the owner's answer");
    assert_eq!(
        vec![probe_at(), ("GET".to_owned(), resource)],
        asked(&a).await?,
        "the owner is probed, then sent the read once"
    );
    assert_eq!(
        vec![probe_at()],
        asked(&b).await?,
        "the other member is only probed (§12.5.1 step 4)"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_read_of_the_ehr_itself_is_answered_by_the_owners_probe_answer() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let (status, acting, text) = answer(
        over(dir.path(), &a, &b)?,
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(ENDPOINT_A), acting.as_deref());
    assert!(text.contains(EHR_A), "the owner's EHR: {text}");
    assert_eq!(vec![probe_at()], asked(&a).await?, "asked once");
    assert_eq!(vec![probe_at()], asked(&b).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn the_probe_teaches_the_index_and_a_later_write_is_routed_by_it_unprobed() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let (read, _, _) = answer(
        app.clone(),
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, read);
    let write = Request::post(format!("/v1/ehr/{EHR_A}/composition"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"_type":"COMPOSITION"}"#))?;
    let (status, acting, text) = answer(app, write).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_A),
        acting.as_deref(),
        "the index names the owner (§12.5.1 step 3)"
    );
    assert_eq!(
        vec![
            probe_at(),
            ("POST".to_owned(), format!("/v1/ehr/{EHR_A}/composition"))
        ],
        asked(&a).await?
    );
    assert_eq!(
        vec![probe_at()],
        asked(&b).await?,
        "the write probes nobody (N41)"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn the_index_never_overrides_an_explicit_target() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let (read, _, _) = answer(
        app.clone(),
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, read);
    let named = Request::get(format!("/v1/ehr/{EHR_A}"))
        .header("openEHR-federation-endpoint", ENDPOINT_B)
        .body(Body::empty())?;
    let (status, acting, _) = answer(app, named).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "node B's own 404 (§11.2)");
    assert_eq!(Some(ENDPOINT_B), acting.as_deref(), "step 1 wins (N41)");
    assert_eq!(
        vec![probe_at()],
        asked(&a).await?,
        "node A is asked only by the probe"
    );
    assert_eq!(vec![probe_at(), probe_at()], asked(&b).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_resolution_teaches_the_index_and_a_follow_up_goes_to_the_resolving_member() -> TestResult
{
    let a = MockServer::start().await;
    mount(
        &a,
        "POST",
        "/v1/query/aql".to_owned(),
        ResponseTemplate::new(200).set_body_raw(
            br##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##.to_vec(),
            "application/json",
        ),
    )
    .await;
    mount(
        &a,
        "GET",
        format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ResponseTemplate::new(200),
    )
    .await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;
    let (queried, _, text) = answer(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, queried, "{text}");
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (status, acting, text) = answer(app, Request::get(&resource).body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(ENDPOINT_A), acting.as_deref());
    assert_eq!(
        vec![
            ("POST".to_owned(), "/v1/query/aql".to_owned()),
            ("GET".to_owned(), resource)
        ],
        asked(&a).await?,
        "the follow-up is routed by the index the resolution taught, unprobed"
    );
    assert!(asked(&b).await?.is_empty(), "node B is never asked");
    Ok(())
}

/// The status and the code `request` is refused with, against a holder at A
/// and `b`, and every request each node received.
async fn refused_with(
    request: Request<Body>,
    b: MockServer,
) -> Result<(StatusCode, String, String, MockServer, MockServer), Box<dyn Error>> {
    let a = holder().await;
    let dir = tempfile::tempdir()?;
    let (status, acting, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert!(acting.is_none(), "no endpoint acted: {text}");
    let code = error_body(&text)?.code;
    Ok((status, code, text, a, b))
}

// conformance: CP-33
#[tokio::test]
async fn a_read_no_member_holds_is_a_404_no_destination() -> TestResult {
    let a = stranger().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let (status, acting, text) = answer(
        over(dir.path(), &a, &b)?,
        Request::get(format!("/v1/ehr/{EHR_A}/ehr_status")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "§11.2");
    assert_eq!("no-destination", error_body(&text)?.code);
    assert!(acting.is_none());
    assert_eq!(
        vec![probe_at()],
        asked(&a).await?,
        "probed, never sent the read"
    );
    assert_eq!(vec![probe_at()], asked(&b).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn two_members_holding_the_ehr_id_are_a_409_naming_both_and_neither_is_read() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (status, code, text, a, b) =
        refused_with(Request::get(&resource).body(Body::empty())?, holder().await).await?;
    assert_eq!(
        (StatusCode::CONFLICT, "ehr-id-collision"),
        (status, code.as_str()),
        "§12.5.2, N42"
    );
    assert!(
        text.contains(ENDPOINT_A) && text.contains(ENDPOINT_B),
        "the claimants are listed: {text}"
    );
    assert!(!text.contains(EHR_A), "the message quotes no path: {text}");
    for server in [&a, &b] {
        assert_eq!(
            vec![probe_at()],
            asked(server).await?,
            "no claimant is read"
        );
    }
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_member_that_does_not_answer_the_probe_in_time_leaves_the_owner_unknown() -> TestResult {
    let slow = MockServer::start().await;
    mount(
        &slow,
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(404).set_delay(Duration::from_millis(2600)),
    )
    .await;
    let started = Instant::now();
    let (status, code, text, a, _) = refused_with(
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
        slow,
    )
    .await?;
    let elapsed = started.elapsed();
    assert_eq!(
        (StatusCode::GATEWAY_TIMEOUT, "node-timeout"),
        (status, code.as_str()),
        "a time-out means unknown, never absent (§11.5, §11.2): {text}"
    );
    assert!(
        text.contains(ENDPOINT_B),
        "the silent member is named: {text}"
    );
    assert!(
        elapsed < Duration::from_millis(2600),
        "the member is abandoned at the per-node timeout of 2000 ms: {elapsed:?}"
    );
    assert_eq!(
        vec![probe_at()],
        asked(&a).await?,
        "the one claimant is not read"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_member_answering_the_probe_with_an_error_leaves_the_owner_unknown() -> TestResult {
    let failing = MockServer::start().await;
    mount(
        &failing,
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(503),
    )
    .await;
    let (status, code, text, a, _) = refused_with(
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
        failing,
    )
    .await?;
    assert_eq!(
        (StatusCode::FAILED_DEPENDENCY, "node-error"),
        (status, code.as_str()),
        "§11.2: {text}"
    );
    assert!(text.contains("node-b-pub (answered 503)"), "{text}");
    assert_eq!(vec![probe_at()], asked(&a).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_write_no_earlier_step_routes_is_a_400_and_probes_nobody() -> TestResult {
    let a = holder().await;
    let b = holder().await;
    let dir = tempfile::tempdir()?;
    for (verb, at) in [
        (Method::POST, format!("/v1/ehr/{EHR_A}/composition")),
        (
            Method::PUT,
            format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ),
        (
            Method::DELETE,
            format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ),
    ] {
        let request = Request::builder()
            .method(verb.clone())
            .uri(at)
            .body(Body::from(r#"{"_type":"COMPOSITION"}"#))?;
        let (status, acting, text) = answer(over(dir.path(), &a, &b)?, request).await?;
        assert_eq!(
            (StatusCode::BAD_REQUEST, "target-required".to_owned()),
            (status, error_body(&text)?.code),
            "{verb} (§12.5.1, N41)"
        );
        assert!(acting.is_none());
    }
    assert!(
        asked(&a).await?.is_empty(),
        "no probe is sent for a write (N41)"
    );
    assert!(
        asked(&b).await?.is_empty(),
        "no probe is sent for a write (N41)"
    );
    Ok(())
}

#[tokio::test]
async fn a_malformed_path_ehr_id_is_a_400_before_any_routing() -> TestResult {
    for segment in ["not%20an%20ehr_id", "%FF%FE"] {
        let a = holder().await;
        let b = holder().await;
        let dir = tempfile::tempdir()?;
        let request = Request::get(format!("/v1/ehr/{segment}/composition/{VERSION_A}"))
            .header("openEHR-federation-endpoint", ENDPOINT_A)
            .body(Body::empty())?;
        let (status, acting, text) = answer(over(dir.path(), &a, &b)?, request).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{segment}: §12.5: {text}");
        assert_eq!("ehr-id-invalid", error_body(&text)?.code, "{segment}");
        assert!(acting.is_none());
        assert!(
            asked(&a).await?.is_empty() && asked(&b).await?.is_empty(),
            "{segment}"
        );
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn neither_the_probe_nor_the_routed_read_carries_an_identifier_or_the_clients_credential()
-> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let mut request = Request::get(&resource).body(Body::empty())?;
    let fields = request.headers_mut();
    fields.insert(
        header::AUTHORIZATION,
        format!("Bearer {CLIENT_TOKEN}").parse()?,
    );
    fields.insert("x-patient", PATIENT.parse()?);
    fields.insert(header::COOKIE, format!("patient={PATIENT}").parse()?);
    fields.insert("x-request-id", format!("req-{PATIENT}").parse()?);
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for server in [&a, &b] {
        let captured = wire(server).await?;
        assert!(!captured.is_empty(), "the member was probed");
        assert!(
            !captured.contains(PATIENT),
            "no identifier reaches a member (N33): {captured:?}"
        );
        assert!(
            !captured.contains(CLIENT_TOKEN),
            "the client's credential reaches no member: {captured:?}"
        );
        assert!(
            !captured.contains_ignoring_ascii_case("openehr-federation"),
            "the federation's headers stay at the gateway: {captured:?}"
        );
    }
    Ok(())
}

#[test]
fn the_configured_index_capacity_is_the_one_resolved_and_zero_is_refused() -> TestResult {
    let text = "[federation]\nehr_index_capacity = 250\n";
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    assert_eq!(250, settings.federation.ehr_index_capacity.get());
    let default = Config::from_sources(Some(""), &BTreeMap::new())?.resolve()?;
    assert_eq!(100_000, default.federation.ehr_index_capacity.get());
    let zero = "[federation]\nehr_index_capacity = 0\n";
    match Config::from_sources(Some(zero), &BTreeMap::new())?.resolve() {
        Err(error::Error::Zero { key }) if key == "federation.ehr_index_capacity" => Ok(()),
        other => Err(format!("refused naming the key: {other:?}").into()),
    }
}
