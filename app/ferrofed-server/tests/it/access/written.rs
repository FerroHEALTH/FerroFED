// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version a routed write left, in its access record (Regulation (EU)
//! 2025/327 Annex II 3.2): read from the node's `ETag` or `Location` as
//! ITS-REST 1.1.0 names it, held to the `OBJECT_VERSION_ID` form, and said
//! to be unknown when the answer names none or names no version id. The
//! origin is named in the §11.1 status set every other record's origins
//! are (N16).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::{Method, Request, StatusCode, header};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{LAB_REPORT, TestResult, accesses, composition, details, gateway_with};
use crate::facade::{EHR_A, PATIENT, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::call;

/// The version the request amends, created at node A.
const PRECEDING: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The version node A answers a write with.
const LEFT: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::2";

/// The targeting header that names node A.
const TARGET: (&str, &str) = ("openEHR-federation-endpoint", "node-a-pub");

/// What node A answers a write with: its status and its headers.
struct Answer {
    status: u16,
    headers: Vec<(&'static str, String)>,
}

impl Answer {
    /// A success with `status` naming [`LEFT`] in `ETag`.
    fn tagged(status: u16) -> Self {
        Self {
            status,
            headers: vec![("ETag", format!("\"{LEFT}\""))],
        }
    }

    /// A success with `status` naming no version.
    fn bare(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
        }
    }
}

/// The request `verb` `at`, targeting node A, naming [`PRECEDING`] in
/// `If-Match` when `versioned`, carrying `sent` as canonical JSON.
fn request(
    verb: &Method,
    at: &str,
    versioned: bool,
    sent: String,
) -> Result<Request<Body>, http::Error> {
    let mut request = Request::builder()
        .method(verb.clone())
        .uri(at)
        .header(TARGET.0, TARGET.1)
        .header(header::CONTENT_TYPE, "application/json");
    if versioned {
        request = request.header(header::IF_MATCH, format!("\"{PRECEDING}\""));
    }
    request.body(Body::from(sent))
}

/// The status of the answer to `sent`, which node A answers at `at` with
/// `answer`, and the one access record of it.
async fn recorded(
    sent: Request<Body>,
    at: &str,
    answer: Answer,
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    recorded_under(sent, at, answer, "").await
}

/// The status and the record of [`recorded`], the `[federation]` keys
/// `federation` added.
async fn recorded_under(
    sent: Request<Body>,
    at: &str,
    answer: Answer,
    federation: &str,
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    let node = Server::start().await;
    let mut template = ResponseTemplate::new(answer.status);
    for (name, value) in &answer.headers {
        template = template.insert_header(*name, value.as_str());
    }
    Mock::given(method(sent.method().as_str()))
        .and(path(at))
        .respond_with(template)
        .mount(&node)
        .await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let idle = format!("{}/node-b", node.uri());
    let app = gateway_with(
        dir.path(),
        (&node.uri(), &idle),
        &repository,
        ("", federation),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, sent).await?;
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = <[Value; 1]>::try_from(records)
        .map_err(|records| format!("one record of the write ({text}), got {records:?}"))?;
    Ok((status, record))
}

/// The version `record`'s origin names, and why it cannot tell one.
fn version(record: &Value) -> (Vec<String>, Vec<String>) {
    (
        details(record, "origin", "written-version"),
        details(record, "origin", "written-version-unread"),
    )
}

#[tokio::test]
async fn a_composition_create_records_the_version_its_etag_names() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/composition");
    let sent = request(
        &Method::POST,
        &at,
        false,
        composition(LAB_REPORT, PRECEDING),
    )?;
    let (status, record) = recorded(sent, &at, Answer::tagged(201)).await?;
    assert_eq!(StatusCode::CREATED, status);
    assert_eq!((vec![LEFT.to_owned()], Vec::new()), version(&record));
    assert_eq!(
        vec![LEFT],
        details(&record, "ehds-categories", "version-uid"),
        "the COMPOSITION written is the version the node created, as in a receipt"
    );
    assert_eq!(vec!["active"], details(&record, "origin", "status"));
    Ok(())
}

#[tokio::test]
async fn a_composition_create_reads_the_location_when_no_etag_names_it() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/composition");
    let sent = request(
        &Method::POST,
        &at,
        false,
        composition(LAB_REPORT, PRECEDING),
    )?;
    let answer = Answer {
        status: 201,
        headers: vec![(
            "Location",
            format!("https://cdr-a.example.org/v1/ehr/{EHR_A}/composition/{LEFT}"),
        )],
    };
    let (_, record) = recorded(sent, &at, answer).await?;
    assert_eq!((vec![LEFT.to_owned()], Vec::new()), version(&record));
    Ok(())
}

#[tokio::test]
async fn a_composition_update_records_the_new_version_never_the_one_it_amends() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/composition/8849182c-82ad-4088-a07f-48ead4180515");
    let sent = request(&Method::PUT, &at, true, composition(LAB_REPORT, PRECEDING))?;
    let (status, record) = recorded(sent, &at, Answer::tagged(204)).await?;
    assert_eq!(StatusCode::NO_CONTENT, status);
    assert_eq!(record["action"], "U");
    assert_eq!((vec![LEFT.to_owned()], Vec::new()), version(&record));
    assert_eq!(
        vec![LEFT],
        details(&record, "ehds-categories", "version-uid")
    );
    Ok(())
}

