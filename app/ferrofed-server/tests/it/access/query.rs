// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The record of a federated query and of a stored-query execution: one per
//! query that reached a node, naming the caller, the patient, every `ehr_id`
//! and endpoint, and the categories of the delivered rows or, with none, of
//! what the query constrains its data to (Annex II 3.2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use axum::body::Body;
use http::{Request, StatusCode, header};

use super::{
    ADMIN, DISCHARGE, DISCHARGE_CATEGORY, LAB_ARCHETYPE, LAB_CATEGORY, LAB_REPORT, TestResult,
    UNMAPPED, accesses, composition, details, gateway, gateway_and_state, gateway_with, named,
    node_with_rows, profile, recorded_the_query,
};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, body, post, settings_with_room};
use crate::feed_audit::{SETTLE, names_the_default_caller};
use crate::support::call;
use ferrofed_testkit::atna_feed::FeedRepository;

const UID_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";
const UID_B: &str = "5c3e9b1a-7d2f-4e8a-9b6c-1f0e2d3c4b5a::cdr-b.example.org::1";

/// The patient query selecting whole compositions, `extra` added to its
/// `FROM`.
fn compositions(extra: &str) -> String {
    format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c{extra} \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// Runs `aql` over a gateway whose node A answers `a` and node B answers `b`,
/// and returns the status, the answer and the access records.
async fn run(
    aql: &str,
    a: &[String],
    b: &[String],
) -> Result<(StatusCode, String, Vec<serde_json::Value>), Box<dyn std::error::Error>> {
    let (node_a, node_b) = (node_with_rows(a).await, node_with_rows(b).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&node_a.uri(), &node_b.uri()), &repository, "")?;
    let (status, text) = call(app, post(body(aql)?)?).await?;
    let records = repository.wait_for(1, SETTLE).await;
    Ok((status, text, accesses(&records)?))
}

#[tokio::test]
async fn a_patient_query_names_the_caller_the_patient_the_origins_and_every_category() -> TestResult
{
    let (status, text, records) = run(
        &compositions(""),
        &[composition(LAB_REPORT, UID_A)],
        &[composition(DISCHARGE, UID_B)],
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, records.len(), "one record of the access");
    let record = &records[0];
    assert_eq!(
        profile(record),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientQuery")
    );
    names_the_default_caller(&record.to_string())?;
    let entities = record["entity"].as_array().ok_or("entities")?;
    assert!(
        entities
            .iter()
            .any(|entity| entity["what"]["identifier"]["value"] == PATIENT),
        "the data subject reaches the repository"
    );
    let mut categories = details(record, "ehds-categories", "ehds-category");
    categories.sort();
    assert_eq!(
        vec![DISCHARGE_CATEGORY, LAB_CATEGORY],
        categories,
        "Annex II 3.2(c): a query spanning categories records each"
    );
    assert_eq!(vec!["2"], details(record, "ehds-categories", "delivered"));
    let mut versions = details(record, "ehds-categories", "version-uid");
    versions.sort();
    assert_eq!(vec![UID_B, UID_A], versions, "Art 9(2)(c): which data");
    let origins: Vec<&str> = named(record, "origin")
        .iter()
        .filter_map(|origin| origin["what"]["identifier"]["value"].as_str())
        .collect();
    assert_eq!(vec!["node-a-pub", "node-b-pub"], origins, "Annex II 3.2(e)");
    let ehrs: Vec<&str> = named(record, "ehr")
        .iter()
        .filter_map(|ehr| ehr["what"]["identifier"]["value"].as_str())
        .collect();
    assert_eq!(vec![EHR_A, EHR_B], ehrs);
    assert!(
        details(record, "ehds-categories", "category-map-digest")
            .first()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "the map the record was classified under"
    );
    Ok(())
}

#[tokio::test]
async fn a_query_with_no_row_is_recorded_with_what_it_queried() -> TestResult {
    let (status, text, records) = run(
        &compositions(&format!(" CONTAINS OBSERVATION o[{LAB_ARCHETYPE}]")),
        &[],
        &[],
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let [record] = records.as_slice() else {
        return Err(format!("one record of a query with no row, got {records:?}").into());
    };
    assert_eq!(
        vec![LAB_CATEGORY],
        details(record, "ehds-categories", "ehds-category")
    );
    assert_eq!(
        vec![format!("{LAB_CATEGORY}:queried")],
        details(record, "ehds-categories", "ehds-category-basis")
    );
    assert_eq!(vec!["0"], details(record, "ehds-categories", "delivered"));
    Ok(())
}

#[tokio::test]
async fn an_unmapped_template_is_unclassified_with_its_id_and_answered() -> TestResult {
    let (status, text, records) =
        run(&compositions(""), &[composition(UNMAPPED, UID_A)], &[]).await?;
    assert_eq!(StatusCode::OK, status, "never refused for it: {text}");
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    assert_eq!(
        vec!["unmapped"],
        details(record, "ehds-categories", "ehds-unclassified")
    );
    assert!(details(record, "ehds-categories", "unmapped-id").contains(&UNMAPPED.to_owned()));
    assert!(details(record, "ehds-categories", "ehds-category").is_empty());
    Ok(())
}

#[tokio::test]
async fn a_template_declared_none_is_of_no_category() -> TestResult {
    let (_, _, records) = run(&compositions(""), &[composition(ADMIN, UID_A)], &[]).await?;
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    assert_eq!(
        vec!["true"],
        details(record, "ehds-categories", "ehds-no-category")
    );
    assert!(details(record, "ehds-categories", "ehds-unclassified").is_empty());
    Ok(())
}

#[tokio::test]
async fn a_leaf_read_through_an_archetype_predicate_takes_that_archetypes_category() -> TestResult {
    let aql = format!(
        "SELECT c/content[{LAB_ARCHETYPE}]/data/events/data/items/value/magnitude \
         FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let (status, text, records) = run(&aql, &["7".to_owned()], &[]).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    assert_eq!(
        vec![LAB_CATEGORY],
        details(record, "ehds-categories", "ehds-category"),
        "Annex II 3.2(c): the value is lab data, read through its archetype"
    );
    assert_eq!(
        vec!["unbound"],
        details(record, "ehds-categories", "ehds-unclassified"),
        "the composition itself is bound to no id"
    );
    Ok(())
}

#[tokio::test]
async fn leaf_values_of_an_unbound_class_are_unclassified_never_none() -> TestResult {
    let aql = format!(
        "SELECT c/name/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let (_, _, records) = run(&aql, &["\"Visit\"".to_owned()], &[]).await?;
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    assert!(details(record, "ehds-categories", "ehds-no-category").is_empty());
    assert_eq!(
        vec!["named-nothing"],
        details(record, "ehds-categories", "ehds-unclassified")
    );
    Ok(())
}

