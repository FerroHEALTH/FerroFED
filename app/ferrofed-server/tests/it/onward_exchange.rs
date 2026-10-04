// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Token exchange and `DPoP` through the real configuration path: a node
//! whose grant is `token_exchange` receives a token exchanged for the caller
//! the gate verified, asked for the caller's scopes that cover the
//! operation and naming the node as its resource; a caller the edge
//! asserted fails that node with nothing sent; a grant with a `DPoP` key
//! proves every request to its node and token endpoint (§13.1, N25, N26,
//! CP-17; RFC 8693 §2, RFC 8707 §2, RFC 9449).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::{
    AuthMode, AuthSettings, IssuerSettings, KeySource, Verification,
};
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::{Claims, Issuer};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::{self, Exchanged, TokenEndpoint};
use ferrofed_testkit::unreachable;
use http::{HeaderName, Request, StatusCode, header};
use jsonwebtoken::jwk::JwkSet;
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, ResponseTemplate};

use crate::facade::{body, crossref, patient_query, post, registry, settings_with_room, wire};
use crate::support::{AUDIENCE, ISSUER, bearer_as, call, issuer, signed};

type TestResult = Result<(), Box<dyn Error>>;

/// The client node A's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The scope of the gateway's own requests.
const SCOPE: &str = "system/aql-*.s";

/// The resource node A's tokens are exchanged for.
const RESOURCE: &str = "https://cdr-a.example.org/openehr";

/// The caller the tests send the query as.
const CALLER: &str = "synthetic-clinician-exchanged";

/// The default token's scope that covers a query: the AQL family alone.
const COVERING: &str = "user/aql-*.cruds";

/// A one-row answer from a node.
const ONE_ROW: &str =
    r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-at-a"]]}"##;

/// The `[credentials]` table of node A's token-exchange grant at
/// `token_url`, with `extra` keys.
fn exchange_grant(token_url: &str, extra: &str) -> String {
    format!(
        "[credentials.\"node-a-pub\".oauth2]\ngrant = \"token_exchange\"\nclient_auth = \"private_key_jwt\"\ntoken_endpoint = \"{token_url}\"\nclient_id = \"{CLIENT_ID}\"\nscope = \"{SCOPE}\"\n{extra}"
    )
}

