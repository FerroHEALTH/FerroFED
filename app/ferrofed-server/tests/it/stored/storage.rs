// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Storing a definition: immutability, the refusals at `PUT`, and the ITS-REST forms (CP-40).

use axum::body::Body;
use http::{Request, StatusCode, header};
use openehr_its::rest::generated::definition::StoredQuery;

use crate::facade::{PATIENT, PATIENT_TAIL, node_answering, received};
use crate::support::{call, error_body, send};

use super::{
    NAME, TestResult, bound, get, invoke, parameterised, put, store_file, stored, two_members,
};

// conformance: CP-40
#[tokio::test]
async fn a_second_put_of_a_held_name_and_version_is_refused_and_the_text_stands() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let first = parameterised().replace("$name", "'first'");
    stored(&app, "1.0.0", &first).await?;

    let second = parameterised().replace("$name", "'second'");
    let (status, text) = call(app.clone(), put(NAME, "1.0.0", &second)?).await?;
    assert_eq!(StatusCode::CONFLICT, status, "§12.7, N44: {text}");
    assert_eq!("stored-query-held", error_body(&text)?.code);

    let (status, text) = call(app, get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let held: StoredQuery = serde_json::from_str(&text)?;
    assert!(
        held.q.contains("'first'"),
        "the held text stands: {}",
        held.q
    );
    assert!(!held.q.contains("'second'"), "never applied: {}", held.q);
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_definition_naming_the_patient_by_a_literal_is_refused_at_put() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let literal = parameterised().replace("$patient", &format!("'{PATIENT}'"));

    let (status, text) = call(app.clone(), put(NAME, "1.0.0", &literal)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "N33: {text}");
    assert_eq!("subject-literal", error_body(&text)?.code);
    assert!(!text.contains(PATIENT_TAIL), "§5.4.3: never quoted: {text}");

    let (status, _) = call(app, get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "nothing was stored");
    let bytes = std::fs::read(store_file(dir.path()))?;
    assert!(
        !bytes
            .windows(PATIENT_TAIL.len())
            .any(|window| window == PATIENT_TAIL.as_bytes()),
        "N33: the identifier never reaches the store"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn the_store_holds_no_identifier_after_a_definition_is_stored_and_invoked() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;
    let (status, text) = call(app.clone(), invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    drop(app);

    let bytes = std::fs::read(store_file(dir.path()))?;
    let holds = |needle: &str| {
        bytes
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    };
    assert!(holds(NAME), "the store file holds the definition");
    assert!(holds("$patient"), "the patient is held as its parameter");
    assert!(!holds(PATIENT_TAIL), "N33: no identifier at rest");
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_stored_version_survives_a_restart_and_stays_immutable() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    {
        let app = two_members(dir.path(), &a, &b)?;
        stored(&app, "1.0.0", &parameterised()).await?;
    }
    let restarted = two_members(dir.path(), &a, &b)?;
    let (status, text) = call(restarted.clone(), get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the version outlives the process: {text}"
    );
    let (status, text) = call(restarted.clone(), invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "invocable after the restart: {text}"
    );
    let (status, text) = call(restarted, put(NAME, "1.0.0", &parameterised())?).await?;
    assert_eq!(
        StatusCode::CONFLICT,
        status,
        "§12.7, N44: the refusal holds after a restart: {text}"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_stored_definition_reads_back_as_an_its_rest_stored_query_and_lists() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let response = send(app.clone(), put(NAME, "1.0.0", &parameterised())?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("1.0.0"),
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        "ITS-REST: Location names the stored query, relative to the request"
    );
    stored(&app, "1.1.0", &parameterised()).await?;

    let (status, text) = call(app.clone(), get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let held: StoredQuery = serde_json::from_str(&text)?;
    assert_eq!(NAME, held.name);
    assert_eq!("AQL", held.r#type);
    assert_eq!("1.0.0", held.version);
    assert!(
        held.saved.parse::<jiff::Timestamp>().is_ok(),
        "{}",
        held.saved
    );
    assert!(held.q.contains("$patient"), "{}", held.q);

    let (status, text) = call(app.clone(), get(&format!("{NAME}/1"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let latest: StoredQuery = serde_json::from_str(&text)?;
    assert_eq!("1.1.0", latest.version, "ITS-REST: the highest 1.x");

    let (status, text) = call(app, get("org.example")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let listed: Vec<StoredQuery> = serde_json::from_str(&text)?;
    let versions: Vec<&str> = listed.iter().map(|query| query.version.as_str()).collect();
    assert_eq!(
        vec!["1.0.0", "1.1.0"],
        versions,
        "ITS-REST definition_query_list"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_definition_the_rewrite_refuses_whatever_is_bound_is_refused_at_put() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    for (aql, code) in [
        ("SELECT FROM WHERE", "not-aql"),
        (
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
             WHERE e/ehr_status/subject/external_ref/id/value = $patient OR c/name/value = 'x'",
            "unreducible",
        ),
    ] {
        let (status, text) = call(app.clone(), put(NAME, "1.0.0", aql)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{aql}: {text}");
        assert_eq!(code, error_body(&text)?.code, "{aql}");
    }
    let (status, _) = call(app, get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "nothing was stored");
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_name_a_version_or_a_query_type_outside_its_rest_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let aql = parameterised();
    let unversioned = Request::put(format!("/v1/definition/query/{NAME}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(aql.clone()))?;
    let typed = Request::put(format!("/v1/definition/query/{NAME}/1.0.0?query_type=SQL"))
        .body(Body::from(aql.clone()))?;
    let undeclared = Request::put(format!("/v1/definition/query/{NAME}/1.0.0?endpoint=x"))
        .body(Body::from(aql.clone()))?;
    for (request, status, code) in [
        (
            put("ns::aql", "1.0.0", &aql)?,
            StatusCode::BAD_REQUEST,
            "query-name-invalid",
        ),
        (
            put("a%20b", "1.0.0", &aql)?,
            StatusCode::BAD_REQUEST,
            "query-name-invalid",
        ),
        (
            put(NAME, "1.0", &aql)?,
            StatusCode::BAD_REQUEST,
            "query-version-invalid",
        ),
        (
            put(NAME, "01.0.0", &aql)?,
            StatusCode::BAD_REQUEST,
            "query-version-invalid",
        ),
        (
            unversioned,
            StatusCode::BAD_REQUEST,
            "query-version-required",
        ),
        (typed, StatusCode::BAD_REQUEST, "query-type-unsupported"),
        (
            undeclared,
            StatusCode::BAD_REQUEST,
            "query-parameter-refused",
        ),
        (
            get(&format!("{NAME}/x"))?,
            StatusCode::BAD_REQUEST,
            "query-version-invalid",
        ),
        (
            invoke("unknown", "{}", &[])?,
            StatusCode::NOT_FOUND,
            "stored-query-unknown",
        ),
    ] {
        let (answered, text) = call(app.clone(), request).await?;
        assert_eq!(status, answered, "{text}");
        assert_eq!(code, error_body(&text)?.code, "{text}");
    }
    let typed = Request::put(format!("/v1/definition/query/{NAME}/1.0.0?query_type=aql"))
        .body(Body::from(aql))?;
    let (status, text) = call(app, typed).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the ITS-REST default, in any case: {text}"
    );
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_store_query_string_the_generated_decoder_refuses_stores_nothing() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    for query in ["query_type=%FF", "query_type=AQL&query_type=AQL"] {
        let request = Request::put(format!("/v1/definition/query/{NAME}/1.0.0?{query}"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(parameterised()))?;
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(
            StatusCode::BAD_REQUEST,
            status,
            "{query}: no UTF-8 text, or a scalar given twice (ITS-REST, RFC 3986 §2.1): {text}"
        );
        assert_eq!("body-invalid", error_body(&text)?.code, "{query}");
    }
    let (status, _) = call(app, get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "nothing was stored");
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}