#[tokio::test]
async fn a_stored_query_execution_is_recorded_under_its_name() -> TestResult {
    let (node_a, node_b) = (
        node_with_rows(&[composition(LAB_REPORT, UID_A)]).await,
        node_with_rows(&[]).await,
    );
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&node_a.uri(), &node_b.uri()), &repository, "")?;
    let definition = format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let put = Request::put("/v1/definition/query/org.example::compositions/1.0.0")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(definition))?;
    let (status, text) = call(app.clone(), put).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let invoke = Request::post("/v1/query/org.example::compositions")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"query_parameters":{{"patient":"{PATIENT}"}}}}"#
        )))?;
    let (status, text) = call(app, invoke).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the execution, got {records:?}").into());
    };
    assert_eq!(
        vec!["org.example::compositions"],
        details(record, "ehds-categories", "stored-query")
    );
    assert_eq!(
        vec![LAB_CATEGORY],
        details(record, "ehds-categories", "ehds-category")
    );
    Ok(())
}

#[tokio::test]
async fn a_console_query_names_the_operator() -> TestResult {
    let (node_a, node_b) = (
        node_with_rows(&[composition(LAB_REPORT, UID_A)]).await,
        node_with_rows(&[]).await,
    );
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&node_a.uri(), &node_b.uri()), &repository, "")?;
    let operator = "Qz7-operator-77";
    let mut request = post(body(&compositions(""))?)?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        crate::support::bearer_as(operator)?.parse()?,
    );
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    let named = crate::feed_audit::named_user(&record.to_string())?;
    assert_eq!(
        Some(operator),
        named.users.first().and_then(|user| user.subject.as_deref()),
        "Annex II 3.2(b): the person who ran the query from the console"
    );
    Ok(())
}

#[tokio::test]
async fn a_query_no_node_was_sent_is_not_an_access() -> TestResult {
    let (node_a, node_b) = (node_with_rows(&[]).await, node_with_rows(&[]).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&node_a.uri(), &node_b.uri()), &repository, "")?;
    let other = compositions("").replace(PATIENT, "Qz7-unknown-patient");
    let (status, text) = call(app, post(body(&other)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        accesses(&repository.records())?.is_empty(),
        "no member knows the patient, so no data was reached"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_answer_past_the_read_bound_is_recorded_as_node_error_with_no_row() -> TestResult {
    let large: Vec<String> = (0..8).map(|_| composition(LAB_REPORT, UID_A)).collect();
    let (node_a, node_b) = (
        node_with_rows(&large).await,
        node_with_rows(&[composition(DISCHARGE, UID_B)]).await,
    );
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", "max_node_answer_bytes = 4096\n"),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, post(body(&compositions(""))?)?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "§11.4: the oversized member fails the query, never access-unrecorded: {text}"
    );
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(
            format!("one record of the query both members were sent, got {records:?}").into(),
        );
    };
    assert_eq!(
        vec!["node-error", "active"],
        details(record, "origin", "status"),
        "§11.1: the record names member A's refused answer"
    );
    assert_eq!(
        vec!["0"],
        details(record, "ehds-categories", "delivered"),
        "no row left the gateway"
    );
    assert!(
        !details(record, "ehds-categories", "version-uid").contains(&UID_A.to_owned()),
        "the oversized answer delivered no data"
    );
    Ok(())
}

// conformance: track-10
#[tokio::test]
async fn no_patient_template_or_caller_reaches_the_log_a_metric_or_a_node() -> TestResult {
    let (node_a, node_b) = (
        node_with_rows(&[composition(LAB_REPORT, UID_A)]).await,
        node_with_rows(&[composition(DISCHARGE, UID_B)]).await,
    );
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let (app, state) = gateway_and_state(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", ""),
        &settings_with_room(),
    )?;
    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "trace",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app, post(body(&compositions(""))?)?).await?;
    let records = repository.wait_for(1, SETTLE).await;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, accesses(&records)?.len());
    let claims = crate::support::claims();
    let log = logs.text();
    let exposition = state.metrics().render()?;
    recorded_the_query(&exposition, "node-a-pub")?;
    for value in [
        PATIENT,
        LAB_REPORT,
        DISCHARGE,
        claims.sub.as_str(),
        claims.client_id.as_str(),
    ] {
        assert!(!log.contains(value), "N33: {value} in the log: {log}");
        assert!(!exposition.contains(value), "{value} in a metric");
    }
    for node in [&node_a, &node_b] {
        for request in crate::facade::received(node).await? {
            assert!(!request.contains(PATIENT), "N33: {request}");
            assert!(!request.contains("ehds"), "no record reaches a node");
        }
    }
    Ok(())
}
