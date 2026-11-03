// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The follow-up routing table every answer teaches (§12.2, N21, CP-13), and
//! follow-up reads of one version, located by their path `ehr_id` in the
//! order of §12.5.1 as 1.0.1 sets it (§12.3, §12a.1, N22, N41, CP-14).
//!
//! A read of a version under `{base}/v1/ehr/{ehr_id}/…` goes where the
//! explicit target, the binding or the index places the path `ehr_id`; the
//! version's `creating_system_id` is step 4, consulted only when those named
//! no member, and never while they named another. A read located at a
//! copy's holder returns the copy, with `openEHR-federation-system-id`
//! naming the holder (§7a.3). Every assertion on what a node received reads
//! the node's own capture (§16, track 6 and track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{EHR_A, EHR_B, PATIENT, body, gateway, post, registry, wire};
use crate::request_log::logged;
use crate::support::{error_body, send};

type TestResult = Result<(), Box<dyn Error>>;

const ENDPOINT_B: &str = "node-b-pub";

/// A version node A created.
const CREATED_AT_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";
/// A version of a system the registry does not map.
const CREATED_ELSEWHERE: &str = "5c3e9b1a-7d2f-4e8a-9b6c-1f0e2d3c4b5a::external.example.org::3";
/// An object of another unmapped system, by its versioned object uid.
const OBJECT_ELSEWHERE: &str = "9f8e7d6c-5b4a-4392-8170-6f5e4d3c2b1a";
/// The version of [`OBJECT_ELSEWHERE`] its holder answers with.
const LATEST_ELSEWHERE: &str = "9f8e7d6c-5b4a-4392-8170-6f5e4d3c2b1a::other.example.org::2";

/// The event a sighting that teaches the table a new route logs.
const LEARNED: &str = "learned a route for a creating_system_id";

/// The client's own credential, which no node ever sees.
use crate::support::CLIENT_TOKEN;

/// The gateway over node A and node B.
fn over(dir: &std::path::Path, a: &Server, b: &Server) -> Result<Router, Box<dyn Error>> {
    gateway(dir, &registry(&a.uri(), &b.uri(), ""), "", "")
}

/// The resource of `uid` under `ehr_id`.
fn composition(ehr_id: &str, uid: &str) -> String {
    format!("/v1/ehr/{ehr_id}/composition/{uid}")
}

/// The body a node answers a read of `uid` with.
fn read_body(uid: &str) -> String {
    format!(r#"{{"_type":"COMPOSITION","uid":{{"_type":"OBJECT_VERSION_ID","value":"{uid}"}}}}"#)
}

/// A node holding the EHR `ehr_id` with a version of each of `uids` in it,
/// and answering the federated query with one row per uid in `rows`.
async fn node(ehr_id: &str, uids: &[&str], rows: &[&str]) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{ehr_id}")))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    for uid in uids {
        Mock::given(method("GET"))
            .and(path(composition(ehr_id, uid)))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(header::ETAG.as_str(), format!("\"{uid}\""))
                    .set_body_raw(read_body(uid).into_bytes(), "application/json"),
            )
            .mount(&server)
            .await;
    }
    let cells: Vec<String> = rows.iter().map(|uid| format!(r#"["{uid}"]"#)).collect();
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}}],"rows":[{}]}}"##,
        cells.join(",")
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// A `GET` of `uri`, naming `endpoint` as the target when one is given.
fn read(uri: &str, endpoint: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::get(uri);
    if let Some(endpoint) = endpoint {
        request = request.header("openEHR-federation-endpoint", endpoint);
    }
    request.body(Body::empty())
}

/// The federated query that names no patient, asked of every member.
fn every_member() -> Result<Request<Body>, Box<dyn Error>> {
    Ok(post(body(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
    )?)?)
}

/// The status, the acting endpoint, the `ETag` and the body text of
/// `request` sent to `app`.
async fn answer(
    app: &Router,
    request: Request<Body>,
) -> Result<(StatusCode, Option<String>, Option<String>, String), Box<dyn Error>> {
    let response = send(app.clone(), request).await?;
    let status = response.status();
    let named = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let acting = named("openEHR-federation-endpoint");
    let tag = named("etag");
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, acting, tag, String::from_utf8(bytes.to_vec())?))
}

/// The method and path of every request `server` received, in order.
async fn asked(server: &Server) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .into_iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect())
}

fn get(at: String) -> (String, String) {
    ("GET".to_owned(), at)
}

fn query() -> (String, String) {
    ("POST".to_owned(), "/v1/query/aql".to_owned())
}

