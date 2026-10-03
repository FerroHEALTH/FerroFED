// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The DEMOGRAPHIC area against two mock nodes (§7a.1, §12.6, N32, N31, N33;
//! CP-25): it is never federated. By default every request under
//! `{base}/v1/demographic/` answers `501` and no node is asked. With
//! `federation.demographic_endpoint` set, a request whose targeting header
//! names that endpoint goes to it alone, byte-identical, and is answered as
//! that node answered; one naming no endpoint or another is refused, because
//! the request chooses its node (§12.4, §12.6, N23). `OPTIONS {base}/`
//! declares whichever behaviour runs, and the behaviour matches it. Every
//! assertion on what a node received reads the node's own capture (§16,
//! track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_server::EXIT_CONFIG;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use openehr_federation::headers::ENDPOINT;
use openehr_federation::options::OptionsRoot;
use wiremock::ResponseTemplate;

use crate::facade::{PATIENT, gateway, registry, schema, wire};
use crate::run::binary;
use crate::support::{acted, asked, call, error_body, exchange, field, mount, refused_at_neither};

type TestResult = Result<(), Box<dyn Error>>;

const ENDPOINT_A: &str = "node-a-pub";
const ENDPOINT_B: &str = "node-b-pub";

/// The `[federation]` line that declares node B for the area.
const ROUTED_TO_B: &str = "demographic_endpoint = \"node-b-pub\"";

/// The PERSON collection (ITS-REST Demographic API).
const PERSONS: &str = "/v1/demographic/person";

/// A synthetic `OBJECT_VERSION_ID` of a PERSON node B holds.
const PARTY: &str = "6a1e7b1c-4d0f-4c3e-9a8e-5f2b1d0c9e7a::cdr-b.example.org::1";

/// The client's own credential, which no node ever sees.
const CLIENT_TOKEN: &str = "synthetic-client-token";

/// A synthetic PERSON, spaced and encoded as a re-serialisation would not
/// keep it, carrying a synthetic identifier under the example arc.
fn person() -> String {
    "{ \"_type\" : \"PERSON\",\n  \"name\": {\"value\": \"Synthétic  Person\"},\n  \"identities\": [],\n  \"details\": {\"items\": [{\"value\": {\"_type\": \"DV_IDENTIFIER\", \"id\": \"2.999.42-0007\", \"issuer\": \"urn:oid:2.999\"}}]}\n}\n".to_owned()
}

/// The gateway over node A and node B, with `federation` in its
/// `[federation]` table.
fn over(
    dir: &std::path::Path,
    (a, b): (&Server, &Server),
    federation: &str,
) -> Result<Router, Box<dyn Error>> {
    gateway(dir, &registry(&a.uri(), &b.uri(), ""), "", federation)
}

/// A read of the PERSON [`PARTY`], naming `target` in the endpoint header
/// when given.
fn read(target: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::get(format!("{PERSONS}/{PARTY}"));
    if let Some(target) = target {
        request = request.header(ENDPOINT, target);
    }
    request
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
}

/// The creation of [`person`], naming `target` in the endpoint header when
/// given.
fn create(target: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::post(PERSONS);
    if let Some(target) = target {
        request = request.header(ENDPOINT, target);
    }
    request
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(person()))
}

/// Node B holding [`PARTY`], answering a read with `answered`.
async fn holder(answered: &str) -> Server {
    let b = Server::start().await;
    mount(
        &b,
        "GET",
        format!("{PERSONS}/{PARTY}"),
        ResponseTemplate::new(200)
            .insert_header("ETag", format!("\"{PARTY}\"").as_str())
            .set_body_raw(answered.as_bytes().to_vec(), "application/json"),
    )
    .await;
    b
}

