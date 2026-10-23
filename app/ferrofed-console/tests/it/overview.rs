// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The overview against a stub gateway: the dashboard and its parts, the
//! page of each node, and the integrity filter.

use std::error::Error;

use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::{answers, gateway, options_example, signed_in_console};
use crate::support::{get, get_as, send};

/// The page `body` as the browser draws it, without what follows the
/// document.
fn drawn(body: &str) -> &str {
    body.split_once("</html>")
        .map_or(body, |(document, _after)| document)
}

/// The rows of the first table of `body`, the incidents of the filter
/// shown.
fn incidents(body: &str) -> &str {
    body.split_once("<tbody>")
        .and_then(|(_, rest)| rest.split_once("</tbody>"))
        .map_or("", |(rows, _)| rows)
}

// A node's page shows its endpoints and their health; one the gateway does
// not declare is a 404, and a visitor is sent to sign in.
#[tokio::test]
async fn a_nodes_page_shows_its_endpoints_and_an_unknown_one_is_not_found()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) =
        send(&service, get_as("/organisations/Org%20A/node_1", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<title>node_1 · FerroFED Console</title>"),
        "{body}"
    );
    assert!(body.contains("cdr1.rso.nl"), "{body}");
    assert!(!drawn(&body).contains("cdr2.rso.nl"), "{body}");
    assert!(body.contains("Reachable"), "{body}");
    assert!(
        body.contains(r#"<span class="fui-state fui-tone-ok">"#),
        "a state is FerroUI's F12 mark: {body}"
    );
    assert!(
        body.contains(r#"<p class="fui-page-subtitle">Node of <a href="/organisations/Org%20A">"#),
        "the subtitle links the node's organisation: {body}"
    );
    let (response, endpoints) = send(
        &service,
        get_as("/organisations/Org%20A/node_1/endpoints", &session)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{endpoints}");
    assert!(endpoints.contains("node_1"), "{endpoints}");
    let (response, body) =
        send(&service, get_as("/organisations/Org%20A/node_9", &session)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status(), "{body}");
    let (response, body) =
        send(&service, get_as("/organisations/Org%20B/node_1", &session)?).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        response.status(),
        "a node under another organisation: {body}"
    );
    let (response, _body) = send(&service, get("/organisations/Org%20A/node_1")?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    Ok(())
}

// An incident whose ehr_id the operator cleared after it was found reads
// as resolved; the open filter, which a link with no filter shows, holds
// the incidents still open.
#[tokio::test]
async fn the_integrity_page_filters_the_open_and_the_resolved_incidents()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (_response, open) = send(&service, get_as("/integrity", &session)?).await?;
    assert!(
        incidents(&open).contains("7d44b88c-4199-4bad-97dc-d78268e01398"),
        "{open}"
    );
    assert!(
        !incidents(&open).contains("1d44b88c-4199-4bad-97dc-d78268e01398"),
        "{open}"
    );
    assert!(open.contains("1 open incident"), "{open}");
    assert!(open.contains("Counts since the start"), "{open}");
    let (_response, resolved) =
        send(&service, get_as("/integrity?show=resolved", &session)?).await?;
    assert!(
        incidents(&resolved).contains("1d44b88c-4199-4bad-97dc-d78268e01398"),
        "{resolved}"
    );
    assert!(
        !incidents(&resolved).contains("7d44b88c-4199-4bad-97dc-d78268e01398"),
        "{resolved}"
    );
    assert!(resolved.contains("1 resolved incident"), "{resolved}");
    Ok(())
}

// The dashboard counts what the gateway answers: the organisations, the
// nodes reachable, the open incidents and the stored queries, each linking
// on, and names the replica that answered.
#[tokio::test]
async fn the_dashboard_counts_the_federation_and_shows_each_part() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<title>Dashboard · FerroFED Console</title>"),
        "{body}"
    );
    assert!(body.contains("2 of 2"), "{body}");
    assert!(body.contains("Nodes reachable"), "{body}");
    assert!(body.contains("Open incidents"), "{body}");
    assert!(body.contains("1 stored version"), "{body}");
    assert!(
        body.contains("The federation as replica ferrofed-0 sees it."),
        "{body}"
    );
    assert!(body.contains(r#"href="/topology""#), "{body}");
    assert!(body.contains(r#"href="/integrity""#), "{body}");
    Ok(())
}

// A part of the dashboard the gateway refuses is named with its status and
// code, and the rest of the dashboard still shows.
#[tokio::test]
async fn a_dashboard_part_the_gateway_refuses_is_named_and_the_rest_shows()
-> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    answers(&gateway, ("OPTIONS", "/"), 200, &options_example()?).await;
    answers(
        &gateway,
        ("GET", "/operator/dependencies"),
        200,
        r#"{"endpoints":{"node_1":"up","node_2":"up"},"replica":"ferrofed-0"}"#,
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/operator/incidents"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("content-type", "application/json")
                .set_body_string(r#"{"message":"no operator scope","code":"scope-insufficient"}"#),
        )
        .mount(&gateway)
        .await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("403: The gateway refused the request."),
        "{body}"
    );
    assert!(body.contains("scope-insufficient"), "{body}");
    assert!(body.contains("Org A"), "the rest still shows: {body}");
    Ok(())
}
