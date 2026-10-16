// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The overview against a stub gateway: the dashboard and its parts, the
//! page of each member, the integrity filter and the routing table, and the
//! view models they are drawn from.

use std::error::Error;

use ferrofed_registry::operator::{IncidentReport, RecordedClear, RecordedIncident};
use ferrofed_viewer::views::load::server::{answered_by, detail, incidents};
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{get, get_as, send};
use crate::views::{MONO, answers, gateway, options_example, signed_in_console};

/// The page `body` as the browser draws it, without the data the hydrating
/// page reads after it.
fn drawn(body: &str) -> &str {
    body.split_once("</html>")
        .map_or(body, |(document, _after)| document)
}

// A member's page shows its endpoints and their health, from the members
// answer the members view reads; one the gateway does not declare is a 404.
#[tokio::test]
async fn a_members_page_shows_its_endpoints_and_an_unknown_one_is_not_found()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/members/node_1", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<title>Member node_1 · FerroFED Console</title>"),
        "{body}"
    );
    assert!(
        body.contains("<caption>Endpoints of this member</caption>"),
        "{body}"
    );
    assert!(
        body.contains(&format!(r#"<th scope="row" class="{MONO}">node_1</th>"#)),
        "{body}"
    );
    assert!(body.contains("cdr1.rso.nl"), "{body}");
    assert!(!drawn(&body).contains("cdr2.rso.nl"), "{body}");
    assert!(
        body.contains(r#"<span class="pill tone-ok">up</span>"#),
        "{body}"
    );

    let (response, body) = send(&service, get_as("/members/node_9", &session)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status(), "{body}");
    assert!(body.contains(">No such member</h1>"), "{body}");
    let (response, _body) = send(&service, get("/members/node_1")?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    Ok(())
}

#[tokio::test]
async fn the_integrity_view_shows_the_counts_and_the_incidents() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/integrity?show=resolved", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("EhrIdCollision"), "{body}");
    assert!(body.contains("found by the ask-all probe"), "{body}");
    assert!(
        body.contains("<caption>Incidents since the gateway started</caption>"),
        "{body}"
    );
    Ok(())
}

// An incident whose ehr_id the operator cleared after it was found reads
// as resolved, and the open filter, which a link with no filter shows,
// says no incident is open.
#[tokio::test]
async fn the_integrity_view_filters_the_open_and_the_resolved_incidents()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (_response, open) = send(&service, get_as("/integrity", &session)?).await?;
    assert!(open.contains(">No open incidents</h2>"), "{open}");
    assert!(
        !drawn(&open).contains("found by the ask-all probe"),
        "{open}"
    );
    assert!(
        open.contains(r#"<a href="/integrity" aria-current="page""#),
        "{open}"
    );
    let (_response, resolved) =
        send(&service, get_as("/integrity?show=resolved", &session)?).await?;
    assert!(
        resolved.contains(r#"<span class="pill tone-ok">resolved</span>"#),
        "{resolved}"
    );
    assert!(
        resolved.contains("found by the ask-all probe"),
        "{resolved}"
    );
    Ok(())
}

#[tokio::test]
async fn the_routing_view_shows_the_creating_system_id_table() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/routing", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<title>Routing · FerroFED Console</title>"),
        "{body}"
    );
    assert!(
        body.contains(&format!(
            r#"<th scope="row" class="{MONO}">legacy-a.example.org</th>"#
        )),
        "{body}"
    );
    assert!(
        body.contains(r#"<span class="badge tone-neutral">learned</span>"#),
        "{body}"
    );
    assert!(body.contains(r#"<label for="routing-filter">"#), "{body}");
    assert!(body.contains("2 of 2 rows on this page."), "{body}");
    Ok(())
}

// The dashboard counts what the gateway answers: the members, the nodes up,
// the open incidents and the stored queries, each linking on.
#[tokio::test]
async fn the_dashboard_counts_the_federation_and_shows_each_endpoints_health()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(&service, get_as("/", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("<title>Dashboard · FerroFED Console</title>"),
        "{body}"
    );
    assert!(body.contains(">2 of 2</p>"), "{body}");
    assert!(
        body.contains(r#"<span class="pill tone-ok">all up</span>"#),
        "{body}"
    );
    assert!(body.contains(">Open incidents</span>"), "{body}");
    assert!(body.contains(">Stored queries</span>"), "{body}");
    assert!(
        body.contains(r#"<a href="/members/node_1">node_1</a>"#),
        "{body}"
    );
    assert!(
        body.contains(r#"<span class="pill tone-ok">resolved</span>"#),
        "{body}"
    );
    assert!(
        !body.contains("Running revision"),
        "no admin listener: {body}"
    );
    assert!(body.contains("Answered by replica ferrofed-0."), "{body}");
    Ok(())
}

// A part of the dashboard the gateway refuses is named with its status and
// code, and the rest of the dashboard still shows.
#[tokio::test]
async fn a_dashboard_part_the_gateway_refuses_is_named_and_the_rest_shows()
-> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    answers(&gateway, "OPTIONS", "/", options_example()?).await;
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
        body.contains("The gateway refused this part: 403 (scope-insufficient)."),
        "{body}"
    );
    assert!(
        body.contains(r#"<a href="/members/node_1">node_1</a>"#),
        "{body}"
    );
    Ok(())
}

#[test]
fn a_replica_with_no_name_drops_the_line_and_two_replicas_are_each_named() {
    assert_eq!(None, answered_by(&[("health", Some("unnamed"))]));
    assert_eq!(
        Some(String::from("Answered by replica gateway-0.")),
        answered_by(&[("health", Some("gateway-0"))])
    );
    assert_eq!(
        Some(String::from(
            "The gateway did not name the replica that answered."
        )),
        answered_by(&[("health", None)])
    );
    assert_eq!(
        Some(String::from(
            "Each part names its replica: the incidents by replica gateway-0, the routing table by a replica that has no name."
        )),
        answered_by(&[
            ("incidents", Some("gateway-0")),
            ("routing table", Some("unnamed"))
        ])
    );
}

#[test]
fn a_members_page_is_keyed_by_a_registry_id_alone() {
    assert_eq!(
        Some(String::from("/members/node-a")),
        detail(Some("node-a"), "node-a-pub")
    );
    assert_eq!(
        Some(String::from("/members/loose-pub")),
        detail(None, "loose-pub")
    );
    assert_eq!(None, detail(Some("node a/../x"), "node-a-pub"));
    assert_eq!(None, detail(None, "?x=1"));
}

fn incident(at: &str, ehr_id: Option<&str>) -> RecordedIncident {
    RecordedIncident {
        kind: String::from("EhrIdCollision"),
        at: at.to_owned(),
        description: String::from("synthetic"),
        creating_system_id: None,
        ehr_id: ehr_id.map(str::to_owned),
        detection: None,
        endpoints: Vec::new(),
        nodes: Vec::new(),
    }
}

#[test]
fn an_incident_is_resolved_only_by_a_clear_of_its_ehr_id_after_it() {
    let ehr_id = "7d44b88c-4199-4bad-97dc-d78268e01398";
    let report = IncidentReport {
        counts: std::collections::BTreeMap::new(),
        recent: vec![
            incident("2026-10-05T08:00:00Z", Some(ehr_id)),
            incident("2026-10-05T10:00:00Z", Some(ehr_id)),
            incident("2026-10-05T08:00:00Z", None),
            incident("no time", Some(ehr_id)),
        ],
        cleared: vec![RecordedClear {
            at: String::from("2026-10-05T09:00:00.5Z"),
            ehr_id: Some(ehr_id.to_owned()),
            claimants: Vec::new(),
        }],
        replica: None,
    };
    let resolved: Vec<bool> = incidents(&report).iter().map(|row| row.resolved).collect();
    assert_eq!(vec![false, false, false, true], resolved, "newest first");
}
