// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored queries against a stub gateway: the list grouped by namespace
//! with no AQL in it, the detail of one query with its versions and its
//! highlighted AQL, the link to the query console that names the query and
//! never its text, and the escaping of every value the server renders into
//! the page (the Leptos book, `view/01_basic_component`).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use http::StatusCode;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, console, get, get_as, header, send, signed_in};
use crate::views::options_example;

/// One held version as the operator surface lists it.
fn held(name: &str, version: &str, saved: &str, aql: &str) -> String {
    let text = |value: &str| serde_json::to_string(value).unwrap_or_default();
    format!(
        r#"{{"name":{},"type":"AQL","version":{},"saved":{},"q":{}}}"#,
        text(name),
        text(version),
        text(saved),
        text(aql)
    )
}

/// Answers the page of the stored-query listing at `offset` with `items`,
/// of `total` in all, for the operator's own token.
async fn page(gateway: &MockServer, offset: u64, items: &[String], total: u64) {
    Mock::given(method("GET"))
        .and(path("/operator/stored-queries"))
        .and(query_param("offset", offset.to_string().as_str()))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(format!(
                    r#"{{"items":[{}],"offset":{offset},"total":{total},"replica":"ferrofed-0"}}"#,
                    items.join(",")
                )),
        )
        .mount(gateway)
        .await;
}

/// A stub gateway whose registry holds `items` over two pages, the first
/// `split` of them on the first, and the self-description the top bar reads.
async fn gateway(items: &[String], split: usize) -> Result<MockServer, Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(options_example()?),
        )
        .mount(&gateway)
        .await;
    let total = u64::try_from(items.len())?;
    let (first, second) = items.split_at(split.min(items.len()));
    page(&gateway, 0, first, total).await;
    if !second.is_empty() {
        page(&gateway, u64::try_from(split)?, second, total).await;
    }
    Ok(gateway)
}

/// A console over `gateway` with one signed-in operator.
fn signed_in_console(
    gateway: &MockServer,
) -> Result<(axum::Router, ferrofed_viewer::session::SessionId), Box<dyn Error>> {
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{}\"\n\n[session]\nsecure_cookie = false\n",
        gateway.uri()
    ))?;
    let session = state.sessions().establish(signed_in())?;
    Ok((service, session))
}

/// Three versions of one query, read over two pages, and one other query.
fn three_versions() -> Vec<String> {
    vec![
        held(
            "org.example::weights",
            "1.0.0",
            "2026-10-01T08:00:00Z",
            "SELECT o FROM EHR e CONTAINS OBSERVATION o",
        ),
        held(
            "org.example::weights",
            "1.0.10",
            "2026-10-03T08:00:00Z",
            "SELECT o/data[at0001]/events[at0002]/data/items[at0004]/value/magnitude AS weight \
             FROM EHR e CONTAINS OBSERVATION o WHERE e/ehr_id/value = $ehr_id",
        ),
        held(
            "org.example::weights",
            "1.0.2",
            "2026-10-02T08:00:00Z",
            "SELECT c FROM EHR e CONTAINS COMPOSITION c",
        ),
        held(
            "org.example::zeta",
            "1.0.0",
            "2026-10-04T08:00:00Z",
            "SELECT 1",
        ),
    ]
}

