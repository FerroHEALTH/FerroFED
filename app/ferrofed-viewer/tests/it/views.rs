// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator views against a stub gateway: each reads the gateway's own
//! surface as the signed-in operator and renders on the server, each needs
//! a signed-in session, each carries the browser security headers, and none
//! renders the operator's token (§5.4.1, N33).

use std::error::Error;
use std::time::Duration;

use ferrofed_viewer::server::ViewerState;
use http::StatusCode;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, console, get, get_as, header, send, signed_in};

/// The classes of a cell that heads a row with an identifier.
pub(crate) const MONO: &str = "font-mono text-[0.8125rem] whitespace-nowrap";

/// The vendored specification page that carries the §7a.2 example.
const REST_FACADE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/pages/rest-facade.adoc"
);

/// The one `[source,json]` example of the §7a.2 page.
pub(crate) fn options_example() -> Result<String, Box<dyn Error>> {
    let page = std::fs::read_to_string(REST_FACADE)?;
    let mut lines = page.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "[source,json]" && lines.next().map(str::trim) == Some("----") {
            let body: Vec<&str> = lines.by_ref().take_while(|l| l.trim() != "----").collect();
            return Ok(body.join("\n"));
        }
    }
    Err("the page carries no JSON example".into())
}

/// The `endpoint_id`s the §7a.2 example declares.
fn example_endpoints() -> Result<Vec<String>, Box<dyn Error>> {
    let body: openehr_federation::options::OptionsRoot = serde_json::from_str(&options_example()?)?;
    Ok(body
        .endpoints
        .iter()
        .map(|endpoint| endpoint.id.to_string())
        .collect())
}

/// Answers `GET` or `OPTIONS` of `route` with the JSON `body`, only for the
/// operator's own token.
pub(crate) async fn answers(gateway: &MockServer, verb: &str, route: &str, body: String) {
    answers_after(Duration::ZERO, gateway, verb, route, body).await;
}

/// Answers as [`answers`] does, each answer `delay` late.
async fn answers_after(
    delay: Duration,
    gateway: &MockServer,
    verb: &str,
    route: &str,
    body: String,
) {
    Mock::given(method(verb))
        .and(path(route))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(body)
                .set_delay(delay),
        )
        .mount(gateway)
        .await;
}

/// A stub gateway answering every surface the views read.
pub(crate) async fn gateway() -> Result<MockServer, Box<dyn Error>> {
    gateway_after(Duration::ZERO).await
}

/// A stub gateway answering every surface the views read, each answer
/// `delay` late.
async fn gateway_after(delay: Duration) -> Result<MockServer, Box<dyn Error>> {
    let gateway = MockServer::start().await;
    answers_after(delay, &gateway, "OPTIONS", "/", options_example()?).await;
    let endpoints = example_endpoints()?
        .iter()
        .map(|id| format!("\"{id}\":\"up\""))
        .collect::<Vec<_>>()
        .join(",");
    answers_after(
        delay,
        &gateway,
        "GET",
        "/operator/dependencies",
        format!(r#"{{"endpoints":{{{endpoints}}},"resolver":"failing","replica":"ferrofed-0"}}"#),
    )
    .await;
    answers_after(
        delay,
        &gateway,
        "GET",
        "/operator/incidents",
        String::from(
            r#"{"counts":{"EhrIdCollision":1,"IndexInsertCollision":0,"LearnedCreatingSystemConflict":0,"RegisteredCreatingSystemConflict":0},
               "recent":[{"kind":"EhrIdCollision","at":"2026-10-05T08:00:00Z",
               "description":"ehr_id 7d44b88c-4199-4bad-97dc-d78268e01398 is claimed by endpoints [node-a-pub, node-b-pub], found by the ask-all probe",
               "creating_system_id":null,"ehr_id":"7d44b88c-4199-4bad-97dc-d78268e01398","detection":"ask-all",
               "endpoints":["node-a-pub","node-b-pub"],"nodes":[]}],
               "cleared":[{"at":"2026-10-05T09:00:00Z","ehr_id":"7d44b88c-4199-4bad-97dc-d78268e01398",
               "claimants":["node-a","node-b"]},
               {"at":"2026-10-05T10:00:00Z","ehr_id":null,"claimants":["node-a","node-c"]}],"replica":"ferrofed-0"}"#,
        ),
    )
    .await;
    answers_after(
        delay,
        &gateway,
        "GET",
        "/operator/creating-systems",
        String::from(
            r#"{"items":[{"creating_system_id":"cdr-a.example.org","source":"member","node":"node-a","endpoint":null},
               {"creating_system_id":"legacy-a.example.org","source":"learned","node":"node-a","endpoint":"node-a-pub"}],
               "offset":0,"total":2,"replica":"ferrofed-1"}"#,
        ),
    )
    .await;
    answers_after(
        delay,
        &gateway,
        "GET",
        "/operator/stored-queries",
        String::from(
            r#"{"items":[{"name":"org.example::compositions","type":"AQL","version":"1.0.0","saved":"2026-10-05T08:00:00Z",
               "q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c"}],
               "offset":0,"total":1,"replica":"ferrofed-0"}"#,
        ),
    )
    .await;
    Ok(gateway)
}

