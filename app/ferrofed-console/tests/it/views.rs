// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pages against a stub gateway: each reads the gateway's own surface
//! as the signed-in operator and renders whole on the server, each needs a
//! signed-in session, each carries the browser security headers, none
//! renders the operator's token (§5.4.1, N33), a refusal shows its status
//! and code, and the top bar names the replica that answered.

use std::error::Error;
use std::time::Duration;

use http::StatusCode;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::{example_endpoints, gateway, options_example, signed_in_console};
use crate::support::{OPERATOR_TOKEN, console, get, get_as, header, send, signed_in};

/// The pages that read the gateway, each with words it shows only once the
/// gateway answered.
const READING: [(&str, &str); 8] = [
    ("/", "Nodes that need attention"),
    ("/organisations", "Organisations"),
    ("/organisations/nodes", "All nodes"),
    ("/integrity", "Two nodes claim one EHR id"),
    ("/stored-queries", "org.example::compositions"),
    ("/self-description", "Federation"),
    ("/topology", "The federation as a map"),
    ("/query", "Run"),
];

#[tokio::test]
async fn the_nodes_page_lists_every_endpoint_with_its_health() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/organisations/nodes", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<title>All nodes · FerroFED Console</title>"),
        "{body}"
    );
    for (node, organisation) in [("node_1", "Org%20A"), ("node_2", "Org%20B")] {
        let link = format!(r#"href="/organisations/{organisation}/{node}""#);
        assert!(body.contains(&link), "{link}: {body}");
    }
    assert!(body.contains("Reachable"), "{body}");
    let (response, org) = send(&service, get_as("/organisations/Org%20A", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{org}");
    assert!(org.contains("node_1"), "{org}");
    let (response, node) =
        send(&service, get_as("/organisations/Org%20A/node_1", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{node}");
    assert!(node.contains("cdr1.rso.nl"), "{node}");
    let (_, services) = send(&service, get_as("/services", &session)?).await?;
    assert!(services.contains("resolver"), "{services}");
    assert!(
        services.contains("Degraded"),
        "the failing resolver says so: {services}"
    );
    Ok(())
}

// The gateway lists the collisions the operator cleared beside the
// incidents, which a clear never removes, and the page shows each, newest
// first.
#[tokio::test]
async fn the_integrity_page_lists_the_collisions_cleared_recently() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/integrity?show=resolved", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("1d44b88c-4199-4bad-97dc-d78268e01398"),
        "{body}"
    );
    let (_, open) = send(&service, get_as("/integrity", &session)?).await?;
    assert!(
        open.contains("7d44b88c-4199-4bad-97dc-d78268e01398"),
        "the open incident stays: {open}"
    );
    Ok(())
}

#[tokio::test]
async fn the_self_description_shows_what_options_declares() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/self-description", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    let example: openehr_federation::options::OptionsRoot =
        serde_json::from_str(&options_example()?)?;
    assert!(body.contains(example.federation.id.as_str()), "{body}");
    assert!(
        body.contains(example.federation.spec_version.as_str()),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_page_without_a_signed_in_session_sends_the_browser_to_sign_in()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, _session) = signed_in_console(&gateway)?;
    let node = example_endpoints()?;
    let node = node.first().ok_or("the example declares an endpoint")?;
    for page in [
        "/organisations",
        "/integrity",
        "/stored-queries",
        "/self-description",
        "/topology",
        &format!("/organisations/org-a/{node}"),
    ] {
        let (response, _body) = send(&service, get(page)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{page}");
        assert_eq!("/login", header(&response, "location"), "{page}");
        let unknown = ferrofed_console::session::SessionId::from_cookie("forged");
        let (response, _body) = send(&service, get_as(page, &unknown)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{page}");
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
async fn a_page_the_gateway_refuses_shows_the_status_and_the_code() -> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operator/stored-queries"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("content-type", "application/json")
                .set_body_string(
                    r#"{"type":"https://ferrofed.eu/docs/integrate/errors.html#scope-insufficient","detail":"no scope of the access token grants this operation","code":"scope-insufficient"}"#,
                ),
        )
        .mount(&gateway)
        .await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("403: The gateway refused the request."),
        "{body}"
    );
    assert!(body.contains("scope-insufficient"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_page_the_gateway_cannot_answer_says_so() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(
        "[gateway]\nbase_url = \"http://127.0.0.1:9/\"\ntimeout_ms = 2000\n\n[session]\nsecure_cookie = false\n",
    )?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/integrity", &session)?).await?;
    assert!(
        body.contains("The console could not reach the gateway."),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn no_page_renders_the_operators_token_and_each_carries_the_security_headers()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for (page, _) in READING {
        let (response, body) = send(&service, get_as(page, &session)?).await?;
        assert!(!body.contains(OPERATOR_TOKEN), "{page}: {body}");
        assert!(!body.contains(session.as_str()), "{page}: {body}");
        assert_eq!("no-store", header(&response, "cache-control"), "{page}");
        assert_eq!("DENY", header(&response, "x-frame-options"), "{page}");
        let policy = header(&response, "content-security-policy");
        crate::support::strict(policy, &body).map_err(|refused| format!("{page}: {refused}"))?;
    }
    Ok(())
}

// The gate names a page by its path in any case and with or without a
// trailing slash, so neither form reaches a page without a session.
#[tokio::test]
async fn a_path_in_another_case_or_with_a_trailing_slash_still_needs_a_session()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, _session) = signed_in_console(&gateway)?;
    for page in [
        "/organisations/",
        "/ORGANISATIONS",
        "/Integrity/",
        "/stored-queries//",
    ] {
        let (response, _body) = send(&service, get(page)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{page}");
        assert_eq!("/login", header(&response, "location"), "{page}");
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(asked.is_empty(), "the gateway was asked {}", asked.len());
    Ok(())
}

/// A stub gateway whose routing table holds `total` rows over pages of
/// two, every row routed to `node_1`.
async fn paged_gateway(total: u64) -> Result<MockServer, Box<dyn Error>> {
    let gateway = gateway().await?;
    for offset in (0..total).step_by(2) {
        Mock::given(method("GET"))
            .and(path("/operator/creating-systems"))
            .and(query_param("offset", offset.to_string().as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(format!(
                        r#"{{"items":[{{"creating_system_id":"a{offset}.example.org","source":"registered","node":"node_1","endpoint":"node_1"}},
                           {{"creating_system_id":"b{offset}.example.org","source":"registered","node":"node_1","endpoint":"node_1"}}],
                           "offset":{offset},"total":{total}}}"#
                    )),
            )
            .with_priority(1)
            .mount(&gateway)
            .await;
    }
    Ok(gateway)
}

// A node's routes are read from every page of the routing table, so a
// creating system on a later page is shown too.
#[tokio::test]
async fn a_nodes_routes_are_read_from_every_page_of_the_routing_table() -> Result<(), Box<dyn Error>>
{
    let gateway = paged_gateway(6).await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(
        &service,
        get_as("/organisations/Org%20A/node_1/routes", &session)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    for system in ["a0.example.org", "b2.example.org", "b4.example.org"] {
        assert!(body.contains(system), "{system}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn a_page_the_gateway_does_not_authenticate_links_back_to_sign_in()
-> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operator/stored-queries"))
        .respond_with(
            ResponseTemplate::new(401)
                .insert_header("content-type", "application/json")
                .set_body_string(
                    r#"{"type":"https://ferrofed.eu/docs/integrate/errors.html#token-invalid","detail":"the access token has expired","code":"token-invalid"}"#,
                ),
        )
        .mount(&gateway)
        .await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("The gateway did not accept your sign-in."),
        "{body}"
    );
    assert!(body.contains("token-invalid"), "{body}");
    assert!(body.contains(r#"href="/login""#), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_page_whose_answer_the_console_cannot_read_says_so() -> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    crate::stub::answers(
        &gateway,
        ("GET", "/operator/stored-queries"),
        200,
        r#"{"definitions":"of a shape this console does not know"}"#,
    )
    .await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("The gateway answered with something this console cannot read."),
        "{body}"
    );
    assert!(
        body.contains("The gateway answered 200 with a body"),
        "{body}"
    );
    assert!(!body.contains("holds no stored query"), "{body}");
    Ok(())
}

/// A stub gateway answering the self-description and the deployment, each
/// answer `delay` late.
async fn slow_gateway(delay: Duration) -> Result<MockServer, Box<dyn Error>> {
    let slow = MockServer::start().await;
    for (verb, route, body) in [
        ("OPTIONS", "/", options_example()?),
        (
            "GET",
            "/operator/deployment",
            String::from(
                r#"{"mode":"production","product":"FerroFED","version":"0.0.12","replica":"ferrofed-0"}"#,
            ),
        ),
    ] {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(body)
                    .set_delay(delay),
            )
            .mount(&slow)
            .await;
    }
    Ok(slow)
}

// A page whose content comes from the gateway is whole in the HTML the
// server sends, every time, however long the gateway takes: nothing waits in
// a `<template>` for an inline script to move it into place, so the page
// reads with no script at all.
#[tokio::test(flavor = "multi_thread")]
async fn every_gateway_page_is_whole_in_the_html_the_server_sends() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for (page, content) in READING {
        for round in 1..=5 {
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
    let slow = slow_gateway(Duration::from_millis(50)).await?;
    let (_state, service, session) = signed_in_console(&slow)?;
    let (_response, body) = send(&service, get_as("/self-description", &session)?).await?;
    let document = body
        .split_once("</html>")
        .map_or(body.as_str(), |(document, _after)| document);
    assert!(
        document.contains("Federation"),
        "a slow answer is still whole: {body}"
    );
    assert!(!body.contains("<template id="), "{body}");
    Ok(())
}

// Each replica holds its own health, incidents and routing table, so the top
// bar names the replica that answered on every page (F6's line in the
// status pill), and names none it was not told.
#[tokio::test]
async fn every_page_names_the_replica_that_answered() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for (page, _) in READING {
        let (response, body) = send(&service, get_as(page, &session)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{page}: {body}");
        assert!(
            body.contains("answered by replica ferrofed-0"),
            "{page}: {body}"
        );
    }
    let unnamed = MockServer::start().await;
    crate::stub::answers(&unnamed, ("OPTIONS", "/"), 200, &options_example()?).await;
    crate::stub::answers(
        &unnamed,
        ("GET", "/operator/deployment"),
        200,
        r#"{"mode":"production","product":"FerroFED","version":"0.0.12"}"#,
    )
    .await;
    let (_state, service, session) = signed_in_console(&unnamed)?;
    let (_response, body) = send(&service, get_as("/about", &session)?).await?;
    assert!(!body.contains("answered by replica"), "{body}");
    Ok(())
}