/// The `its_rest.demographic` the gateway `app` declares, with the body
/// checked against the vendored schema.
async fn declared(app: Router) -> Result<String, Box<dyn Error>> {
    let (status, text) = call(app, Request::options("/").body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate_options(&text)?;
    let body: OptionsRoot = serde_json::from_str(&text)?;
    Ok(body.federation.its_rest.demographic.as_str().to_owned())
}

// conformance: CP-25
#[tokio::test]
async fn by_default_a_read_and_a_create_are_501_and_no_node_is_asked() -> TestResult {
    let a = Server::start().await;
    let b = holder(&person()).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), (&a, &b), "")?;
    for request in [read(None)?, read(Some(ENDPOINT_B))?, create(None)?] {
        let (status, _, body) = exchange(app.clone(), request).await?;
        let text = String::from_utf8(body)?;
        assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "§7a.1, N32: {text}");
        assert_eq!("not-implemented", error_body(&text)?.code);
    }
    assert!(asked(&a).await?.is_empty(), "node A received nothing");
    assert!(asked(&b).await?.is_empty(), "node B received nothing");
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn with_the_setting_a_party_read_reaches_only_that_node_byte_identical() -> TestResult {
    let answered = format!("{{ \"uid\" : {{\"value\": \"{PARTY}\"}},\n  \"_type\": \"PERSON\" }}");
    let a = Server::start().await;
    let b = holder(&answered).await;
    let dir = tempfile::tempdir()?;
    let mut request = read(Some(ENDPOINT_B))?;
    let fields = request.headers_mut();
    fields.insert("x-patient", PATIENT.parse()?);
    fields.insert(
        header::AUTHORIZATION,
        format!("Bearer {CLIENT_TOKEN}").parse()?,
    );
    let (status, headers, body) =
        exchange(over(dir.path(), (&a, &b), ROUTED_TO_B)?, request).await?;
    assert_eq!(StatusCode::OK, status);
    acted(&headers, ENDPOINT_B, "cdr-b.example.org");
    assert_eq!(
        Some(format!("\"{PARTY}\"").as_str()),
        field(&headers, "etag"),
        "N31: ETag passed through"
    );
    assert_eq!(
        answered.as_bytes(),
        body.as_slice(),
        "node B's answer, byte for byte"
    );
    assert_eq!(
        vec![("GET".to_owned(), format!("{PERSONS}/{PARTY}"))],
        asked(&b).await?,
        "node B is asked once"
    );
    assert!(asked(&a).await?.is_empty(), "node A is never asked");
    let composed = wire(&b).await?;
    assert!(!composed.contains(PATIENT), "N33: {composed}");
    assert!(!composed.contains(CLIENT_TOKEN), "N33: {composed}");
    assert!(
        !composed.contains_ignoring_ascii_case("openehr-federation"),
        "the federation's own headers stay at the gateway: {composed}"
    );
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn with_the_setting_a_create_body_is_forwarded_unchanged() -> TestResult {
    let created = format!("https://cdr-b.example.org/openehr{PERSONS}/{PARTY}");
    let b = Server::start().await;
    mount(
        &b,
        "POST",
        PERSONS.to_owned(),
        ResponseTemplate::new(201)
            .insert_header("Location", created.as_str())
            .insert_header("ETag", format!("\"{PARTY}\"").as_str()),
    )
    .await;
    let a = Server::start().await;
    let dir = tempfile::tempdir()?;
    let (status, headers, _) = exchange(
        over(dir.path(), (&a, &b), ROUTED_TO_B)?,
        create(Some(ENDPOINT_B))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status);
    acted(&headers, ENDPOINT_B, "cdr-b.example.org");
    assert_eq!(
        Some(created.as_str()),
        field(&headers, "location"),
        "N31: Location passed through"
    );
    let requests = b.received_requests().await.ok_or("recording is on")?;
    let [sent] = requests.as_slice() else {
        return Err(format!("one create at node B, not {}", requests.len()).into());
    };
    assert_eq!(
        person().as_bytes(),
        sent.body.as_slice(),
        "§5.4 scope note: a client-supplied body passes through unmodified"
    );
    assert_eq!(
        Some("application/json"),
        field(&sent.headers, "content-type"),
        "a declared media type travels with the body"
    );
    assert!(asked(&a).await?.is_empty(), "node A is never asked");
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn a_header_naming_the_declared_endpoint_is_accepted() -> TestResult {
    let a = Server::start().await;
    let b = holder(&person()).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), (&a, &b), ROUTED_TO_B)?;
    let (status, headers, _) = exchange(app, read(Some(ENDPOINT_B))?).await?;
    assert_eq!(StatusCode::OK, status);
    acted(&headers, ENDPOINT_B, "cdr-b.example.org");
    assert!(asked(&a).await?.is_empty(), "node A is never asked");
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn with_the_setting_a_request_naming_no_endpoint_is_refused_and_no_node_is_asked()
-> TestResult {
    for request in [read(None)?, create(None)?] {
        let a = Server::start().await;
        let b = holder(&person()).await;
        let dir = tempfile::tempdir()?;
        refused_at_neither(
            over(dir.path(), (&a, &b), ROUTED_TO_B)?,
            request,
            "target-required",
            (&a, &b),
        )
        .await?;
    }
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn a_header_naming_another_endpoint_several_or_a_star_is_refused() -> TestResult {
    for (named, code) in [
        (ENDPOINT_A, "targeting-conflict"),
        ("node-a-pub, node-b-pub", "endpoint-several"),
        ("*", "endpoint-unknown"),
        ("node-x-pub", "endpoint-unknown"),
    ] {
        let a = Server::start().await;
        let b = holder(&person()).await;
        let dir = tempfile::tempdir()?;
        refused_at_neither(
            over(dir.path(), (&a, &b), ROUTED_TO_B)?,
            read(Some(named))?,
            code,
            (&a, &b),
        )
        .await?;
    }
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn the_conflict_names_both_endpoints() -> TestResult {
    let a = Server::start().await;
    let b = holder(&person()).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), (&a, &b), ROUTED_TO_B)?;
    let (status, _, body) = exchange(app, read(Some(ENDPOINT_A))?).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert!(text.contains(ENDPOINT_A), "§8.4.1: {text}");
    assert!(text.contains(ENDPOINT_B), "§8.4.1: {text}");
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn with_the_setting_a_path_naming_no_its_rest_operation_is_still_501() -> TestResult {
    let a = Server::start().await;
    let b = holder(&person()).await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), (&a, &b), ROUTED_TO_B)?;
    let request = Request::get(format!("/v1/demographic/party/{PARTY}")).body(Body::empty())?;
    let (status, _, body) = exchange(app, request).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "{text}");
    assert!(asked(&b).await?.is_empty(), "node B received nothing");
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn options_declares_each_mode_and_the_behaviour_matches_it() -> TestResult {
    let a = Server::start().await;
    let b = holder(&person()).await;
    let dir = tempfile::tempdir()?;
    let unsupported = declared(over(dir.path(), (&a, &b), "")?).await?;
    assert_eq!("unsupported: 501", unsupported, "§7a.1, N32");
    let (status, _, _) = exchange(over(dir.path(), (&a, &b), "")?, read(None)?).await?;
    assert_eq!(
        StatusCode::NOT_IMPLEMENTED,
        status,
        "as declared: {unsupported}"
    );

    let routed = declared(over(dir.path(), (&a, &b), ROUTED_TO_B)?).await?;
    assert!(
        routed.starts_with("routed-single-node") && routed.contains(ENDPOINT_B),
        "§7a.1, §12.6, N32: {routed}"
    );
    assert!(!routed.starts_with("federated"), "N32: {routed}");
    let (status, headers, _) = exchange(
        over(dir.path(), (&a, &b), ROUTED_TO_B)?,
        read(Some(ENDPOINT_B))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "as declared: {routed}");
    acted(&headers, ENDPOINT_B, "cdr-b.example.org");
    assert!(asked(&a).await?.is_empty(), "node A is never asked");
    Ok(())
}

