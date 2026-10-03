// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stored definition invoked by name: binding, versions, targeting, a failing member.

use axum::body::Body;
use http::{Request, StatusCode, header};

use crate::directive::{EHR_C, Nodes};
use crate::facade::{
    Answer, EHR_A, EHR_B, PATIENT, PATIENT_TAIL, node_answering, received, schema, statuses, wire,
};
use crate::support::{call, error_body};

use super::{NAME, Named, TestResult, bound, invoke, parameterised, stored, two_members};

// conformance: CP-40
#[tokio::test]
async fn a_stored_query_invoked_by_name_fans_out_and_names_the_gateways_definition() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;

    let (status, text) = call(app, invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let named: Named = serde_json::from_str(&text)?;
    assert_eq!(
        Some(NAME),
        named.name.as_deref(),
        "§12.7, N44: the name of the gateway's definition"
    );
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "CP-40: meta.federation.endpoints[] covers both members"
    );
    let mut rows = answer.rows.clone();
    rows.sort();
    assert_eq!(
        vec![
            vec!["uid-at-a::cdr-a.example.org::1".to_owned()],
            vec!["uid-at-b::cdr-b.example.org::1".to_owned()],
        ],
        rows,
        "CP-40: rows from more than one member"
    );
    for node in [&a, &b] {
        let captured = wire(node).await?;
        assert!(!captured.is_empty(), "each member was asked");
        assert!(
            !captured.contains(PATIENT_TAIL),
            "N33: no node receives the identifier: {captured}"
        );
    }
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn query_parameters_bind_into_the_stored_query_as_if_submitted_inline() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;
    let (status, text) = call(app.clone(), invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");

    let inline = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let other = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let elsewhere = tempfile::tempdir()?;
    let adhoc = two_members(elsewhere.path(), &inline, &other)?;
    let body = format!(
        r#"{{"q":{},"query_parameters":{{"patient":"{PATIENT}","name":"Visit"}}}}"#,
        serde_json::to_string(&parameterised())?
    );
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))?;
    let (status, text) = call(adhoc, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        received(&inline).await?,
        received(&a).await?,
        "§12.7: exactly as if the client had submitted the text inline"
    );
    let dispatched = received(&a).await?.concat();
    assert!(dispatched.contains("'Visit'"), "{dispatched}");
    assert!(
        dispatched.contains(EHR_A),
        "§7.1: scoped to the node's ehr_id"
    );

    let (status, text) = call(app, invoke(NAME, "{}", &[])?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(
        "parameters",
        error_body(&text)?.code,
        "an unbound parameter is refused by name"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn no_version_runs_the_latest_and_a_prefix_runs_the_highest_it_matches() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    for (version, marker) in [
        ("1.2.0", "one-two"),
        ("1.10.0", "one-ten"),
        ("2.0.0", "two"),
    ] {
        stored(
            &app,
            version,
            &parameterised().replace("$name", &format!("'{marker}'")),
        )
        .await?;
    }
    let body = format!(r#"{{"query_parameters":{{"patient":"{PATIENT}"}}}}"#);
    for (path, marker) in [
        (NAME.to_owned(), "two"),
        (format!("{NAME}/1"), "one-ten"),
        (format!("{NAME}/1.2"), "one-two"),
        (format!("{NAME}/1.10.0"), "one-ten"),
    ] {
        let (status, text) = call(app.clone(), invoke(&path, &body, &[])?).await?;
        assert_eq!(StatusCode::OK, status, "{path}: {text}");
        let answer: Answer = serde_json::from_str(&text)?;
        assert!(
            answer.q.contains(marker),
            "ITS-REST: {path} runs the version with {marker}: {}",
            answer.q
        );
        let named: Named = serde_json::from_str(&text)?;
        assert_eq!(Some(NAME), named.name.as_deref(), "{path}");
    }
    let (status, text) = call(app, invoke(&format!("{NAME}/3"), &body, &[])?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!("stored-query-unknown", error_body(&text)?.code);
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn a_stored_query_is_targeted_by_the_header_as_by_the_directive() -> TestResult {
    let everywhere = [("node-a", EHR_A), ("node-b", EHR_B), ("node-c", EHR_C)];
    let plain = parameterised();
    let directed = plain.replace(
        "FROM EHR e",
        r#"FROM ENDPOINT p ["node-a-pub", "node-b-pub"] CONTAINS EHR e"#,
    );
    let body = bound();

    let by_header = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = by_header.gateway_with_store(dir.path(), &everywhere)?;
    stored(&app, "1.0.0", &plain).await?;
    let header = [("openEHR-federation-endpoint", "node-a-pub, node-b-pub")];
    let (status, text) = call(app, invoke(NAME, &body, &header)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let header: Answer = serde_json::from_str(&text)?;

    let by_directive = Nodes::start().await;
    let other = tempfile::tempdir()?;
    let app = by_directive.gateway_with_store(other.path(), &everywhere)?;
    stored(&app, "1.0.0", &directed).await?;
    let (status, text) = call(app.clone(), invoke(NAME, &body, &[])?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let directive: Answer = serde_json::from_str(&text)?;

    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "excluded"),
        ],
        statuses(&header),
        "§8.4: the header selects the node set of a stored query"
    );
    assert_eq!(
        statuses(&directive),
        statuses(&header),
        "CP-28: the same stored query, by header and by directive"
    );
    assert_eq!(directive.rows.len(), header.rows.len());
    assert_eq!([1, 1, 0], by_header.asked().await?);
    assert_eq!([1, 1, 0], by_directive.asked().await?);

    let conflict = [("openEHR-federation-endpoint", "node-c-pub")];
    let (status, text) = call(app, invoke(NAME, &body, &conflict)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "§8.4.1, N35: {text}");
    assert_eq!("targeting-conflict", error_body(&text)?.code);
    assert_eq!(
        [1, 1, 0],
        by_directive.asked().await?,
        "nothing more is dispatched"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_failing_member_fails_the_stored_query_and_the_envelope_still_names_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = crate::facade::node_failing(500).await;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;
    let (status, text) = call(app, invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.4: {text}");
    schema::validate(&text)?;
    let named: Named = serde_json::from_str(&text)?;
    assert_eq!(Some(NAME), named.name.as_deref(), "§12.7: the name stands");
    Ok(())
}