#[tokio::test]
async fn the_detail_shows_the_newest_version_and_reads_every_page() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(&three_versions(), 2).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(
        &service,
        get_as("/stored-queries/org.example::weights", &session)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<title>org.example::weights · FerroFED Console</title>"),
        "{body}"
    );
    assert!(
        body.contains("Version 1.0.10, stored 2026-10-03T08:00:00Z."),
        "{body}"
    );
    assert!(
        body.contains(r#"<span class="tok-keyword">SELECT</span>"#),
        "{body}"
    );
    assert!(
        body.contains(r#"<span class="tok-parameter">$ehr_id</span>"#),
        "{body}"
    );
    assert!(
        body.contains(r#"<span class="tok-path">/data[at0001]/events[at0002]/data/items[at0004]/value/magnitude</span>"#),
        "{body}"
    );
    let switch = body
        .split_once(r#"<nav aria-label="Versions">"#)
        .and_then(|(_, rest)| rest.split_once("</nav>"))
        .map(|(switch, _)| switch)
        .ok_or("the detail carries the version switch")?;
    let order: Vec<usize> = ["1.0.10", "1.0.2", "1.0.0"]
        .iter()
        .filter_map(|version| switch.find(&format!("?version={version}\"")))
        .collect();
    assert_eq!(3, order.len(), "{switch}");
    assert!(order.is_sorted(), "newest first: {switch}");
    let newest = switch
        .split_once(r#"?version=1.0.10""#)
        .and_then(|(_, rest)| rest.split_once('>'))
        .map(|(attributes, _)| attributes)
        .ok_or("the switch links the newest version")?;
    assert!(newest.contains(r#"aria-current="page""#), "{switch}");
    assert_eq!(
        1,
        switch.matches(r#"aria-current="page""#).count(),
        "{switch}"
    );
    assert!(
        body.contains(r#"href="/query?stored=org.example::weights&amp;version=1.0.10""#),
        "{body}"
    );
    assert!(!body.contains("SELECT 1"), "another query's text: {body}");
    Ok(())
}

#[tokio::test]
async fn the_version_switch_shows_the_version_the_url_names() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(&three_versions(), 4).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(
        &service,
        get_as(
            "/stored-queries/org.example::weights?version=1.0.2",
            &session,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("Version 1.0.2, stored 2026-10-02T08:00:00Z."),
        "{body}"
    );
    assert!(body.contains("COMPOSITION"), "{body}");
    assert!(!body.contains("magnitude"), "{body}");
    assert!(
        body.contains(r#"href="/query?stored=org.example::weights&amp;version=1.0.2""#),
        "{body}"
    );
    let current = body
        .split(r#"aria-current="page""#)
        .count()
        .saturating_sub(1);
    assert!(current >= 1, "{body}");
    assert!(body.contains(">shown</span>"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_version_the_gateway_does_not_hold_is_said_and_the_newest_shown()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway(&three_versions(), 4).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (_, body) = send(
        &service,
        get_as(
            "/stored-queries/org.example::weights?version=9.9.9",
            &session,
        )?,
    )
    .await?;
    assert!(
        body.contains("The gateway holds no version 9.9.9 of this query. The newest is shown."),
        "{body}"
    );
    assert!(body.contains("Version 1.0.10,"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_query_the_gateway_does_not_hold_is_said() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(&three_versions(), 4).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(
        &service,
        get_as("/stored-queries/org.example::absent", &session)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("No such stored query"), "{body}");
    assert!(
        body.contains("The gateway holds no stored query named org.example::absent."),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn the_gateways_own_namespace_is_marked_read_only() -> Result<(), Box<dyn Error>> {
    let items = vec![
        held(
            "eu.ferrofed.eehrxf::patient-summary-problems",
            "1.0.0",
            "2026-10-06T00:00:00Z",
            "SELECT c FROM EHR e CONTAINS COMPOSITION c",
        ),
        held(
            "org.example::weights",
            "1.0.0",
            "2026-10-01T08:00:00Z",
            "SELECT 1",
        ),
    ];
    let gateway = gateway(&items, 2).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (_, list) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(1, list.matches(">read-only</span>").count(), "{list}");
    let reserved = list
        .split_once(">eu.ferrofed.eehrxf</h2>")
        .and_then(|(_, rest)| rest.split_once("</header>"))
        .map(|(header, _)| header)
        .ok_or("the reserved namespace has its card")?;
    assert!(reserved.contains("read-only"), "{reserved}");
    assert!(!list.contains("CONTAINS COMPOSITION"), "{list}");
    let (_, detail) = send(
        &service,
        get_as(
            "/stored-queries/eu.ferrofed.eehrxf::patient-summary-problems",
            &session,
        )?,
    )
    .await?;
    assert!(detail.contains(">read-only</span>"), "{detail}");
    let (_, other) = send(
        &service,
        get_as("/stored-queries/org.example::weights", &session)?,
    )
    .await?;
    assert!(!other.contains("read-only"), "{other}");
    Ok(())
}

// The list and the detail reach the page through `inner_html`, so every
// value the gateway names in them is escaped by `view!`, and none breaks out
// of a text or an attribute.
#[tokio::test]
async fn hostile_values_in_a_stored_query_render_escaped() -> Result<(), Box<dyn Error>> {
    let aql = "SELECT '<script>alert(1)</script>' AS \"x\\\" onmouseover=\\\"y\" \
               FROM EHR e -- </code></pre><img src=x onerror=alert(2)>\nWHERE c/name/value = \
               '&amp; <b>bold</b>'";
    let items = vec![held(
        "org.example::hostile",
        "1.0.0",
        "\"><b>saved</b>",
        aql,
    )];
    let stub = gateway(&items, 1).await?;
    let (service, session) = signed_in_console(&stub)?;
    let (_, detail) = send(
        &service,
        get_as("/stored-queries/org.example::hostile", &session)?,
    )
    .await?;
    for raw in [
        "<script>alert(1)",
        "<img src=x",
        "</code></pre><img",
        "<b>bold</b>",
        "<b>saved</b>",
    ] {
        assert!(!detail.contains(raw), "{raw}: {detail}");
    }
    for escaped in [
        "&lt;script&gt;alert(1)&lt;/script&gt;",
        "&lt;img src=x onerror=alert(2)&gt;",
        "&amp;amp; &lt;b&gt;bold&lt;/b&gt;",
        "&lt;b&gt;saved&lt;/b&gt;",
    ] {
        assert!(detail.contains(escaped), "{escaped}: {detail}");
    }

    let items = vec![held(
        "org.example\"><i>ns</i>::q\" onclick=\"x<b>",
        "1.0.0\"><u>v</u>",
        "2026-10-01T08:00:00Z",
        "SELECT 1",
    )];
    let stub = gateway(&items, 1).await?;
    let (service, session) = signed_in_console(&stub)?;
    let (_, list) = send(&service, get_as("/stored-queries", &session)?).await?;
    for raw in [
        "<i>ns</i>",
        "<b>@",
        "<u>v</u>",
        "<a onclick",
        "\" onclick=\"x<b>",
    ] {
        assert!(!list.contains(raw), "{raw}: {list}");
    }
    // A quote in a text node is text: it ends no attribute.
    assert!(
        list.contains(">q\" onclick=\"x&lt;b&gt;</span><span class=\"whitespace-nowrap text-muted\">@1.0.0\"&gt;&lt;u&gt;v&lt;/u&gt;</span></a>"),
        "{list}"
    );
    assert!(
        list.contains("org.example\"&gt;&lt;i&gt;ns&lt;/i&gt;"),
        "{list}"
    );
    assert!(
        list.contains(r#"href="/stored-queries/org.example%22%3E%3Ci%3Ens%3C%2Fi%3E::q%22%20onclick%3D%22x%3Cb%3E?version=1.0.0%22%3E%3Cu%3Ev%3C%2Fu%3E""#),
        "{list}"
    );
    Ok(())
}

#[tokio::test]
async fn the_detail_of_a_stored_query_needs_a_session() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(&three_versions(), 4).await?;
    let (service, _session) = signed_in_console(&gateway)?;
    for detail in [
        "/stored-queries/org.example::weights",
        "/Stored-Queries/org.example::weights/",
    ] {
        let (response, _body) = send(&service, get(detail)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{detail}");
        assert_eq!("/login", header(&response, "location"), "{detail}");
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(asked.is_empty(), "the gateway was asked {}", asked.len());
    Ok(())
}

// The detail's "Open in query console" names the stored query and its
// version in the URL, and the console opens on that query with no AQL.
#[tokio::test]
async fn the_console_opens_on_the_stored_query_its_url_names() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(&three_versions(), 4).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(
        &service,
        get_as("/query?stored=org.example::weights&version=1.0.2", &session)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains(r#"value="org.example::weights""#), "{body}");
    assert!(body.contains(r#"value="1.0.2""#), "{body}");
    assert!(body.contains(r#"value="stored""#), "{body}");
    assert!(!body.contains("CONTAINS OBSERVATION"), "{body}");
    Ok(())
}
