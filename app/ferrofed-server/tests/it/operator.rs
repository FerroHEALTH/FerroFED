// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read-only operator surface, `{base}/operator/`: admitted only for a
//! caller whose token carries the operator scope its issuer's entry names,
//! answering routing ids and counts, and never a patient identifier
//! (§5.4.1, N33). No specification governs the surface: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_registry::incident::Incident;
use ferrofed_registry::operator::{CreatingSystemEntry, IncidentReport, Page, RouteSource};
use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use http::{Request, StatusCode, header};

use crate::facade::settings_with_room;
use crate::support::{bearer, error_body, operator_bearer, send, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// A registry of one node with one endpoint and one `[[creating_system]]`
/// mapping.
const REGISTRY: &str = r#"
[[organisation]]
id = "org-a"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "http://127.0.0.1:9/a"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[creating_system]]
creating_system_id = "legacy-a.example.org"
endpoint = "node-a-pub"
"#;

/// A gateway over [`REGISTRY`].
fn gateway(dir: &std::path::Path) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, REGISTRY)?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n"
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    Ok(ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &settings_with_room(),
    ))
}

/// A `GET` of `path` as `authorization`.
fn get(path: &str, authorization: &str) -> Result<Request<Body>, http::Error> {
    Request::get(path)
        .header(header::AUTHORIZATION, authorization)
        .body(Body::empty())
}

/// The status and body of `request` through `app`, sent as it is.
async fn answer(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let response = send_as_is(app, request).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, String::from_utf8(bytes.to_vec())?))
}

