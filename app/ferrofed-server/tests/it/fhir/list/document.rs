// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Listed documents by their stable ids (#1977; MyHealth@EU national contact
//! point API 9.1.1-ci `_id` and the `DocumentReference` read): every entry
//! carries an id derived from its holder and the holder's id of it, the same
//! in every list; `_id` beside the patient selects the entry; with
//! FerroFED's store, `_id` alone and `GET DocumentReference/{id}` name the
//! document's patient through the record of the id, and an id the store does
//! not hold is a `404` asking for the search again; without a store both are
//! refused and `metadata` declares no read.

use axum::body::Body;
use ferrofed_testkit::containers::{self, database};
use http::{Request, StatusCode};
use serde_json::Value;

use super::super::{FHIR, TestResult};
use super::{DOCUMENT_A, DOCUMENT_B, Members, entries, fhir_document, list};
use crate::e2e::store::{executed, store_toml};
use crate::support::call;

/// The patient summary document type, percent-encoded.
const SUMMARY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C60591-5";

/// `GET {fhir-base}/{path}`.
fn get(path: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/{path}")).body(Body::empty())
}

/// The ids of the entries `bundle` lists, in their order.
fn ids(bundle: &Value) -> Vec<String> {
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
async fn interactions(app: &axum::Router) -> Result<Vec<String>, Box<dyn std::error::Error>> {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_store_a_document_is_read_by_its_id_until_it_expires() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("document_id", &[]).await?;
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "document_id", dir.path())?;
    let app = tokio::task::block_in_place(|| members.gateway(dir.path(), ("", &store)))?;
    assert!(interactions(&app).await?.iter().any(|code| code == "read"));
    let bundle = listed(&app, list(SUMMARY_TYPE)?).await?;
    let matches = entries(&bundle, "match");
    let listed_one = matches.first().ok_or("an entry")?;
    let one = listed_one["id"].as_str().ok_or("an id")?;
    let (status, text) = call(app.clone(), get(&format!("DocumentReference/{one}"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let read: Value = serde_json::from_str(&text)?;
    assert_eq!(read["resourceType"], "DocumentReference", "{read}");
    assert_eq!(read["id"], listed_one["id"]);
    assert_eq!(read["masterIdentifier"], listed_one["masterIdentifier"]);
    let alone = listed(&app, get(&format!("DocumentReference?_id={one}"))?).await?;
    assert_eq!(ids(&alone), vec![one.to_owned()]);
    let never = "a".repeat(52);
    let (status, text) = call(app.clone(), get(&format!("DocumentReference/{never}"))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert!(
        text.contains("repeat the Patient lookup or the document search"),
        "{text}"
    );
    executed(
        &server.plain_url("document_id"),
        "UPDATE ferrofed.face_id SET expires_at = now() - interval '1 second'",
    )
    .await?;
    members.proxy_a.clear_journal();
    members.proxy_b.clear_journal();
    let (status, text) = call(app.clone(), get(&format!("DocumentReference/{one}"))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert!(
        members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty(),
        "an expired id asks no member"
    );
    Ok(())
}
