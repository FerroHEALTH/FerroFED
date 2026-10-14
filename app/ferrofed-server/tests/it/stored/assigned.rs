// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A definition `PUT` with no version: the registry stores it at the version
//! it assigns, `1.0.0` for a new name and the next patch of the newest
//! version by number otherwise, names that version in `Location`, and never
//! replaces a held version (§12.7 stored-query-versioning, N44, CP-40;
//! ITS-REST Definition API, `definition_query_store`). The literal-patient
//! refusal applies before any version is assigned (§5.4.1, N33), the
//! reserved namespace stays closed, and a distribution sends the assigned
//! version (§12.7 stored-query-fanout). The read-only backend's `405` for the
//! same `PUT` is held in the `files` module.

use axum::Router;
use axum::body::Body;
use ferrofed_eehrxf::reserved;
use http::{Request, StatusCode, header};
use openehr_its::rest::generated::definition::StoredQuery;
use wiremock::ResponseTemplate;

use crate::facade::{PATIENT, PATIENT_TAIL, node_answering, received};
use crate::stored_fan_out::{self, offered, storing};
use crate::support::{asked, call, error_body, exchange, field, mount};

use super::{NAME, TestResult, get, parameterised, store_file, stored, two_members};

/// `PUT {base}/v1/definition/query/{name}` with the AQL `aql`.
fn unversioned(name: &str, aql: &str) -> Result<Request<Body>, http::Error> {
    Request::put(format!("/v1/definition/query/{name}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(aql.to_owned()))
}

/// Stores `aql` at [`NAME`] with no version, and returns the `Location` the
/// registry answers with.
async fn assigned(app: &Router, aql: &str) -> Result<String, Box<dyn std::error::Error>> {
    let (status, headers, body) = exchange(app.clone(), unversioned(NAME, aql)?).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::OK, status, "§12.7: stored: {text}");
    Ok(field(&headers, "location").ok_or("a Location")?.to_owned())
}

/// The AQL the registry holds at [`NAME`] and `version`.
async fn held_at(app: &Router, version: &str) -> Result<String, Box<dyn std::error::Error>> {
    let (status, text) = call(app.clone(), get(&format!("{NAME}/{version}"))?).await?;
    assert_eq!(StatusCode::OK, status, "{version}: {text}");
    Ok(serde_json::from_str::<StoredQuery>(&text)?.q)
}

/// [`parameterised`] with the composition name `marker`.
fn marked(marker: &str) -> String {
    parameterised().replace("$name", &format!("'{marker}'"))
}

// conformance: CP-40
#[tokio::test]
async fn a_new_name_is_stored_at_one_zero_zero_and_named_in_location() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let location = assigned(&app, &marked("first")).await?;
    assert_eq!(
        format!("./{NAME}/1.0.0"),
        location,
        "RFC 3986 §4.2: relative to the request path, the name kept from reading as a scheme"
    );
    assert!(held_at(&app, "1.0.0").await?.contains("'first'"));
    assert!(
        received(&a).await?.is_empty() && received(&b).await?.is_empty(),
        "no node is asked"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn the_next_patch_follows_the_newest_version_by_number_and_none_is_replaced() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.3", &marked("held-1.0.3")).await?;
    assert_eq!(
        format!("./{NAME}/1.0.4"),
        assigned(&app, &marked("a")).await?
    );
    stored(&app, "1.2.0", &marked("held-1.2.0")).await?;
    assert_eq!(
        format!("./{NAME}/1.2.1"),
        assigned(&app, &marked("b")).await?
    );
    stored(&app, "1.9.0", &marked("held-1.9.0")).await?;
    stored(&app, "1.10.0", &marked("held-1.10.0")).await?;
    assert_eq!(
        format!("./{NAME}/1.10.1"),
        assigned(&app, &marked("c")).await?,
        "semantic version order: 1.10.0 is newer than 1.9.0"
    );
    for (version, marker) in [
        ("1.0.3", "held-1.0.3"),
        ("1.0.4", "a"),
        ("1.2.0", "held-1.2.0"),
        ("1.2.1", "b"),
        ("1.9.0", "held-1.9.0"),
        ("1.10.0", "held-1.10.0"),
        ("1.10.1", "c"),
    ] {
        assert!(
            held_at(&app, version)
                .await?
                .contains(&format!("'{marker}'")),
            "N44: {version} holds what it was stored with"
        );
    }
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_literal_patient_is_refused_before_any_version_is_assigned() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let literal = parameterised().replace("$patient", &format!("'{PATIENT}'"));
    let (status, text) = call(app.clone(), unversioned(NAME, &literal)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "N33: {text}");
    assert_eq!("subject-literal", error_body(&text)?.code);
    assert!(!text.contains(PATIENT_TAIL), "§5.4.3: never quoted: {text}");
    let (status, text) = call(app.clone(), get(NAME)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        0,
        serde_json::from_str::<Vec<StoredQuery>>(&text)?.len(),
        "nothing was stored"
    );
    assert_eq!(
        format!("./{NAME}/1.0.0"),
        assigned(&app, &marked("after")).await?,
        "the refusal took no version"
    );
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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_unversioned_puts_each_store_a_version_of_their_own() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let writers = 12;
    let mut tasks = tokio::task::JoinSet::new();
    for writer in 0..writers {
        let (app, aql) = (app.clone(), marked(&format!("writer-{writer}")));
        tasks.spawn(async move {
            let request = unversioned(NAME, &aql).map_err(|failed| failed.to_string())?;
            exchange(app, request)
                .await
                .map_err(|failed| failed.to_string())
        });
    }
    let mut locations = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let (status, headers, body) = joined??;
        assert_eq!(StatusCode::OK, status, "{}", String::from_utf8(body)?);
        locations.push(field(&headers, "location").ok_or("a Location")?.to_owned());
    }
    locations.sort();
    locations.dedup();
    assert_eq!(writers, locations.len(), "§12.7, N44: no version is shared");
    let (status, text) = call(app, get(NAME)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let listed: Vec<StoredQuery> = serde_json::from_str(&text)?;
    let mut versions: Vec<String> = listed.iter().map(|held| held.version.clone()).collect();
    versions.sort();
    let mut expected: Vec<String> = (0..writers).map(|patch| format!("1.0.{patch}")).collect();
    expected.sort();
    assert_eq!(expected, versions, "one patch each, from 1.0.0");
    let mut texts: Vec<String> = listed.into_iter().map(|held| held.q).collect();
    texts.sort();
    texts.dedup();
    assert_eq!(writers, texts.len(), "every writer's own text is held");
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn an_unversioned_put_in_the_reserved_namespace_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let name = format!("{}::mine", reserved::NAMESPACE);
    let (status, text) = call(app.clone(), unversioned(&name, &parameterised())?).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("stored-query-reserved", error_body(&text)?.code);
    let (status, text) = call(app, get(&format!("{name}/1.0.0"))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "nothing was stored: {text}");
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_distribution_sends_the_version_the_registry_assigned() -> TestResult {
    let (a, b, c) = (storing(200).await, storing(200).await, storing(200).await);
    let path = |version: &str| format!("/v1/definition/query/{}/{version}", stored_fan_out::NAME);
    for node in [&a, &b, &c] {
        mount(node, "PUT", path("1.0.1"), ResponseTemplate::new(200)).await;
    }
    let dir = tempfile::tempdir()?;
    let app = offered(dir.path(), [&a, &b, &c])?;
    for assigned in ["1.0.0", "1.0.1"] {
        let request = Request::put(format!("/v1/definition/query/{}", stored_fan_out::NAME))
            .header(header::CONTENT_TYPE, "text/plain")
            .header(stored_fan_out::ENDPOINT, "*")
            .body(Body::from(stored_fan_out::definition()))?;
        let (status, headers, body) = exchange(app.clone(), request).await?;
        let text = String::from_utf8(body)?;
        assert_eq!(StatusCode::OK, status, "{assigned}: {text}");
        let answer: stored_fan_out::Reported = serde_json::from_str(&text)?;
        assert_eq!(assigned, answer.definition.version, "{text}");
        assert_eq!(Some("stored"), answer.meta.registry.as_deref());
        assert_eq!(
            Some(format!("./{}/{assigned}", stored_fan_out::NAME).as_str()),
            field(&headers, "location"),
            "the assigned version"
        );
    }
    for node in [&a, &b, &c] {
        assert_eq!(
            vec![
                ("PUT".to_owned(), path("1.0.0")),
                ("PUT".to_owned(), path("1.0.1")),
            ],
            asked(node).await?,
            "§12.7 stored-query-fanout: each member is sent the version the registry stored"
        );
    }
    Ok(())
}
