// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Location` of every answer that names a stored query is the URL of the
//! Stored Query resource, `{base}/v1/definition/query/{name}/{version}`
//! (ITS-REST 1.1.0 Definition API, `200_StoredQuery_stored`,
//! `Location_Query`), whichever route answered: each test resolves the header
//! against the request URL (RFC 3986 §5.2) and compares the result with that
//! URL. The routes are the `PUT` at a version, the `PUT` with no version, and
//! the operator's distribution of a held version on the admin listener, each
//! with and without `server.public_url`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_identity::dev::Profile;
use ferrofed_server::admin::{self, Access};
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use url::Url;

use crate::stored_fan_out::{
    ENDPOINT, NAME, VERSION, definition, put, state_serving, storing, three,
};
use crate::support::{exchange, field, operator_bearer};

type TestResult = Result<(), Box<dyn Error>>;

/// The public base URL the gateway is configured with; the suite's JWK Set
/// URL sits under it.
const PUBLIC: &str = "https://gw.example.org";

/// The origin a client reaches the gateway's client listener at when the
/// gateway is configured with no public base URL.
const CLIENT_ORIGIN: &str = "http://gateway.test";

/// The origin of the admin listener.
const ADMIN_ORIGIN: &str = "http://127.0.0.1:9464";

/// The `[server]` line that names [`PUBLIC`].
fn public_url() -> String {
    format!("public_url = \"{PUBLIC}\"")
}

/// The URL of the stored query [`NAME`] at `version` under `origin`.
fn stored_query(origin: &str, version: &str) -> Result<Url, url::ParseError> {
    Url::parse(&format!("{origin}/v1/definition/query/{NAME}/{version}"))
}

/// The URL `location` names, resolved against `request` (RFC 3986 §5.2).
fn resolved(request: &str, location: &str) -> Result<Url, url::ParseError> {
    Url::parse(request)?.join(location)
}

/// The client application and the admin listener's application of a gateway
/// over three nodes that accept every definition, offering distribution,
/// with `server` in its `[server]` table, and the nodes, which serve while
/// they are held.
async fn both(dir: &Path, server: &str) -> Result<(Router, Router, [Server; 3]), Box<dyn Error>> {
    let (a, b, c) = (storing(200).await, storing(200).await, storing(200).await);
    let registry = three(&a, &b, &c);
    let state = state_serving(dir, (&registry, "fan_out_stored_queries = true"), server)?;
    let client = ferrofed_server::router(Arc::clone(&state), &crate::facade::settings_with_room());
    let access = Access::new(&crate::support::auth(), Profile::Production, None);
    Ok((client, admin::router(state, access), [a, b, c]))
}

/// Sends `request` to `app` and returns the `Location` of its `200`.
async fn location(app: &Router, request: Request<Body>) -> Result<Option<String>, Box<dyn Error>> {
    let (status, headers, body) = exchange(app.clone(), request).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(field(&headers, header::LOCATION.as_str()).map(str::to_owned))
}

/// `PUT {base}/v1/definition/query/{name}` with no version.
fn unversioned() -> Result<Request<Body>, http::Error> {
    Request::put(format!("/v1/definition/query/{NAME}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(definition()))
}

/// The operator's distribution of [`NAME`] at [`VERSION`] to node C.
fn distribute() -> Result<Request<Body>, Box<dyn Error>> {
    Ok(
        Request::post(format!("/admin/stored-queries/{NAME}/{VERSION}/distribute"))
            .header(header::AUTHORIZATION, operator_bearer()?)
            .header(ENDPOINT, "node-c-pub")
            .body(Body::empty())?,
    )
}

// conformance: CP-40
#[tokio::test]
async fn a_versioned_put_names_the_stored_query_relative_to_its_request() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, _, _nodes) = both(dir.path(), "").await?;
    let named = location(&app, put(&definition(), None)?)
        .await?
        .ok_or("a Location")?;
    let request = stored_query(CLIENT_ORIGIN, VERSION)?;
    assert_eq!(
        stored_query(CLIENT_ORIGIN, VERSION)?,
        resolved(request.as_str(), &named)?,
        "ITS-REST Location_Query: {named}"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn an_unversioned_put_names_the_stored_query_relative_to_its_request() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, _, _nodes) = both(dir.path(), "").await?;
    let named = location(&app, unversioned()?).await?.ok_or("a Location")?;
    let request = format!("{CLIENT_ORIGIN}/v1/definition/query/{NAME}");
    assert_eq!(
        stored_query(CLIENT_ORIGIN, "1.0.0")?,
        resolved(&request, &named)?,
        "ITS-REST Location_Query: {named}"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn with_a_public_url_both_puts_name_the_absolute_url_of_the_stored_query() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, _, _nodes) = both(dir.path(), &public_url()).await?;
    let named = location(&app, put(&definition(), None)?)
        .await?
        .ok_or("a Location")?;
    let expected = stored_query(PUBLIC, VERSION)?;
    assert_eq!(expected.as_str(), named, "ITS-REST Location_Query");
    let request = stored_query(PUBLIC, VERSION)?;
    assert_eq!(expected, resolved(request.as_str(), &named)?);

    let named = location(&app, unversioned()?).await?.ok_or("a Location")?;
    let expected = stored_query(PUBLIC, "1.0.1")?;
    assert_eq!(expected.as_str(), named, "ITS-REST Location_Query");
    let request = format!("{PUBLIC}/v1/definition/query/{NAME}");
    assert_eq!(expected, resolved(&request, &named)?);
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn with_a_public_url_the_admin_distribution_names_the_stored_query() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, operator, _nodes) = both(dir.path(), &public_url()).await?;
    location(&app, put(&definition(), None)?).await?;
    let named = location(&operator, distribute()?)
        .await?
        .ok_or("a Location")?;
    let request = format!("{ADMIN_ORIGIN}/admin/stored-queries/{NAME}/{VERSION}/distribute");
    assert_eq!(
        stored_query(PUBLIC, VERSION)?,
        resolved(&request, &named)?,
        "ITS-REST Location_Query, from the admin listener's request: {named}"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn without_a_public_url_the_admin_distribution_names_no_location() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, operator, _nodes) = both(dir.path(), "").await?;
    location(&app, put(&definition(), None)?).await?;
    assert_eq!(
        None,
        location(&operator, distribute()?).await?,
        "no reference from the admin listener's origin names the client listener's URL"
    );
    Ok(())
}
