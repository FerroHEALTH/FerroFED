// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The record of a routed read and a routed write, and the refusal of every
//! access whose record the spool cannot take: the record names the endpoint
//! that acted and the categories of the `COMPOSITION` read or written, and an
//! access whose record cannot be stored answers `503 access-unrecorded` with
//! no patient data in it (Annex II 3.2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    LAB_CATEGORY, LAB_REPORT, TestResult, accesses, composition, details, gateway, gateway_with,
    named, profile,
};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, body, post, settings_with_room};
use crate::feed_audit::{SETTLE, names_the_default_caller};
use crate::support::call;

const UID: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The targeting header that names node A.
const TARGET: (&str, &str) = ("openEHR-federation-endpoint", "node-a-pub");

/// A node holding the EHR [`EHR_A`] with the lab report [`UID`], answering a
/// read of it, a read of the EHR, a new composition and the federated query.
async fn node() -> Server {
    let server = Server::start().await;
    let json = |text: String| {
        ResponseTemplate::new(200).set_body_raw(text.into_bytes(), "application/json")
    };
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition/{UID}")))
        .respond_with(json(composition(LAB_REPORT, UID)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_A}")))
        .respond_with(json(format!(
            r#"{{"system_id":{{"value":"cdr-a.example.org"}},"ehr_id":{{"value":"{EHR_A}"}}}}"#
        )))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(ResponseTemplate::new(201).insert_header("ETag", format!("\"{UID}\"")))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(json(format!(
            r##"{{"q":"node","columns":[{{"name":"#0","path":"c"}}],"rows":[[{}]]}}"##,
            composition(LAB_REPORT, UID)
        )))
        .mount(&server)
        .await;
    server
}

/// The read of the lab report at node A.
fn read() -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/ehr/{EHR_A}/composition/{UID}"))
        .header(TARGET.0, TARGET.1)
        .body(Body::empty())
}

/// A new composition at node A, `content` in `media`.
fn write(media: &str, content: String) -> Result<Request<Body>, http::Error> {
    Request::post(format!("/v1/ehr/{EHR_A}/composition"))
        .header(TARGET.0, TARGET.1)
        .header(header::CONTENT_TYPE, media)
        .body(Body::from(content))
}

/// A gateway over node A at `node`, and node B under a path of it that
/// answers nothing, recording to `repository` with the `[audit.repository]`
/// keys `extra`.
fn over(
    dir: &std::path::Path,
    node: &Server,
    repository: &FeedRepository,
    extra: &str,
) -> Result<Router, Box<dyn std::error::Error>> {
    let idle = format!("{}/node-b", node.uri());
    gateway(dir, (&node.uri(), &idle), repository, extra)
}

#[tokio::test]
async fn a_routed_read_names_the_endpoint_and_the_category_of_what_it_read() -> TestResult {
    let node = node().await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(over(dir.path(), &node, &repository, "")?, read()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the read, got {records:?}").into());
    };
    assert_eq!(
        profile(record),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Read")
    );
    assert_eq!(record["action"], "R");
    names_the_default_caller(&record.to_string())?;
    assert_eq!(
        vec![format!("{LAB_CATEGORY}:returned")],
        details(record, "ehds-categories", "ehds-category-basis")
    );
    assert_eq!(vec![UID], details(record, "ehds-categories", "version-uid"));
    let origins = named(record, "origin");
    assert_eq!(1, origins.len());
    assert_eq!(origins[0]["what"]["identifier"]["value"], "node-a-pub");
    assert_eq!(vec!["200"], details(record, "origin", "status"));
    assert_eq!(
        vec![LAB_CATEGORY],
        details(record, "origin", "ehds-category"),
        "Annex II 3.4: the category of the one origin"
    );
    let ehrs = named(record, "ehr");
    assert_eq!(
        ehrs.first().map(|ehr| &ehr["what"]["identifier"]["value"]),
        Some(&serde_json::json!(EHR_A))
    );
    Ok(())
}

#[tokio::test]
async fn a_routed_write_records_the_category_of_what_it_wrote() -> TestResult {
    let node = node().await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &node, &repository, "")?;
    let sent = composition(LAB_REPORT, UID);
    let (status, text) = call(app, write("application/json", sent.clone())?).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the write, got {records:?}").into());
    };
    assert_eq!(record["action"], "C");
    assert_eq!(
        vec![format!("{LAB_CATEGORY}:written")],
        details(record, "ehds-categories", "ehds-category-basis")
    );
    let forwarded = crate::facade::received(&node).await?;
    assert!(
        forwarded.iter().any(|request| request.contains(&sent)),
        "N22: the body reaches the node byte for byte"
    );
    Ok(())
}

