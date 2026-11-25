// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Listed documents by their stable ids (#1977; MyHealth@EU national contact
//! point API 9.1.1-ci `_id` and the `DocumentReference` read): every entry
//! carries an id derived from its holder and the holder's id of it, the same
//! in every list; `_id` beside the patient selects the entry; with
//! FerroFED's store, `_id` alone and `GET DocumentReference/{id}` name the
//! document's patient through the record of the id
//! (`crate::e2e::face_ids`); without a store both are refused and `metadata`
//! declares no read.

use axum::body::Body;
use http::{Request, StatusCode};
use serde_json::Value;

use super::super::{FHIR, TestResult};
use super::{DOCUMENT_A, DOCUMENT_B, Members, entries, fhir_document, list};
use crate::support::call;

/// The patient summary document type, percent-encoded.
const SUMMARY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C60591-5";

/// `GET {fhir-base}/{path}`.
pub(crate) fn get(path: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/{path}")).body(Body::empty())
}

/// The ids of the entries `bundle` lists, in their order.
pub(crate) fn ids(bundle: &Value) -> Vec<String> {
    entries(bundle, "match")
        .iter()
        .filter_map(|reference| reference.get("id")?.as_str().map(str::to_owned))
        .collect()
}

/// The `searchset` `request` answers with `200`.
async fn listed(
    app: &axum::Router,
    request: Request<Body>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (status, text) = call(app.clone(), request).await?;
    if status != StatusCode::OK {
        return Err(format!("{status}: {text}").into());
    }
    Ok(serde_json::from_str(&text)?)
}

/// The `DocumentReference` interactions `metadata` declares.
pub(crate) async fn interactions(
    app: &axum::Router,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let statement = listed(app, get("metadata")?).await?;
    Ok(statement
        .pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|resource| resource.get("type") == Some(&Value::from("DocumentReference")))
        .flat_map(|resource| {
            resource
                .get("interaction")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|interaction| interaction.get("code")?.as_str().map(str::to_owned))
        .collect())
}

#[tokio::test]
async fn every_entry_keeps_its_id_and_id_beside_the_patient_selects_it() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let first = ids(&listed(&app, list(SUMMARY_TYPE)?).await?);
    let second = ids(&listed(&app, list(SUMMARY_TYPE)?).await?);
    assert_eq!(first.len(), 3, "{first:?}");
    let (mut sorted_first, mut sorted_second) = (first.clone(), second);
    sorted_first.sort();
    sorted_second.sort();
    assert_eq!(
        sorted_first, sorted_second,
        "FHIR R4 Resource.id: one id in every list"
    );
    let one = first.first().ok_or("an id")?;
    let selected = listed(&app, list(&format!("{SUMMARY_TYPE}&_id={one}"))?).await?;
    assert_eq!(ids(&selected), vec![one.clone()]);
    let none = listed(
        &app,
        list(&format!("{SUMMARY_TYPE}&_id={}", "a".repeat(52)))?,
    )
    .await?;
    assert!(ids(&none).is_empty(), "{none}");
    Ok(())
}

#[tokio::test]
async fn without_a_store_an_id_alone_and_a_read_are_refused() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let one = ids(&listed(&app, list(SUMMARY_TYPE)?).await?)
        .into_iter()
        .next()
        .ok_or("an id")?;
    for path in [
        format!("DocumentReference?_id={one}"),
        format!("DocumentReference/{one}"),
    ] {
        let (status, text) = call(app.clone(), get(&path)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{path}: {text}");
        assert!(
            text.contains("keeps no record of the ids it gives"),
            "{text}"
        );
    }
    assert!(!interactions(&app).await?.iter().any(|code| code == "read"));
    Ok(())
}
