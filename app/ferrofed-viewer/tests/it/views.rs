// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator views against a stub gateway: each reads the gateway's own
//! surface as the signed-in operator and renders on the server, each needs
//! a signed-in session, each carries the browser security headers, and none
//! renders the operator's token (§5.4.1, N33).

use std::error::Error;

use axum::body::Body;
use ferrofed_viewer::server::ViewerState;
use http::{Request, StatusCode};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, console, get, get_as, header, send, signed_in};

/// The vendored specification page that carries the §7a.2 example.
const REST_FACADE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/pages/rest-facade.adoc"
);

/// The one `[source,json]` example of the §7a.2 page.
fn options_example() -> Result<String, Box<dyn Error>> {
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
async fn answers(gateway: &MockServer, verb: &str, route: &str, body: String) {
    Mock::given(method(verb))
        .and(path(route))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(body),
        )
        .mount(gateway)
        .await;
}

/// A stub gateway answering every surface the views read.
async fn gateway() -> Result<MockServer, Box<dyn Error>> {
    let gateway = MockServer::start().await;
    answers(&gateway, "OPTIONS", "/", options_example()?).await;
    let endpoints = example_endpoints()?
        .iter()
        .map(|id| format!("\"{id}\":\"up\""))
        .collect::<Vec<_>>()
        .join(",");
    answers(
        &gateway,
        "GET",
        "/health/dependencies",
        format!(r#"{{"endpoints":{{{endpoints}}},"resolver":"failing"}}"#),
    )
    .await;
    answers(
        &gateway,
        "GET",
        "/operator/incidents",
        String::from(
            r#"{"counts":{"EhrIdCollision":1,"IndexInsertCollision":0,"LearnedCreatingSystemConflict":0,"RegisteredCreatingSystemConflict":0},
               "recent":[{"kind":"EhrIdCollision","at":"2026-10-05T08:00:00Z",
               "description":"ehr_id 7d44b88c-4199-4bad-97dc-d78268e01398 is claimed by endpoints [node-a-pub, node-b-pub], found by the ask-all probe",
               "creating_system_id":null,"ehr_id":"7d44b88c-4199-4bad-97dc-d78268e01398","detection":"ask-all",
               "endpoints":["node-a-pub","node-b-pub"],"nodes":[]}]}"#,
        ),
    )
    .await;
    answers(
        &gateway,
        "GET",
        "/operator/creating-systems",
        String::from(
            r#"{"items":[{"creating_system_id":"cdr-a.example.org","source":"member","node":"node-a","endpoint":null},
               {"creating_system_id":"legacy-a.example.org","source":"learned","node":"node-a","endpoint":"node-a-pub"}],
               "offset":0,"total":2}"#,
        ),
    )
    .await;
    answers(
        &gateway,
        "GET",
        "/operator/stored-queries",
        String::from(
            r#"{"items":[{"name":"org.example::compositions","type":"AQL","version":"1.0.0","saved":"2026-10-05T08:00:00Z",
               "q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c"}],
               "offset":0,"total":1}"#,
        ),
    )
    .await;
    Ok(gateway)
}

/// A console over `gateway` with one signed-in operator.
fn signed_in_console(
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
        body.contains("<title>Members · FerroFED operator console</title>"),
        "{body}"
    );
    for id in example_endpoints()? {
        assert!(
            body.contains(&format!("<th scope=\"row\">{id}</th>")),
            "{id}: {body}"
        );
    }
    assert!(body.contains("<td>up</td>"), "{body}");
    assert!(body.contains("resolver"), "{body}");
    assert!(body.contains("<td>failing</td>"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_integrity_view_shows_the_counts_the_incidents_and_the_routing_table()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/integrity", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("EhrIdCollision"), "{body}");
    assert!(body.contains("found by the ask-all probe"), "{body}");
    assert!(
        body.contains("<th scope=\"row\">legacy-a.example.org</th>"),
        "{body}"
    );
    assert!(body.contains("<td>learned</td>"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_stored_query_view_lists_each_version_with_its_text() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("org.example::compositions"), "{body}");
    assert!(body.contains("<td>1.0.0</td>"), "{body}");
    assert!(
        body.contains("SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c"),
        "{body}"
    );
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
    for view in ["/members", "/integrity", "/stored-queries", "/federation"] {
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
    for view in ["/members", "/integrity", "/stored-queries", "/federation"] {
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

/// Every server function a view loads through, with the form body it takes.
const SERVER_FUNCTIONS: [(&str, &str); 4] = [
    ("/api/members", ""),
    ("/api/integrity", "offset=0"),
    ("/api/stored-queries", "offset=0"),
    ("/api/federation", ""),
];

/// A `POST` of the server function at `route` with `form`, carrying the
/// session cookie `cookie` when given.
fn call(route: &str, form: &str, cookie: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json");
    if let Some(cookie) = cookie {
        request = request.header(
            "cookie",
            format!("{}={cookie}", ferrofed_viewer::session::COOKIE),
        );
    }
    Ok(request.body(Body::from(form.to_owned()))?)
}

// Each server function is a public endpoint of the console, so each refuses
// a caller with no live session before it asks the gateway anything.
#[tokio::test]
async fn every_server_function_refuses_a_caller_without_a_live_session()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, _session) = signed_in_console(&gateway)?;
    for (route, form) in SERVER_FUNCTIONS {
        for cookie in [None, Some("forged")] {
            let (response, body) = send(&service, call(route, form, cookie)?).await?;
            assert_ne!(StatusCode::OK, response.status(), "{route} {cookie:?}");
            assert!(body.contains("SignedOut"), "{route} {cookie:?}: {body}");
        }
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
async fn every_server_function_answers_a_live_session() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for (route, form) in SERVER_FUNCTIONS {
        let (response, body) = send(&service, call(route, form, Some(session.as_str()))?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
        assert!(!body.contains(OPERATOR_TOKEN), "{route}: {body}");
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
    let (response, body) = send(&service, get_as("/integrity", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("Rows 1 to 2 of 150."), "{body}");
    assert!(
        body.contains(r#"href="/integrity?offset=2" rel="next""#),
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
    let (response, body) = send(&service, get_as("/integrity?offset=148", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("Rows 149 to 150 of 150."), "{body}");
    assert!(
        body.contains(r#"href="/integrity?offset=48" rel="prev""#),
        "{body}"
    );
    assert!(!body.contains(r#"rel="next""#), "{body}");
    Ok(())
}

#[tokio::test]
async fn an_offset_that_is_no_number_shows_the_first_page() -> Result<(), Box<dyn Error>> {
    let gateway = paged_gateway(0, 2).await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/integrity?offset=-1", &session)?).await?;
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