// conformance: CP-25
#[tokio::test]
async fn options_on_a_party_names_its_methods_only_when_routed() -> TestResult {
    let a = Server::start().await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let at = format!("{PERSONS}/{PARTY}");
    let (status, _) = call(
        over(dir.path(), (&a, &b), "")?,
        Request::options(at.as_str()).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "N32: not exposed");
    let (status, headers, _) = exchange(
        over(dir.path(), (&a, &b), ROUTED_TO_B)?,
        Request::options(at.as_str()).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NO_CONTENT, status);
    assert_eq!(
        Some("GET, PUT, DELETE, OPTIONS"),
        field(&headers, "allow"),
        "§7a.2, RFC 9110 §10.2.1"
    );
    assert!(asked(&a).await?.is_empty() && asked(&b).await?.is_empty());
    Ok(())
}

/// The `config check` output for a gateway over a registry at nothing,
/// with `federation` in its `[federation]` table.
fn checked(federation: &str) -> Result<std::process::Output, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(
        &document,
        registry("http://127.0.0.1:9/a", "http://127.0.0.1:9/b", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let toml = format!(
        "[registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n{federation}\n"
    );
    binary(&["config", "check"], &toml)
}

#[test]
fn config_check_refuses_a_demographic_endpoint_the_registry_does_not_hold() -> TestResult {
    let output = checked("demographic_endpoint = \"node-x-pub\"")?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("federation.demographic_endpoint") && stderr.contains("node-x-pub"),
        "names the key and the id: {stderr}"
    );

    let output = checked("demographic_endpoint = \"node a\"")?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("federation.demographic_endpoint"),
        "names the key: {stderr}"
    );

    let output = checked(ROUTED_TO_B)?;
    assert_eq!(
        Some(0),
        output.status.code(),
        "a registry endpoint is accepted"
    );
    Ok(())
}

#[test]
fn a_demographic_endpoint_without_a_registry_refuses_to_boot() -> TestResult {
    let output = binary(
        &["config", "check"],
        &format!("[federation]\n{ROUTED_TO_B}\n"),
    )?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("federation.demographic_endpoint"),
        "names the key: {stderr}"
    );
    Ok(())
}
