// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The EHR a routed create left, in its access record (Regulation (EU)
//! 2025/327 Art 9(1), Annex II 3.2): the create's path names no `ehr_id`,
//! so the record reads the one the node's `ETag` or `Location` named
//! (ITS-REST 1.1.0 EHR API `ETag_EHR`, `Location_EHR`), held to the
//! `HIER_OBJECT_ID` form, and names the patient the identity binding holds
//! behind it; an answer that names none, or no `HIER_OBJECT_ID`, is said
//! so on the origin, and no text of the answer reaches the record as
//! itself (§5.4, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{TestResult, accesses, details, named, settings_resolving};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, crossref, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::call;

/// The EHR collection, where a create names no `ehr_id`.
const AT: &str = "/v1/ehr";

/// The status and the one access record of an EHR create node A answers
/// with `status` and `headers`, the patient looked up in [`NAMESPACE`].
async fn created(
    status: u16,
    headers: &[(&'static str, String)],
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    let node = Server::start().await;
    let mut template = ResponseTemplate::new(status);
    for (name, value) in headers {
        template = template.insert_header(*name, value.as_str());
    }
    Mock::given(method("POST"))
        .and(path(AT))
        .respond_with(template)
        .mount(&node)
        .await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let idle = format!("{}/node-b", node.uri());
    let asked = format!("\n[access_log]\npatient_namespaces = [\"{NAMESPACE}\"]\n");
    let identity = crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]);
    let settings = settings_resolving(
        dir.path(),
        (&node.uri(), &idle),
        &repository,
        ("", ""),
        (&asked, &identity),
    )?;
    let state = Arc::new(AppState::build(&settings)?);
    let app = ferrofed_server::router(state, &settings_with_room());
    let request = Request::post(AT)
        .header("openEHR-federation-endpoint", "node-a-pub")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty())?;
    let (answered, text) = call(app, request).await?;
    // NOTE: no specification governs this: our own design; an identity service's own
    // record may arrive first, so the wait goes on until the access record does.
    let mut records = Vec::new();
    for count in 1..=3 {
        records = accesses(&repository.wait_for(count, SETTLE).await)?;
        if !records.is_empty() {
            break;
        }
    }
    let [record] = <[Value; 1]>::try_from(records)
        .map_err(|records| format!("one record of the create ({text}), got {records:?}"))?;
    Ok((answered, record))
}

/// The `ehr_id`s the `ehr` entities of `record` name.
fn ehrs(record: &Value) -> Vec<String> {
    named(record, "ehr")
        .iter()
        .filter_map(|ehr| {
            ehr.pointer("/what/identifier/value")
                .and_then(Value::as_str)
        })
        .map(str::to_owned)
        .collect()
}

/// The `ehr_id` the origin of `record` names as created, and why it cannot
/// tell one.
fn origin_says(record: &Value) -> (Vec<String>, Vec<String>) {
    (
        details(record, "origin", "created-ehr"),
        details(record, "origin", "created-ehr-unread"),
    )
}

#[tokio::test]
async fn an_ehr_create_names_the_ehr_id_its_etag_named_and_the_patient_behind_it() -> TestResult {
    let (status, record) = created(201, &[("ETag", format!("\"{EHR_A}\""))]).await?;
    assert_eq!(StatusCode::CREATED, status);
    assert_eq!(
        vec![EHR_A.to_owned()],
        ehrs(&record),
        "Art 9(1): a search by the EHR finds the access that created it"
    );
    assert_eq!(vec!["node-a-pub"], details(&record, "ehr", "endpoint"));
    assert_eq!(
        vec!["found"],
        details(&record, "ehr", "patient-lookup"),
        "the patient behind the new ehr_id is looked up as for any other"
    );
    assert_eq!((vec![EHR_A.to_owned()], Vec::new()), origin_says(&record));
    assert_eq!(vec!["active"], details(&record, "origin", "status"));
    Ok(())
}

#[tokio::test]
async fn an_ehr_create_reads_the_location_when_no_etag_names_it() -> TestResult {
    let location = format!("https://cdr-a.example.org/v1/ehr/{EHR_A}");
    let (_, record) = created(201, &[("Location", location)]).await?;
    assert_eq!(vec![EHR_A.to_owned()], ehrs(&record));
    assert_eq!((vec![EHR_A.to_owned()], Vec::new()), origin_says(&record));
    Ok(())
}

#[tokio::test]
async fn an_ehr_create_whose_answer_names_no_ehr_id_is_recorded_unread() -> TestResult {
    let (status, record) = created(201, &[]).await?;
    assert_eq!(StatusCode::CREATED, status, "the create was made");
    assert!(ehrs(&record).is_empty());
    assert_eq!(
        (Vec::new(), vec!["unnamed".to_owned()]),
        origin_says(&record)
    );

    let (_, record) = created(201, &[("ETag", "\"\"".to_owned())]).await?;
    assert!(ehrs(&record).is_empty());
    assert_eq!(
        (Vec::new(), vec!["malformed".to_owned()]),
        origin_says(&record)
    );
    Ok(())
}

#[tokio::test]
async fn a_create_the_node_refused_names_no_ehr() -> TestResult {
    let (status, record) = created(409, &[("ETag", format!("\"{EHR_A}\""))]).await?;
    assert_eq!(StatusCode::CONFLICT, status);
    assert!(ehrs(&record).is_empty(), "a failed create left no EHR");
    assert_eq!((Vec::new(), Vec::new()), origin_says(&record));
    assert_eq!(vec!["node-error"], details(&record, "origin", "status"));
    Ok(())
}

#[tokio::test]
async fn an_answer_that_names_a_patient_identifier_in_place_of_an_ehr_id_records_none() -> TestResult
{
    let (status, record) = created(
        201,
        &[
            ("ETag", format!("\"{NAMESPACE}|{PATIENT}\"")),
            (
                "Location",
                format!("https://cdr-a.example.org/v1/ehr?subject_id={PATIENT}"),
            ),
        ],
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status);
    assert!(ehrs(&record).is_empty());
    assert_eq!(
        (Vec::new(), vec!["malformed".to_owned()]),
        origin_says(&record)
    );
    assert!(
        !record.to_string().contains(PATIENT),
        "§5.4, N33: no value the node put in its answer reaches the record as itself"
    );
    Ok(())
}