#[tokio::test]
async fn a_composition_delete_records_the_version_its_answer_names() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/composition/{PRECEDING}");
    let sent = request(&Method::DELETE, &at, false, String::new())?;
    let (status, record) = recorded(sent, &at, Answer::tagged(204)).await?;
    assert_eq!(StatusCode::NO_CONTENT, status);
    assert_eq!(record["action"], "D");
    assert_eq!((vec![LEFT.to_owned()], Vec::new()), version(&record));
    Ok(())
}

#[tokio::test]
async fn an_ehr_status_update_records_the_new_version() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/ehr_status");
    let sent = request(&Method::PUT, &at, true, "{}".to_owned())?;
    let (_, record) = recorded(sent, &at, Answer::tagged(204)).await?;
    assert_eq!((vec![LEFT.to_owned()], Vec::new()), version(&record));
    Ok(())
}

#[tokio::test]
async fn a_directory_create_records_the_new_version() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/directory");
    let sent = request(&Method::POST, &at, false, "{}".to_owned())?;
    let (_, record) = recorded(sent, &at, Answer::tagged(201)).await?;
    assert_eq!((vec![LEFT.to_owned()], Vec::new()), version(&record));
    Ok(())
}

#[tokio::test]
async fn a_directory_delete_whose_answer_names_no_version_says_it_cannot_tell() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/directory");
    let sent = request(&Method::DELETE, &at, true, String::new())?;
    let (_, record) = recorded(sent, &at, Answer::bare(204)).await?;
    assert_eq!((Vec::new(), vec!["unnamed".to_owned()]), version(&record));
    Ok(())
}

#[tokio::test]
async fn a_demographic_create_records_the_new_version() -> TestResult {
    let at = "/v1/demographic/person";
    let sent = request(&Method::POST, at, false, "{}".to_owned())?;
    let (_, record) = recorded_under(
        sent,
        at,
        Answer::tagged(201),
        "demographic_endpoint = \"node-a-pub\"\n",
    )
    .await?;
    assert_eq!((vec![LEFT.to_owned()], Vec::new()), version(&record));
    Ok(())
}

#[tokio::test]
async fn an_ehr_create_names_no_version() -> TestResult {
    let at = "/v1/ehr";
    let sent = request(&Method::POST, at, false, String::new())?;
    let answer = Answer {
        status: 201,
        headers: vec![("ETag", format!("\"{EHR_A}\""))],
    };
    let (_, record) = recorded(sent, at, answer).await?;
    assert_eq!(
        (Vec::new(), vec!["unnamed".to_owned()]),
        version(&record),
        "ITS-REST 1.1.0: the ETag of an EHR create is the ehr_id, which is no version"
    );
    Ok(())
}

#[tokio::test]
async fn a_tags_write_names_no_version() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/composition/{PRECEDING}/tags");
    let sent = request(&Method::PUT, &at, false, "[]".to_owned())?;
    let answer = Answer {
        status: 204,
        headers: vec![("ETag", format!("\"{LEFT}\""))],
    };
    let (_, record) = recorded(sent, &at, answer).await?;
    assert_eq!((Vec::new(), vec!["unnamed".to_owned()]), version(&record));
    Ok(())
}

#[tokio::test]
async fn a_write_the_node_refused_left_no_version_and_is_a_node_error() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/composition");
    let sent = request(
        &Method::POST,
        &at,
        false,
        composition(LAB_REPORT, PRECEDING),
    )?;
    let (status, record) = recorded(sent, &at, Answer::tagged(422)).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status);
    assert_eq!((Vec::new(), Vec::new()), version(&record));
    assert_eq!(
        details(&record, "ehds-categories", "version-uid"),
        Vec::<String>::new()
    );
    assert_eq!(
        vec!["node-error"],
        details(&record, "origin", "status"),
        "§11.1, N16: a node that answered with a failure is node-error"
    );
    Ok(())
}

#[tokio::test]
async fn an_answer_that_names_a_patient_identifier_in_place_of_a_version_records_none() -> TestResult
{
    let at = format!("/v1/ehr/{EHR_A}/composition");
    let sent = request(
        &Method::POST,
        &at,
        false,
        composition(LAB_REPORT, PRECEDING),
    )?;
    let answer = Answer {
        status: 201,
        headers: vec![
            ("ETag", format!("\"{PATIENT}\"")),
            (
                "Location",
                format!("https://cdr-a.example.org/v1/ehr/{EHR_A}/composition/{PATIENT}"),
            ),
        ],
    };
    let (_, record) = recorded(sent, &at, answer).await?;
    assert_eq!((Vec::new(), vec!["malformed".to_owned()]), version(&record));
    assert!(
        !record.to_string().contains(PATIENT),
        "§5.4, N33: no value the node put in its answer reaches the record as itself"
    );
    Ok(())
}
