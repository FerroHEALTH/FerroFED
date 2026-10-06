// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query registry as the operator surface lists it: every held
//! version with its name, version and text, to a caller with the operator
//! scope.

use axum::body::Body;
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::reserved;
use ferrofed_registry::operator::Page;
use http::{Request, StatusCode, header};
use openehr_its::rest::generated::definition::StoredQuery;

use crate::facade::node_answering;
use crate::support::{call, operator_bearer};

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