#[tokio::test]
async fn a_write_in_a_simplified_format_is_recorded_unclassified() -> TestResult {
    let node = node().await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &node, &repository, "")?;
    let flat = r#"{"report/context/start_time":"2026-10-05T10:00:00Z"}"#.to_owned();
    let (status, text) = call(app, write("application/openehr.wt.flat+json", flat)?).await?;
    assert_eq!(StatusCode::CREATED, status, "never refused for it: {text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    assert_eq!(
        vec!["format-not-read"],
        details(record, "ehds-categories", "ehds-unclassified")
    );
    Ok(())
}

#[tokio::test]
async fn a_read_of_the_ehr_is_of_no_category() -> TestResult {
    let node = node().await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let request = Request::get(format!("/v1/ehr/{EHR_A}"))
        .header(TARGET.0, TARGET.1)
        .body(Body::empty())?;
    let (status, text) = call(over(dir.path(), &node, &repository, "")?, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    assert_eq!(
        vec!["true"],
        details(record, "ehds-categories", "ehds-no-category")
    );
    Ok(())
}

/// A synthetic patient identifier with a literal `+`, which RFC 3986 keeps
/// as a plus and HTML form decoding would read as a space.
const PLUS_PATIENT: &str = "Qz7+plus-55";

#[tokio::test]
async fn a_read_by_subject_records_the_patient_the_route_resolved() -> TestResult {
    let node = node().await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let idle = format!("{}/node-b", node.uri());
    let crossref = format!(
        "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PLUS_PATIENT}\"\n\
         member = \"node-a\"\nehr_id = \"{EHR_A}\"\n"
    );
    let app = gateway_with(
        dir.path(),
        (&node.uri(), &idle),
        &repository,
        ("", &crossref),
        &settings_with_room(),
    )?;
    let request = Request::get(format!(
        "/v1/ehr?subject_id={PLUS_PATIENT}&subject_namespace={NAMESPACE}"
    ))
    .body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the route resolves the identifier with its plus: {text}"
    );
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    let patients: Vec<&str> = record["entity"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entity| entity.pointer("/what/identifier/value"))
        .filter_map(serde_json::Value::as_str)
        .filter(|value| value.starts_with("Qz7"))
        .collect();
    assert_eq!(
        vec![PLUS_PATIENT],
        patients,
        "the record names the patient the route resolved, never another decoding of it"
    );
    Ok(())
}

/// A gateway over `node` whose spool holds one record and whose repository
/// is down, its one place taken by a first read, so the next record is
/// refused.
async fn full(
    dir: &std::path::Path,
    node: &Server,
) -> Result<(Router, FeedRepository), Box<dyn std::error::Error>> {
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let app = over(dir, node, &repository, "spool_max_events = 1")?;
    let (status, text) = call(app.clone(), read()?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the first record fills the spool: {text}"
    );
    Ok((app, repository))
}

/// Fails unless the answer `(status, text)` is the refusal of an access
/// whose record could not be stored, with none of the data in it.
fn withheld((status, text): (StatusCode, String)) {
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert!(text.contains("access-unrecorded"), "{text}");
    for value in [UID, LAB_REPORT, "archetype_details"] {
        assert!(
            !text.contains(value),
            "no data leaves without its record: {text}"
        );
    }
}

#[tokio::test]
async fn every_access_whose_record_the_spool_cannot_take_is_refused_with_no_data() -> TestResult {
    let node = node().await;
    let dir = tempfile::tempdir()?;
    let (app, _repository) = full(dir.path(), &node).await?;
    withheld(call(app.clone(), read()?).await?);
    withheld(
        call(
            app.clone(),
            write("application/json", composition(LAB_REPORT, UID))?,
        )
        .await?,
    );
    let query = format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    withheld(call(app.clone(), post(body(&query)?)?).await?);
    let mut console = post(body(&query)?)?;
    console.headers_mut().insert(
        header::AUTHORIZATION,
        crate::support::operator_bearer()?.parse()?,
    );
    withheld(call(app.clone(), console).await?);
    let get = Request::get(format!(
        "/v1/query/aql?q={}",
        url::form_urlencoded::byte_serialize(query.as_bytes())
            .collect::<String>()
            .replace('+', "%20")
    ))
    .body(Body::empty())?;
    withheld(call(app, get).await?);
    Ok(())
}

#[tokio::test]
async fn a_stored_query_whose_record_the_spool_cannot_take_is_refused_with_no_data() -> TestResult {
    let node = node().await;
    let dir = tempfile::tempdir()?;
    let (app, _repository) = full(dir.path(), &node).await?;
    let definition = format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let put = Request::put("/v1/definition/query/org.example::compositions/1.0.0")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(definition))?;
    let (status, text) = call(app.clone(), put).await?;
    assert_eq!(StatusCode::OK, status, "a definition is no access: {text}");
    let invoke = Request::post("/v1/query/org.example::compositions")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"query_parameters":{{"patient":"{PATIENT}"}}}}"#
        )))?;
    withheld(call(app, invoke).await?);
    Ok(())
}