/// The gateway over node A at `a` and an unreachable node B, the patient
/// resolving at node A, with `tables`, authenticating callers as `auth`.
fn gateway(
    dir: &Path,
    a: &str,
    tables: &str,
    auth: AuthSettings,
) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, unreachable::BASE, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\nbest_effort = false\n\n{}\n{tables}",
        crossref(&[("node-a", "2222aaaa-2222-4222-8222-222222222222")])
    );
    let settings = Config::from_sources(Some(&signed(&text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    let mut server = settings_with_room();
    server.auth = auth;
    Ok(ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &server,
    ))
}

/// A node answering the query to a request `matcher` admits, and `401` to
/// every other.
async fn node(matcher: impl Match + 'static) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(matcher)
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(ONE_ROW.as_bytes().to_vec(), "application/json"),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(&server)
        .await;
    server
}

/// The JWK Set `app` publishes.
async fn published(app: &Router) -> Result<JwkSet, Box<dyn Error>> {
    let request = Request::get("/.well-known/jwks.json").body(Body::empty())?;
    let response = tower::ServiceExt::oneshot(app.clone(), request).await?;
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// The patient query, sent as `authorization`.
fn patient_post(authorization: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = post(body(&patient_query())?)?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, authorization.parse()?);
    Ok(request)
}

/// Node A receives a token exchanged for the caller the gate verified,
/// asked for the caller's scope that covers the query and naming the node
/// as its resource, and the caller's token reaches the token endpoint alone
/// (RFC 8693 §2.1, RFC 8707 §2, N26).
// conformance: CP-17
#[tokio::test]
async fn node_a_receives_a_token_exchanged_for_the_verified_caller() -> TestResult {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    endpoint.accept_exchange(issuer().jwks(), ISSUER);
    endpoint.expect_resource(RESOURCE);
    let a = node(endpoint.bearer_for(CALLER)).await;
    let dir = tempfile::tempdir()?;
    let tables = exchange_grant(
        &endpoint.token_url(),
        &format!("resource = \"{RESOURCE}\"\n"),
    );
    let app = gateway(dir.path(), &a.uri(), &tables, crate::support::auth())?;
    endpoint.trust(published(&app).await?);
    let authorization = bearer_as(CALLER)?;

    let (status, text) = call(app, patient_post(&authorization)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![Exchanged {
            subject: CALLER.to_owned(),
            scope: Some(COVERING.to_owned()),
            resource: Some(RESOURCE.to_owned()),
        }],
        endpoint.exchanges()
    );
    let token = authorization
        .strip_prefix("Bearer ")
        .ok_or("a bearer credential")?;
    let captured = wire(&a).await?;
    assert!(
        !captured.contains(token),
        "the caller's token never reaches the node: {captured}"
    );
    Ok(())
}

/// The edge's header and issuer.
const EDGE_HEADER: &str = "ferrofed-edge-assertion";
const EDGE: &str = "https://edge.example.test";

/// A caller the edge asserted has no token of its own to exchange, so the
/// exchanging node fails `node-error` and is sent nothing.
// conformance: CP-17
#[tokio::test]
async fn an_edge_asserted_caller_fails_an_exchanging_node_with_nothing_sent() -> TestResult {
    let edge = Issuer::new(EDGE)?;
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    endpoint.accept_exchange(edge.jwks(), EDGE);
    let a = node(endpoint.bearer()).await;
    let dir = tempfile::tempdir()?;
    let tables = exchange_grant(
        &endpoint.token_url(),
        &format!("resource = \"{RESOURCE}\"\n"),
    );
    let auth = AuthSettings {
        mode: AuthMode::Edge(HeaderName::from_static(EDGE_HEADER)),
        audience: Some(AUDIENCE.to_owned()),
        issuers: vec![IssuerSettings {
            issuer: EDGE.to_owned(),
            verification: Verification::KeySet(KeySource::Set(edge.jwks())),
            backend_clients: BTreeSet::new(),
            demographic_clients: BTreeSet::new(),
        }],
        ..AuthSettings::default()
    };
    let app = gateway(dir.path(), &a.uri(), &tables, auth)?;
    endpoint.trust(published(&app).await?);
    let mut claims = Claims::new(EDGE, AUDIENCE);
    CALLER.clone_into(&mut claims.sub);
    let mut request = post(body(&patient_query())?)?;
    request
        .headers_mut()
        .insert(EDGE_HEADER, edge.mint(&claims)?.parse()?);

    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert!(endpoint.forms().is_empty(), "no token request was sent");
    assert!(
        a.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty(),
        "node A was sent nothing"
    );
    Ok(())
}

/// A grant with a `DPoP` key proves every request to its node and token
/// endpoint, and node A receives the bound token under the `DPoP` scheme
/// (RFC 9449 §5, §7.1).
// conformance: CP-17
#[tokio::test]
async fn a_dpop_bound_grant_proves_every_request_to_its_node() -> TestResult {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    endpoint.require_dpop();
    let a = node(endpoint.dpop_bound(None)).await;
    let dir = tempfile::tempdir()?;
    let key = dir.path().join("dpop.pem");
    std::fs::write(&key, oauth::p256_pem()?)?;
    let key = toml::Value::String(key.display().to_string());
    let tables = format!(
        "[credentials.\"node-a-pub\".oauth2]\ngrant = \"client_credentials\"\nclient_auth = \"private_key_jwt\"\ntoken_endpoint = \"{}\"\nclient_id = \"{CLIENT_ID}\"\nscope = \"{SCOPE}\"\ndpop_key_file = {key}\n",
        endpoint.token_url()
    );
    let app = gateway(dir.path(), &a.uri(), &tables, crate::support::auth())?;
    endpoint.trust(published(&app).await?);

    let (status, text) = call(app, patient_post(&bearer_as(CALLER)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, endpoint.issued());
    Ok(())
}

/// The configuration refuses a token-exchange grant that names no node as
/// its resource, and a `DPoP` key that is no EC key, naming each key and
/// quoting no part of the file (RFC 8707 §2, RFC 9449).
#[test]
fn the_configuration_refuses_an_exchange_without_a_resource_and_a_bad_dpop_key() -> TestResult {
    let dir = tempfile::tempdir()?;
    let resolve = |tables: &str| -> Result<ConfigError, Box<dyn Error>> {
        match Config::from_sources(Some(&signed(tables)), &BTreeMap::new())?.resolve() {
            Ok(_) => Err(format!("accepted: {tables}").into()),
            Err(error) => Ok(error),
        }
    };
    let error = resolve(&exchange_grant("https://idp.example.org/token", ""))?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "credentials.node-a-pub.oauth2.resource"),
        "{error:?}"
    );
    let bad = dir.path().join("bad.pem");
    std::fs::write(&bad, "Qz7-not-a-key\n")?;
    let bad = toml::Value::String(bad.display().to_string());
    let error = resolve(&exchange_grant(
        "https://idp.example.org/token",
        &format!("resource = \"{RESOURCE}\"\ndpop_key_file = {bad}\n"),
    ))?;
    assert!(
        matches!(&error, ConfigError::DpopKey { key, .. } if key == "credentials.node-a-pub.oauth2.dpop_key_file"),
        "{error:?}"
    );
    assert!(!format!("{error}").contains("Qz7"), "{error}");
    Ok(())
}
