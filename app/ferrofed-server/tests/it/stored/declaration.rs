// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What `OPTIONS` declares with and without the registry (CP-23, CP-34).

use std::collections::BTreeMap;

use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use http::{Request, StatusCode, header};
use openehr_federation::options::{DefinitionBehaviour, OptionsRoot};

use crate::facade::{node_answering, schema};
use crate::support::{call, error_body, send};

use super::{NAME, TestResult, bound, invoke, parameterised, put, two_members};

// conformance: CP-23 CP-40
#[tokio::test]
async fn options_declares_the_registry_and_the_methods_it_serves() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let (status, text) = call(app.clone(), Request::options("/").body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate_options(&text)?;
    let body: OptionsRoot = serde_json::from_str(&text)?;
    assert_eq!(
        DefinitionBehaviour::new(false)
            .with_stored_query_registry(true)?
            .with_stored_query_fan_out(false)?,
        body.federation.definition,
        "§7a.2, N44: the registry is declared; no definition fan-out"
    );
    let described = &body.federation.its_rest.definition;
    assert!(
        described.starts_with("routed-single-node") && described.contains("gateway registry"),
        "§7a.2 definition-area-split: templates routed, stored queries held: {described}"
    );
    for (uri, expected) in [
        (
            "/v1/definition/template/adl1.4".to_owned(),
            "GET, POST, OPTIONS",
        ),
        (format!("/v1/query/{NAME}"), "GET, POST, OPTIONS"),
        (format!("/v1/query/{NAME}/1.0.0"), "GET, POST, OPTIONS"),
        (format!("/v1/definition/query/{NAME}"), "GET, PUT, OPTIONS"),
        (
            format!("/v1/definition/query/{NAME}/1.0.0"),
            "GET, PUT, OPTIONS",
        ),
    ] {
        let response = send(app.clone(), Request::options(&uri).body(Body::empty())?).await?;
        assert_eq!(StatusCode::NO_CONTENT, response.status(), "{uri}");
        let allow = response
            .headers()
            .get(header::ALLOW)
            .ok_or("an Allow field")?
            .to_str()?;
        assert_eq!(expected, allow, "§7a.2: {uri}");
    }
    Ok(())
}

// conformance: CP-34
#[tokio::test]
async fn without_the_registry_a_definition_is_routed_and_no_name_is_invoked() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = crate::facade::dev_gateway(dir.path(), &a.uri(), &b.uri(), &[])?;
    let unversioned = Request::put(format!("/v1/definition/query/{NAME}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(parameterised()))?;
    let (status, text) = call(app.clone(), unversioned).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        status,
        "§12.7 registry-not-offered: §12.6 routes it to one explicitly chosen node: {text}"
    );
    assert_eq!("target-required", error_body(&text)?.code);
    let (status, text) = call(app.clone(), put(NAME, "1.0.0", &parameterised())?).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        status,
        "§12.7 registry-not-offered: the versioned PUT routes as the unversioned one: {text}"
    );
    assert_eq!("target-required", error_body(&text)?.code);
    let (status, text) = call(app, invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "§12.6: {text}");
    for server in [&a, &b] {
        let requests = server.received_requests().await.ok_or("recording is on")?;
        assert!(requests.is_empty(), "no node is asked");
    }
    Ok(())
}

#[test]
fn a_store_without_a_registry_document_refuses_to_resolve() -> TestResult {
    let text = "[stored_queries]\npath = \"/var/lib/ferrofed/definitions.redb\"\n";
    let refused = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err()
        .ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Missing { key } if key == "registry.document"),
        "{refused:?}"
    );
    Ok(())
}