/// The log lines that name `needle`, at any level.
fn naming<'t>(text: &'t str, needle: &str) -> Vec<&'t str> {
    text.lines().filter(|line| line.contains(needle)).collect()
}

/// Whether one logged line is `message` naming every one of `fields`.
fn logged_with(text: &str, message: &str, fields: &[&str]) -> bool {
    naming(text, message)
        .iter()
        .any(|line| fields.iter().all(|field| line.contains(field)))
}

/// The `openEHR-federation-system-id` an answer of `app` to `request` names.
async fn system_id_of(app: &Router, request: Request<Body>) -> Result<String, Box<dyn Error>> {
    let response = send(app.clone(), request).await?;
    assert_eq!(StatusCode::OK, response.status(), "the read is answered");
    Ok(response
        .headers()
        .get("openEHR-federation-system-id")
        .ok_or("the answer names the acting system (§7a.3)")?
        .to_str()?
        .to_owned())
}

// conformance: CP-14 CP-33
#[tokio::test]
async fn an_imported_copy_is_read_where_the_index_places_the_path_ehr_id_never_at_its_creator()
-> TestResult {
    let a = node(EHR_A, &[CREATED_AT_A], &[]).await;
    let b = node(EHR_B, &[CREATED_AT_A], &[]).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let ehr = format!("/v1/ehr/{EHR_B}");
    let (status, acting, _, text) = answer(&app, read(&ehr, None)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_B),
        acting.as_deref(),
        "the probe teaches the index"
    );
    let resource = composition(EHR_B, CREATED_AT_A);
    let system_id = system_id_of(&app, read(&resource, None)?).await?;
    assert_eq!(
        "cdr-b.example.org", system_id,
        "the holder answers with its copy and names itself, which differs from the uid's \
         creating_system_id, so the client can tell a copy (§12.3, §7a.3, N22)"
    );
    let probe = |ehr_id: &str| get(format!("/v1/ehr/{ehr_id}"));
    assert_eq!(
        vec![probe(EHR_B)],
        asked(&a).await?,
        "step 3 named the holder, so the creator is never sent the path ehr_id \
         (§12.5.1 ehr-id-not-creating-system, N41)"
    );
    assert_eq!(
        Some(&get(resource)),
        asked(&b).await?.last(),
        "the holder is sent the read"
    );
    Ok(())
}

// conformance: CP-14 CP-33
#[tokio::test]
async fn with_no_target_binding_or_index_a_version_read_reaches_its_creator_by_step_4() -> TestResult
{
    let a = node(EHR_A, &[CREATED_AT_A], &[]).await;
    let b = node(EHR_B, &[], &[]).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let resource = composition(EHR_A, CREATED_AT_A);
    let (status, acting, tag, text) = answer(&app, read(&resource, None)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some("node-a-pub"),
        acting.as_deref(),
        "§12.5.1 step 4: the member whose system_id is the creating_system_id (N22, N41)"
    );
    assert_eq!(Some(format!("\"{CREATED_AT_A}\"")), tag, "N22");
    assert_eq!(vec![get(resource)], asked(&a).await?, "nothing is probed");
    assert!(
        asked(&b).await?.is_empty(),
        "step 4 named a member, so the ask-all probe of step 5 is never taken"
    );
    Ok(())
}

// conformance: CP-14 CP-33
#[tokio::test]
async fn a_version_read_whose_creator_the_registry_does_not_map_falls_to_the_probe() -> TestResult {
    let a = node(EHR_A, &[], &[]).await;
    let b = node(EHR_B, &[CREATED_ELSEWHERE], &[]).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let resource = composition(EHR_B, CREATED_ELSEWHERE);
    let (status, acting, _, text) = answer(&app, read(&resource, None)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_B),
        acting.as_deref(),
        "§12.5.1: step 4 names no member, so the read is probed for (step 5)"
    );
    let probe = |ehr_id: &str| get(format!("/v1/ehr/{ehr_id}"));
    assert_eq!(vec![probe(EHR_B)], asked(&a).await?);
    assert_eq!(vec![probe(EHR_B), get(resource)], asked(&b).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn an_imported_copy_is_read_at_the_endpoint_the_client_names() -> TestResult {
    let a = node(EHR_A, &[CREATED_AT_A], &[]).await;
    let b = node(EHR_B, &[CREATED_AT_A], &[]).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let resource = composition(EHR_B, CREATED_AT_A);
    let (status, acting, _, text) = answer(&app, read(&resource, Some(ENDPOINT_B))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(ENDPOINT_B), acting.as_deref(), "§12.5.1 step 1, N41");
    assert_eq!(vec![get(resource)], asked(&b).await?);
    assert!(asked(&a).await?.is_empty(), "the creator is not asked");
    Ok(())
}

// conformance: CP-13
#[test]
fn a_fan_out_answer_teaches_the_table_a_system_seen_at_one_node() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (a, b) = runtime.block_on(async {
        (
            node(EHR_A, &[], &[CREATED_AT_A]).await,
            node(EHR_A, &[], &[CREATED_ELSEWHERE]).await,
        )
    });
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let text = logged(&app, "debug", vec![every_member()?])?;
    assert!(
        logged_with(&text, LEARNED, &["external.example.org", ENDPOINT_B]),
        "the system seen only at node B is mapped there (§12.2, N21): {text}"
    );
    assert!(
        !logged_with(&text, LEARNED, &["cdr-a.example.org"]),
        "a member's own system_id is already mapped, so nothing is learned: {text}"
    );
    Ok(())
}

