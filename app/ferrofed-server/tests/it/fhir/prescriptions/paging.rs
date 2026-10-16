// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Paging (FHIR R4 `http.html` §3.1.0.14): a member's `next` link is
//! followed under its own base and nowhere else, and the gateway pages its
//! own answer by `_count` (FHIR R4 search §3.1.1.5.3).

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::Mock;
use wiremock::matchers::{method, path, query_param};

use super::{Members, RX_A, RX_A2, RX_B, entries, own, page, search, searchset};
use crate::facade::{PATIENT, PATIENT_TAIL};
use crate::fhir::TestResult;
use crate::support::call;

#[tokio::test]
async fn a_members_next_page_is_followed_under_its_own_base() -> TestResult {
    let own_b = own(page(searchset("https://cdr-b.example.org", &[RX_B], None))).await;
    let members = Members::over(Server::start().await, own_b).await?;
    let next = format!(
        "{}/fhir/MedicationRequest?page=2&state=synthetic",
        members.proxy_a.origin()
    );
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationRequest"))
        .and(query_param("page", "2"))
        .respond_with(page(searchset("https://cdr-a.example.org", &[RX_A2], None)))
        .with_priority(1)
        .mount(&members.own_a)
        .await;
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationRequest"))
        .respond_with(page(searchset(
            "https://cdr-a.example.org",
            &[RX_A],
            Some(&next),
        )))
        .mount(&members.own_a)
        .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(members.both(dir.path())?, search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(bundle["total"], Value::from(3), "{bundle}");
    assert!(
        text.contains(RX_A2),
        "the second page's prescription: {text}"
    );
    let journal = members.proxy_a.journal();
    let [_, second] = journal.as_slice() else {
        return Err(format!("two pages asked, got {}", journal.len()).into());
    };
    assert_eq!(second.path, "/fhir/MedicationRequest");
    assert_eq!(
        second.query.as_deref(),
        Some("page=2&state=synthetic"),
        "FHIR R4 http.html §3.1.0.14: the member's own link, as it wrote it"
    );
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !members.proxy_a.journal_contains(withheld.as_bytes()),
            "N33"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_next_link_outside_the_members_base_fails_the_search_and_is_not_followed() -> TestResult {
    let members = Members::start(
        page(searchset(
            "https://cdr-a.example.org",
            &[RX_A],
            Some("https://elsewhere.example.org/fhir/MedicationRequest?page=2"),
        )),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(members.both(dir.path())?, search("")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.4: {text}");
    assert!(text.contains("node-a-mpd (node-error)"), "{text}");
    assert_eq!(1, members.proxy_a.journal().len(), "one page asked");
    Ok(())
}

#[tokio::test]
async fn the_gateway_pages_its_answer_by_count() -> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    let (status, text) = call(app.clone(), search("&_count=1")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let first: Value = serde_json::from_str(&text)?;
    assert_eq!(first["total"], Value::from(2), "every match is counted");
    let matched = entries(&first, "match");
    assert_eq!(1, matched.len(), "FHIR R4 search §3.1.1.5.3: {first}");
    assert_eq!(
        matched.first().map(|resource| &resource["id"]),
        Some(&Value::from("rx-a1"))
    );
    let next = first["link"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|link| link["relation"] == "next")
        .and_then(|link| link["url"].as_str())
        .ok_or("a next link")?;
    let path = url::Url::parse(next)?;
    let following = format!("{}?{}", path.path(), path.query().unwrap_or_default());
    let (status, text) = call(app, Request::get(following).body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let second: Value = serde_json::from_str(&text)?;
    let matched = entries(&second, "match");
    assert_eq!(
        matched.first().map(|resource| &resource["id"]),
        Some(&Value::from("rx-b1"))
    );
    assert!(
        !second["link"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|link| link["relation"] == "next"),
        "the last page has no next link: {second}"
    );
    assert!(text.contains(RX_B), "{text}");
    Ok(())
}
