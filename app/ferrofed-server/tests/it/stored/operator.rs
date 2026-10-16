// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query registry as the operator surface lists it: every held
//! version with its name, version and text, to a caller with the operator
//! scope, each marked read-only or not, and one name's versions by that
//! name. No specification governs the operator surface: our own design.

use std::error::Error;

use axum::body::Body;
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::reserved;
use ferrofed_registry::operator::{Page, StoredVersion};
use http::{Request, StatusCode, header};
use openehr_its::rest::generated::definition::StoredQuery;

use crate::facade::node_answering;
use crate::support::{bearer, call, error_body, operator_bearer, send_as_is};

use super::{NAME, TestResult, parameterised, stored, two_members};

#[tokio::test]
async fn an_operator_lists_every_held_version_with_its_text() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;
    stored(&app, "1.1.0", &parameterised()).await?;
    let request = Request::get("/operator/stored-queries")
        .header(header::AUTHORIZATION, operator_bearer()?)
        .body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let report: Page<StoredQuery> = serde_json::from_str(&text)?;
    let (own, deployment): (Vec<&StoredQuery>, Vec<&StoredQuery>) =
        report.items.iter().partition(|entry| {
            entry
                .name
                .starts_with(&format!("{}::", reserved::NAMESPACE))
        });
    let versions: Vec<(&str, &str)> = deployment
        .iter()
        .map(|entry| (entry.name.as_str(), entry.version.as_str()))
        .collect();
    assert_eq!(vec![(NAME, "1.0.0"), (NAME, "1.1.0")], versions);
    assert_eq!(
        Section::ALL.len(),
        own.len(),
        "the gateway's own section queries are listed beside them"
    );
    // The gateway holds the definition as it prints it, the patient a
    // parameter and never a value (§12.7, §5.4.1).
    assert!(
        deployment
            .iter()
            .all(|entry| entry.q.starts_with("SELECT c/uid/value FROM EHR e")),
        "{report:?}"
    );
    assert!(
        report.items.iter().all(|entry| {
            entry.q.contains("$patient") && !entry.q.contains(crate::facade::PATIENT)
        }),
        "{report:?}"
    );
    Ok(())
}

/// One stored-query version as the operator surface lists it.
type Listed = StoredVersion<StoredQuery>;

/// A `GET` of `path` on the operator surface as `authorization`.
fn operator_get(path: &str, authorization: &str) -> Result<Request<Body>, http::Error> {
    Request::get(path)
        .header(header::AUTHORIZATION, authorization)
        .body(Body::empty())
}

/// A writable registry gateway holding [`NAME`] at 1.0.0 and 1.1.0.
async fn holding_two_versions(dir: &std::path::Path) -> Result<axum::Router, Box<dyn Error>> {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir, &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;
    stored(&app, "1.1.0", &parameterised()).await?;
    Ok(app)
}

#[tokio::test]
async fn the_listing_marks_the_gateways_own_versions_read_only_and_a_stored_one_writable()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let app = holding_two_versions(dir.path()).await?;
    let request = operator_get("/operator/stored-queries", &operator_bearer()?)?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let page: Page<Listed> = serde_json::from_str(&text)?;
    let own: Vec<&Listed> = page
        .items
        .iter()
        .filter(|entry| {
            entry
                .definition
                .name
                .starts_with(&format!("{}::", reserved::NAMESPACE))
        })
        .collect();
    assert_eq!(Section::ALL.len(), own.len(), "{text}");
    assert!(own.iter().all(|entry| entry.read_only), "{text}");
    let held: Vec<&Listed> = page
        .items
        .iter()
        .filter(|entry| entry.definition.name == NAME)
        .collect();
    assert_eq!(2, held.len(), "{text}");
    assert!(held.iter().all(|entry| !entry.read_only), "{text}");
    Ok(())
}

#[tokio::test]
async fn one_name_answers_its_versions_lowest_first_naming_the_replica() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = holding_two_versions(dir.path()).await?;
    let request = operator_get(
        &format!("/operator/stored-queries/{NAME}"),
        &operator_bearer()?,
    )?;
    let response = send_as_is(app, request).await?;
    assert_eq!(StatusCode::OK, response.status());
    let named = response
        .headers()
        .get(ferrofed_registry::replica::HEADER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let page: Page<Listed> = serde_json::from_slice(&bytes)?;
    let versions: Vec<(&str, &str, bool)> = page
        .items
        .iter()
        .map(|entry| {
            (
                entry.definition.name.as_str(),
                entry.definition.version.as_str(),
                entry.read_only,
            )
        })
        .collect();
    assert_eq!(
        vec![(NAME, "1.0.0", false), (NAME, "1.1.0", false)],
        versions
    );
    assert_eq!(2, page.total);
    assert!(named.is_some(), "the replica header");
    assert_eq!(
        named, page.replica,
        "the header and the body name one replica"
    );
    Ok(())
}

#[tokio::test]
async fn one_of_the_gateways_own_names_answers_read_only() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = holding_two_versions(dir.path()).await?;
    let section = Section::ALL.first().ok_or("a section")?.name()?;
    let request = operator_get(
        &format!("/operator/stored-queries/{}", section.as_str()),
        &operator_bearer()?,
    )?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let page: Page<Listed> = serde_json::from_str(&text)?;
    assert_eq!(1, page.items.len(), "{text}");
    assert!(page.items.iter().all(|entry| entry.read_only), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_name_the_registry_does_not_hold_is_a_404() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = holding_two_versions(dir.path()).await?;
    for path in [
        "/operator/stored-queries/org.example::never_stored",
        "/operator/stored-queries/org.example::patient",
    ] {
        let (status, text) = call(app.clone(), operator_get(path, &operator_bearer()?)?).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{path}: {text}");
        assert_eq!("stored-query-unknown", error_body(&text)?.code, "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_name_itsrest_does_not_admit_is_a_400() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = holding_two_versions(dir.path()).await?;
    for path in [
        "/operator/stored-queries/org.example::a::b",
        "/operator/stored-queries/org.example::aql",
    ] {
        let (status, text) = call(app.clone(), operator_get(path, &operator_bearer()?)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{path}: {text}");
        assert_eq!("query-name-invalid", error_body(&text)?.code, "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_caller_without_the_operator_scope_reads_no_stored_query_by_name() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = holding_two_versions(dir.path()).await?;
    let request = operator_get(&format!("/operator/stored-queries/{NAME}"), &bearer()?)?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!("scope-insufficient", error_body(&text)?.code);
    assert!(!text.contains("$patient"), "no definition text: {text}");
    Ok(())
}