// conformance: CP-13
#[test]
fn a_routed_answer_teaches_the_table_the_version_its_etag_names() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (a, b) = runtime.block_on(async {
        let b = Server::start().await;
        Mock::given(method("GET"))
            .and(path(composition(EHR_A, OBJECT_ELSEWHERE)))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(header::ETAG.as_str(), format!("\"{LATEST_ELSEWHERE}\""))
                    .set_body_raw(read_body(LATEST_ELSEWHERE).into_bytes(), "application/json"),
            )
            .mount(&b)
            .await;
        (Server::start().await, b)
    });
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let text = logged(
        &app,
        "debug",
        vec![read(
            &composition(EHR_A, OBJECT_ELSEWHERE),
            Some(ENDPOINT_B),
        )?],
    )?;
    assert!(
        logged_with(&text, LEARNED, &["other.example.org", ENDPOINT_B]),
        "the ETag's version is seen at the endpoint that answered: {text}"
    );
    Ok(())
}

/// The version `version_of` reads from a request `method path` names.
fn named_version(method: &http::Method, path: &str) -> Option<String> {
    match openehr_its::rest::routes::lookup(method, path) {
        openehr_its::rest::routes::Lookup::Matched(matched) => {
            ferrofed_server::facade::follow_up::version_of(method, &matched)
                .map(|version| version.value().to_owned())
        }
        _ => None,
    }
}

// conformance: CP-13
#[test]
fn a_read_names_the_version_its_path_identifier_classes_carry() {
    let ehr = format!("/ehr/{EHR_A}");
    for at in [
        format!("{ehr}/composition/{CREATED_ELSEWHERE}"),
        format!("{ehr}/ehr_status/{CREATED_ELSEWHERE}"),
        format!("{ehr}/versioned_composition/{OBJECT_ELSEWHERE}/version/{CREATED_ELSEWHERE}"),
    ] {
        assert_eq!(
            Some(CREATED_ELSEWHERE.to_owned()),
            named_version(&http::Method::GET, &at),
            "{at}"
        );
    }
    for (method, at) in [
        (
            http::Method::GET,
            format!("{ehr}/composition/{OBJECT_ELSEWHERE}"),
        ),
        (
            http::Method::GET,
            format!("{ehr}/versioned_composition/{OBJECT_ELSEWHERE}"),
        ),
        (
            http::Method::DELETE,
            format!("{ehr}/composition/{CREATED_ELSEWHERE}"),
        ),
    ] {
        assert_eq!(None, named_version(&method, &at), "{method} {at}");
    }
}

// conformance: CP-13
#[test]
fn a_routed_read_teaches_the_table_the_version_it_named() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (a, b) = runtime.block_on(async {
        let b = Server::start().await;
        Mock::given(method("GET"))
            .and(path(composition(EHR_A, CREATED_ELSEWHERE)))
            .respond_with(ResponseTemplate::new(200))
            .mount(&b)
            .await;
        (Server::start().await, b)
    });
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let text = logged(
        &app,
        "debug",
        vec![read(
            &composition(EHR_A, CREATED_ELSEWHERE),
            Some(ENDPOINT_B),
        )?],
    )?;
    assert!(
        logged_with(&text, LEARNED, &["external.example.org", ENDPOINT_B]),
        "a node that answers a read of a version holds it: {text}"
    );
    Ok(())
}

// conformance: CP-13
#[test]
fn a_system_seen_at_two_nodes_is_an_integrity_incident() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (a, b) = runtime.block_on(async {
        (
            node(EHR_A, &[], &[CREATED_ELSEWHERE]).await,
            node(EHR_A, &[], &[CREATED_ELSEWHERE]).await,
        )
    });
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let text = logged(&app, "error", vec![every_member()?])?;
    assert!(
        logged_with(
            &text,
            "integrity incident",
            &["LearnedCreatingSystemConflict", "external.example.org"]
        ),
        "the conflict is an incident naming the creating_system_id: {text}"
    );
    assert!(!text.contains(PATIENT), "{text}");
    Ok(())
}