/// A console over `gateway` with one signed-in operator.
pub(crate) fn signed_in_console(
    gateway: &MockServer,
) -> Result<
    (
        ViewerState,
        axum::Router,
        ferrofed_viewer::session::SessionId,
    ),
    Box<dyn Error>,
> {
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{}\"\n\n[session]\nsecure_cookie = false\n",
        gateway.uri()
    ))?;
    let session = state.sessions().establish(signed_in())?;
    Ok((state, service, session))
}

#[tokio::test]
async fn the_members_view_lists_every_endpoint_with_its_health() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/members", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<title>Members · FerroFED Console</title>"),
        "{body}"
    );
    // Each endpoint of the example names its node, so each row leads to
    // that member's page.
    for id in example_endpoints()? {
        assert!(
            body.contains(&format!(
                r#"<th scope="row" class="{MONO}"><a href="/members/{id}">{id}</a></th>"#
            )),
            "{id}: {body}"
        );
    }
    assert!(
        body.contains(r#"<td><span class="pill tone-ok">up</span></td>"#),
        "{body}"
    );
    assert!(body.contains("resolver"), "{body}");
    assert!(
        body.contains(r#"<td><span class="pill tone-danger">failing</span></td>"#),
        "{body}"
    );
    Ok(())
}

// The gateway lists the collisions the operator cleared beside the
// incidents, which a clear never removes, and the view shows each, newest
// first, naming the ehr_id only when the gateway does.
#[tokio::test]
async fn the_integrity_view_lists_the_collisions_cleared_recently_newest_first()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/integrity?show=resolved", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<caption>Collisions cleared recently, newest first</caption>"),
        "{body}"
    );
    let newer = body
        .find(r#"<td class="font-mono text-[0.8125rem] whitespace-nowrap">2026-10-05T10:00:00Z</td><th scope="row" class="font-mono text-[0.8125rem] whitespace-nowrap">not a bare UUID</th><td>node-a, node-c</td>"#)
        .ok_or("the newer clear")?;
    let older = body
        .find(r#"<td class="font-mono text-[0.8125rem] whitespace-nowrap">2026-10-05T09:00:00Z</td><th scope="row" class="font-mono text-[0.8125rem] whitespace-nowrap">7d44b88c-4199-4bad-97dc-d78268e01398</th><td>node-a, node-b</td>"#)
        .ok_or("the older clear")?;
    assert!(newer < older, "newest first: {body}");
    assert!(
        body.contains("found by the ask-all probe"),
        "the incident stays: {body}"
    );

    let none = paged_gateway(0, 2).await;
    let (_state, service, session) = signed_in_console(&none)?;
    let (_response, body) = send(&service, get_as("/integrity", &session)?).await?;
    assert!(
        body.contains("No collision cleared since the gateway started."),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn the_stored_query_view_groups_each_version_by_namespace_without_its_text()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains(">org.example</h2>"), "{body}");
    assert!(body.contains(">1 version</span>"), "{body}");
    assert!(
        body.contains(r#"href="/stored-queries/org.example::compositions?version=1.0.0""#),
        "{body}"
    );
    assert!(
        body.contains(
            ">compositions</span><span class=\"whitespace-nowrap text-muted\">@1.0.0</span></a>"
        ),
        "{body}"
    );
    assert!(!body.contains("CONTAINS COMPOSITION"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_self_description_view_shows_what_options_declares() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/federation", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    let example: openehr_federation::options::OptionsRoot =
        serde_json::from_str(&options_example()?)?;
    assert!(body.contains(example.federation.id.as_str()), "{body}");
    assert!(
        body.contains(example.federation.spec_version.as_str()),
        "{body}"
    );
    assert!(body.contains("spec_version"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_view_without_a_signed_in_session_sends_the_browser_to_sign_in()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, _session) = signed_in_console(&gateway)?;
    for view in [
        "/members",
        "/integrity",
        "/routing",
        "/members/node_1",
        "/stored-queries",
        "/federation",
    ] {
        let (response, _body) = send(&service, get(view)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{view}");
        assert_eq!("/login", header(&response, "location"), "{view}");
        let unknown = ferrofed_viewer::session::SessionId::from_cookie("forged");
        let (response, _body) = send(&service, get_as(view, &unknown)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{view}");
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(
        asked.is_empty(),
        "the gateway was asked {} times",
        asked.len()
    );
    Ok(())
}

#[tokio::test]
async fn a_view_the_gateway_refuses_shows_the_status_and_the_code() -> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operator/stored-queries"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("content-type", "application/json")
                .set_body_string(r#"{"message":"no scope of the access token grants this operation","code":"scope-insufficient"}"#),
        )
        .mount(&gateway)
        .await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("The gateway refused this view: 403 (scope-insufficient)."),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_view_the_gateway_cannot_answer_says_so() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(
        "[gateway]\nbase_url = \"http://127.0.0.1:9/\"\ntimeout_ms = 2000\n\n[session]\nsecure_cookie = false\n",
    )?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/integrity", &session)?).await?;
    assert!(body.contains("the gateway could not be reached"), "{body}");
    Ok(())
}

#[tokio::test]
async fn no_view_renders_the_operators_token_and_each_carries_the_security_headers()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for view in [
        "/members",
        "/integrity",
        "/routing",
        "/members/node_1",
        "/stored-queries",
        "/federation",
    ] {
        let (response, body) = send(&service, get_as(view, &session)?).await?;
        assert!(!body.contains(OPERATOR_TOKEN), "{view}: {body}");
        assert!(!body.contains(session.as_str()), "{view}: {body}");
        assert_eq!("no-store", header(&response, "cache-control"), "{view}");
        assert_eq!("DENY", header(&response, "x-frame-options"), "{view}");
        let policy = header(&response, "content-security-policy");
        let nonce = policy
            .split_once("'nonce-")
            .and_then(|(_, rest)| rest.split_once('\''))
            .map(|(nonce, _)| nonce)
            .ok_or("the policy names a nonce")?;
        assert!(
            body.contains(&format!(r#"<script type="module" nonce="{nonce}">"#)),
            "{view}: {body}"
        );
        assert!(!policy.contains("'unsafe-inline'"), "{view}: {policy}");
    }
    Ok(())
}

// The gate names a view by its path in any case and with or without a
// trailing slash, so neither form reaches a view without a session.
#[tokio::test]
async fn a_view_path_in_another_case_or_with_a_trailing_slash_still_needs_a_session()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, _session) = signed_in_console(&gateway)?;
    for view in ["/members/", "/MEMBERS", "/Integrity/", "/stored-queries//"] {
        let (response, _body) = send(&service, get(view)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{view}");
        assert_eq!("/login", header(&response, "location"), "{view}");
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(asked.is_empty(), "the gateway was asked {}", asked.len());
    Ok(())
}

/// A stub gateway whose routing table holds `total` rows, of which it answers
/// the two from `offset` when asked for that page.
async fn paged_gateway(offset: u64, total: u64) -> MockServer {
    let gateway = MockServer::start().await;
    answers(
        &gateway,
        "GET",
        "/operator/incidents",
        String::from(r#"{"counts":{},"recent":[]}"#),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/operator/creating-systems"))
        .and(query_param("offset", offset.to_string().as_str()))
        .and(query_param("limit", "100"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(format!(
                    r#"{{"items":[{{"creating_system_id":"a.example.org","source":"registered","node":null,"endpoint":"node-a-pub"}},
                       {{"creating_system_id":"b.example.org","source":"registered","node":null,"endpoint":"node-a-pub"}}],
                       "offset":{offset},"total":{total}}}"#
                )),
        )
        .mount(&gateway)
        .await;
    gateway
}

#[tokio::test]
async fn the_routing_table_shows_its_page_and_links_the_next() -> Result<(), Box<dyn Error>> {
    let gateway = paged_gateway(0, 150).await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/routing", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("Rows 1 to 2 of 150."), "{body}");
    assert!(
        body.contains(r#"href="/routing?offset=2" rel="next""#),
        "{body}"
    );
    assert!(!body.contains(r#"rel="prev""#), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_later_page_of_the_routing_table_is_asked_for_by_its_offset() -> Result<(), Box<dyn Error>>
{
    let gateway = paged_gateway(148, 150).await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/routing?offset=148", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("Rows 149 to 150 of 150."), "{body}");
    assert!(
        body.contains(r#"href="/routing?offset=48" rel="prev""#),
        "{body}"
    );
    assert!(!body.contains(r#"rel="next""#), "{body}");
    Ok(())
}

#[tokio::test]
async fn an_offset_that_is_no_number_shows_the_first_page() -> Result<(), Box<dyn Error>> {
    let gateway = paged_gateway(0, 2).await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/routing?offset=-1", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("Rows 1 to 2 of 2."), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_view_the_gateway_does_not_authenticate_links_back_to_sign_in()
-> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operator/stored-queries"))
        .respond_with(
            ResponseTemplate::new(401)
                .insert_header("content-type", "application/json")
                .set_body_string(
                    r#"{"message":"the access token has expired","code":"token-invalid"}"#,
                ),
        )
        .mount(&gateway)
        .await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("The gateway did not accept your sign-in (token-invalid)."),
        "{body}"
    );
    assert!(body.contains(r#"href="/login""#), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_view_whose_answer_the_console_cannot_read_says_so() -> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    answers(
        &gateway,
        "GET",
        "/operator/stored-queries",
        String::from(r#"{"definitions":"of a shape this console does not know"}"#),
    )
    .await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("the gateway answered 200 with a body this console cannot read"),
        "{body}"
    );
    assert!(!body.contains("holds no stored query"), "{body}");
    Ok(())
}

// A page whose content comes from the gateway is whole in the HTML the
// server sends, every time, however long the gateway takes: nothing waits in
// a `<template>` for an inline script to move it into place, so the page
// reads with no script at all.
#[tokio::test(flavor = "multi_thread")]
async fn every_gateway_page_is_whole_in_the_html_the_server_sends() -> Result<(), Box<dyn Error>> {
    for delay in [Duration::ZERO, Duration::from_millis(50)] {
        let gateway = gateway_after(delay).await?;
        let (_state, service, session) = signed_in_console(&gateway)?;
        for (page, content) in [
            ("/", "Gateway UP"),
            ("/members", "<caption>Member endpoints</caption>"),
            (
                "/members/node_1",
                "<caption>Endpoints of this member</caption>",
            ),
            (
                "/integrity",
                "<caption>Incidents since the gateway started</caption>",
            ),
            (
                "/routing",
                "<caption>The creating_system_id routing table</caption>",
            ),
            ("/stored-queries", r#"aria-label="Namespaces""#),
            ("/federation", "<caption>Declared endpoints</caption>"),
            ("/query", r#"<button type="submit" disabled class="#),
        ] {
            for round in 1..=10 {
                let (response, body) = send(&service, get_as(page, &session)?).await?;
                assert_eq!(StatusCode::OK, response.status(), "{page} #{round}: {body}");
                let document = body
                    .split_once("</html>")
                    .map_or(body.as_str(), |(document, _after)| document);
                assert!(document.contains(content), "{page} #{round}: {body}");
                assert!(!body.contains("<template id="), "{page} #{round}: {body}");
                assert!(
                    !body.contains("createTreeWalker"),
                    "{page} #{round}: {body}"
                );
            }
        }
    }
    Ok(())
}

/// Each replica holds its own health, incidents and routing table, so each
/// view of the operator surface names the replica its reports name, and
/// names each when two replicas answered one view.
#[tokio::test]
async fn each_operator_view_names_the_replica_that_answered() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for (page, line) in [
        (
            "/members",
            r#"<p class="-mt-3 mb-5 text-xs text-muted">Answered by replica ferrofed-0.</p>"#,
        ),
        (
            "/members/node_1",
            r#"<p class="-mt-3 mb-5 text-xs text-muted">Answered by replica ferrofed-0.</p>"#,
        ),
        ("/stored-queries", "<p>Answered by replica ferrofed-0.</p>"),
        (
            "/integrity",
            r#"<p class="-mt-3 mb-5 text-xs text-muted">Answered by replica ferrofed-0.</p>"#,
        ),
        (
            "/routing",
            r#"<p class="-mt-3 mb-5 text-xs text-muted">Answered by replica ferrofed-1.</p>"#,
        ),
    ] {
        let (response, body) = send(&service, get_as(page, &session)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{page}: {body}");
        assert!(body.contains(line), "{page}: {body}");
    }
    Ok(())
}

/// A gateway whose reports name no replica is shown as one, never as a
/// replica the console guessed.
#[tokio::test]
async fn a_view_whose_reports_name_no_replica_says_so() -> Result<(), Box<dyn Error>> {
    let gateway = paged_gateway(0, 2).await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/integrity", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains(r#"<p class="-mt-3 mb-5 text-xs text-muted">The gateway did not name the replica that answered.</p>"#),
        "{body}"
    );
    Ok(())
}