#[tokio::test]
async fn an_operator_reads_every_incident_kind_and_the_recent_incidents() -> TestResult {
    let dir = tempfile::tempdir()?;
    Incident::EhrIdCollision {
        ehr_id: "7d44b88c-4199-4bad-97dc-d78268e01398".parse()?,
        detection: ferrofed_registry::incident::Detection::Index,
        claimants: vec!["node-a-pub".parse()?],
    }
    .emit();
    let (status, text) = answer(
        gateway(dir.path())?,
        get("/operator/incidents", &operator_bearer()?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let report: IncidentReport = serde_json::from_str(&text)?;
    assert_eq!(4, report.counts.len(), "{report:?}");
    assert!(
        report
            .recent
            .iter()
            .any(|recorded| recorded.ehr_id.as_deref()
                == Some("7d44b88c-4199-4bad-97dc-d78268e01398")),
        "{report:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_ehr_id_that_is_no_uuid_never_reaches_the_operator() -> TestResult {
    let dir = tempfile::tempdir()?;
    Incident::IndexInsertCollision {
        ehr_id: "synthetic-patient-12345".parse()?,
        claimants: vec!["node-a".parse()?],
    }
    .emit();
    let (status, text) = answer(
        gateway(dir.path())?,
        get("/operator/incidents", &operator_bearer()?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(!text.contains("synthetic-patient-12345"), "{text}");
    Ok(())
}

#[tokio::test]
async fn an_operator_reads_the_creating_system_routing_table() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (status, text) = answer(
        gateway(dir.path())?,
        get("/operator/creating-systems", &operator_bearer()?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let report: Page<CreatingSystemEntry> = serde_json::from_str(&text)?;
    let row = |id: &str| {
        report
            .items
            .iter()
            .find(|entry| entry.creating_system_id == id)
            .cloned()
    };
    let member = row("cdr-a.example.org").ok_or("the member's own system_id")?;
    assert_eq!(RouteSource::Member, member.source);
    assert_eq!(Some("node-a"), member.node.as_deref());
    let registered = row("legacy-a.example.org").ok_or("the registered mapping")?;
    assert_eq!(RouteSource::Registered, registered.source);
    assert_eq!(Some("node-a-pub"), registered.endpoint.as_deref());
    Ok(())
}

#[tokio::test]
async fn a_caller_without_the_operator_scope_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path())?;
    for path in [
        "/operator/incidents",
        "/operator/creating-systems",
        "/operator/stored-queries",
    ] {
        let (status, text) = answer(app.clone(), get(path, &bearer()?)?).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{path}: {text}");
        let error = error_body(&text)?;
        assert_eq!("scope-insufficient", error.code, "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_caller_with_no_token_is_refused_before_anything_is_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let request = Request::get("/operator/incidents").body(Body::empty())?;
    let response = send_as_is(gateway(dir.path())?, request).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    Ok(())
}

#[tokio::test]
async fn an_issuer_that_names_no_operator_scope_admits_no_operator() -> TestResult {
    let mut server = settings_with_room();
    for issuer in &mut server.auth.issuers {
        issuer.operator_scope = None;
    }
    let app = ferrofed_server::router(Arc::new(AppState::default()), &server);
    let response = send_as_is(app, get("/operator/incidents", &operator_bearer()?)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, response.status());
    Ok(())
}

#[tokio::test]
async fn a_gateway_without_a_registry_answers_an_empty_table_and_no_stored_queries() -> TestResult {
    let app = ferrofed_server::router(Arc::new(AppState::default()), &settings_with_room());
    for (path, empty) in [
        (
            "/operator/creating-systems",
            r#"{"items":[],"offset":0,"total":0}"#,
        ),
        (
            "/operator/stored-queries",
            r#"{"items":[],"offset":0,"total":0}"#,
        ),
    ] {
        let mut request = Request::get(path).body(Body::empty())?;
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, operator_bearer()?.parse()?);
        let response = send(app.clone(), request).await?;
        assert_eq!(StatusCode::OK, response.status(), "{path}");
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
        assert_eq!(empty, String::from_utf8(bytes.to_vec())?, "{path}");
    }
    Ok(())
}

// RFC 6749 §3.3: a scope is a space-separated list of tokens, so only the
// whole token is the operator scope.
#[tokio::test]
async fn a_scope_that_only_contains_or_starts_the_operator_scope_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path())?;
    for near in [
        "ferrofed:operator-x",
        "xferrofed:operator",
        "ferrofed:operat",
        "FERROFED:OPERATOR",
    ] {
        let authorization = crate::support::bearer_adding_scope(near)?;
        let (status, text) =
            answer(app.clone(), get("/operator/incidents", &authorization)?).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{near}: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn the_routing_table_answers_one_page_and_says_how_many_there_are() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path())?;
    let (status, text) = answer(
        app,
        get(
            "/operator/creating-systems?offset=1&limit=1",
            &operator_bearer()?,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let page: Page<CreatingSystemEntry> = serde_json::from_str(&text)?;
    assert_eq!(1, page.items.len(), "{page:?}");
    assert_eq!(1, page.offset);
    assert_eq!(2, page.total);
    assert_eq!(
        Some("legacy-a.example.org"),
        page.items
            .first()
            .map(|entry| entry.creating_system_id.as_str())
    );
    Ok(())
}

#[tokio::test]
async fn a_page_beyond_the_bound_or_of_nothing_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path())?;
    let beyond = ferrofed_registry::operator::MAX_PAGE + 1;
    for path in [
        format!("/operator/creating-systems?limit={beyond}"),
        String::from("/operator/creating-systems?limit=0"),
        format!("/operator/stored-queries?limit={beyond}"),
    ] {
        let (status, text) = answer(app.clone(), get(&path, &operator_bearer()?)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{path}: {text}");
        assert_eq!("parameter-invalid", error_body(&text)?.code, "{path}");
    }
    Ok(())
}

/// An operator reads the last observed state of each member endpoint.
#[tokio::test]
async fn an_operator_reads_the_dependency_report() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (status, text) = answer(
        gateway(dir.path())?,
        get("/operator/dependencies", &operator_bearer()?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let report: ferrofed_registry::health::DependencyReport = serde_json::from_str(&text)?;
    assert!(
        report
            .endpoints
            .keys()
            .any(|id| id.as_str() == "node-a-pub"),
        "{text}"
    );
    Ok(())
}

/// The dependency report names every member and which are down, so a
/// request without a token, a caller without the operator scope and the
/// health family get none of it.
#[tokio::test]
async fn no_dependency_report_reaches_a_caller_who_is_no_operator() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path())?;
    let unauthenticated = Request::get("/operator/dependencies").body(Body::empty())?;
    let (status, text) = answer(app.clone(), unauthenticated).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{text}");
    assert!(!text.contains("node-a-pub"), "{text}");
    let (status, text) = answer(app.clone(), get("/operator/dependencies", &bearer()?)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert!(!text.contains("node-a-pub"), "{text}");
    let open = Request::get("/health/dependencies").body(Body::empty())?;
    let (status, text) = answer(app, open).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert!(!text.contains("node-a-pub"), "{text}");
    Ok(())
}