// conformance: CP-13
#[tokio::test]
async fn a_learned_route_never_routes_a_write() -> TestResult {
    let a = node(EHR_A, &[], &[]).await;
    let b = node(EHR_A, &[], &[CREATED_ELSEWHERE]).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let (status, _, _, text) = answer(&app, every_member()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let delete = Request::delete(composition(EHR_A, CREATED_ELSEWHERE)).body(Body::empty())?;
    let (status, acting, _, text) = answer(&app, delete).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!("target-required", error_body(&text)?.code);
    assert!(acting.is_none());
    assert_eq!(
        vec![query()],
        asked(&b).await?,
        "a learned holder is no write's controlling CDR (§10.3, §12.4)"
    );
    assert_eq!(vec![query()], asked(&a).await?);
    Ok(())
}

// conformance: CP-33 CP-26
#[tokio::test]
async fn a_version_read_reaches_the_node_byte_identical_and_carries_no_identifier() -> TestResult {
    let a = node(EHR_A, &[], &[]).await;
    let b = node(EHR_B, &[CREATED_AT_A], &[]).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let resource = composition(EHR_B, CREATED_AT_A);
    let mut request = read(&resource, Some(ENDPOINT_B))?;
    let fields = request.headers_mut();
    fields.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", *CLIENT_TOKEN).parse()?,
    );
    fields.insert("x-patient", PATIENT.parse()?);
    fields.insert("x-request-id", format!("req-{PATIENT}").parse()?);
    let (status, _, tag, text) = answer(&app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(format!("\"{CREATED_AT_A}\"")), tag, "N22");
    assert_eq!(read_body(CREATED_AT_A), text, "N22");
    let requests = b.received_requests().await.ok_or("recording is on")?;
    let sent = requests.first().ok_or("node B was asked")?;
    assert_eq!(resource, sent.url.path(), "no uid is rewritten (N22)");
    let captured = wire(&b).await?;
    assert!(
        !captured.contains(PATIENT),
        "no identifier reaches the node (N33): {captured}"
    );
    assert!(!captured.contains(CLIENT_TOKEN.as_str()), "{captured}");
    assert!(
        !captured.contains_ignoring_ascii_case("openehr-federation"),
        "{captured}"
    );
    Ok(())
}

// conformance: CP-15 CP-33
#[tokio::test]
async fn a_versioned_write_no_earlier_step_locates_reaches_its_creator_by_step_4() -> TestResult {
    let a = node(EHR_A, &[], &[]).await;
    let b = node(EHR_B, &[], &[]).await;
    let resource = composition(EHR_A, CREATED_AT_A);
    Mock::given(method("DELETE"))
        .and(path(resource.as_str()))
        .respond_with(ResponseTemplate::new(204))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let delete = Request::delete(resource.as_str()).body(Body::empty())?;
    let (status, acting, _, text) = answer(&app, delete).await?;
    assert_eq!(StatusCode::NO_CONTENT, status, "{text}");
    assert_eq!(
        Some("node-a-pub"),
        acting.as_deref(),
        "§12.5.1 step 4 locates a versioned write whose path carries the version, and its \
         node controls it (§12.4, N23, N41)"
    );
    assert_eq!(vec![("DELETE".to_owned(), resource)], asked(&a).await?);
    assert!(asked(&b).await?.is_empty(), "a write is never probed for");
    Ok(())
}

// conformance: CP-15 CP-33
#[tokio::test]
async fn a_versioned_write_whose_path_carries_no_version_is_still_a_400_with_no_earlier_step()
-> TestResult {
    let a = node(EHR_A, &[], &[]).await;
    let b = node(EHR_B, &[], &[]).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let object = CREATED_AT_A.split("::").next().ok_or("an object id")?;
    let update = Request::put(composition(EHR_A, object))
        .header(header::IF_MATCH, format!("\"{CREATED_AT_A}\""))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(read_body(CREATED_AT_A)))?;
    let (status, acting, _, text) = answer(&app, update).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(
        "target-required",
        error_body(&text)?.code,
        "§12.5.1 step 4 reads the path alone, and If-Match is no part of it (N41)"
    );
    assert!(acting.is_none());
    assert_eq!(
        Vec::<(String, String)>::new(),
        asked(&a).await?,
        "no node is sent it"
    );
    assert_eq!(
        Vec::<(String, String)>::new(),
        asked(&b).await?,
        "nothing is probed"
    );
    Ok(())
}
