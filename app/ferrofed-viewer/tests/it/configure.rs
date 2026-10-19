// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page against a stub admin listener: each member and
//! stored query is marked with its owner, a member the file provides offers
//! no edit, a console change reaches the first replica with the operator's
//! own token and the body the gateway takes, a saved revision is previewed
//! before it is applied, and every value the gateway answers is escaped.
//! The shared stub and form helpers live here; the refusals are in
//! `configure_refusals`.

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_viewer::admin::model::ActionAnswer;
use ferrofed_viewer::session::{COOKIE, SessionId};
use http::{Request, StatusCode};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, console, get_as, send, signed_in};

/// The running revision 1, applied by the file, and revision 2 saved.
const STANDING: &str = r#"{"ui":"managed","head":2,"current":{"revision":1,"previous":null,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"file"},
    "applied_at":"2026-10-15T08:00:00Z"},"running":1,"in_step":true,"refused":null}"#;

/// Member A the file provides, member C the console holds, with a markup
/// name and URL, and a stored query whose name carries markup.
const RESOURCES: &str = r#"{"running":1,"head":2,"members":[
    {"node":"node-a","organisation":"org-a","system_id":"cdr-a.example.org","endpoints":["node-a-pub"],"owner":"file"},
    {"node":"node-c","organisation":"org-c","system_id":"cdr-c.example.org","endpoints":["node-c-pub"],
     "owner":"console","state":"pending",
     "member":{"organisation":{"id":"org-c","name":"<script>alert(1)</script>\" onmouseover=\"x"},
               "node":{"id":"node-c","system_id":"cdr-c.example.org"},
               "endpoints":[{"id":"node-c-pub","url":"https://cdr-c.example.org/openehr\"><b>x</b>",
                             "connection_type":"openehr-rest-query"}]}}],
    "stored_query_registry":"writable",
    "stored_queries":[{"name":"org.example::<i>q</i>","version":"1.0.0","owner":"store"}]}"#;

/// The history: revision 2 by an operator, with markup in its reason.
const HISTORY: &str = r#"{"items":[
    {"id":2,"base":1,"origin":"operator","restores":null,
     "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-<b>1</b>"},
     "at":"2026-10-15T09:00:00Z","reason":"add C <script>x</script>","toml":"ignored = 1"},
    {"id":1,"base":null,"origin":"file","restores":null,"author":{"kind":"file"},
     "at":"2026-10-15T08:00:00Z","reason":"the configuration file was loaded","toml":""}],
    "offset":0,"total":2}"#;

/// What revision 2 changes against revision 1.
const DIFF: &str = r#"{"from":1,"to":2,"changes":[
    {"key":"registry.members[0].node.id","change":"added","from":null,"to":"\"node-c\"<script>"}]}"#;

/// Revision 2 as a save answers it.
const SAVED: &str = r#"{"id":2,"base":1,"origin":"operator","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:00:00Z","reason":"add C","toml":"x = 1"}"#;

/// Mounts on `listener` an answer of `status` with `body` to `verb` of
/// `route` that carries the operator's own token.
pub(crate) async fn answers(
    listener: &Server,
    (verb, route): (&str, &str),
    status: u16,
    body: &str,
) {
    Mock::given(method(verb))
        .and(path(route))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(status)
                .insert_header("content-type", "application/json")
                .set_body_string(body),
        )
        .mount(listener)
        .await;
}

/// An admin listener with revision 2 saved over the running revision 1.
pub(crate) async fn listener() -> Server {
    let listener = Server::start().await;
    answers(&listener, ("GET", "/admin/config"), 200, STANDING).await;
    answers(
        &listener,
        ("GET", "/admin/config/resources"),
        200,
        RESOURCES,
    )
    .await;
    answers(&listener, ("GET", "/admin/config/revisions"), 200, HISTORY).await;
    answers(
        &listener,
        ("GET", "/admin/config/revisions/1/diff/2"),
        200,
        DIFF,
    )
    .await;
    listener
}

/// A console with `listener` as its one replica, and a signed-in operator.
pub(crate) fn signed_in_console(
    listener: &Server,
) -> Result<(axum::Router, SessionId), Box<dyn Error>> {
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"http://127.0.0.1:9/\"\n\n[session]\nsecure_cookie = false\n\n\
         [[admin.replicas]]\nname = \"gateway-0\"\nadmin_url = \"{}\"\n",
        listener.uri()
    ))?;
    let session = state.sessions().establish(signed_in())?;
    Ok((service, session))
}

/// The form body of a configuration form with `fields`.
pub(crate) fn form(fields: &[(&str, &str)]) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        form.append_pair(&format!("form[{name}]"), value);
    }
    form.finish()
}

/// Runs the configuration form `body` as the hydrated page sends it, and
/// reads its rendered answer.
pub(crate) async fn configured(
    service: &axum::Router,
    session: &SessionId,
    body: String,
) -> Result<String, Box<dyn Error>> {
    Ok(answered(service, session, body).await?.html)
}

/// Runs the configuration form `body` as the hydrated page sends it, and
/// reads its answer whole.
pub(crate) async fn answered(
    service: &axum::Router,
    session: &SessionId,
    body: String,
) -> Result<ActionAnswer, Box<dyn Error>> {
    let request = Request::post("/api/configure")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(body))?;
    let (response, text) = send(service, request).await?;
    assert_eq!(StatusCode::OK, response.status(), "{text}");
    assert!(!text.contains(OPERATOR_TOKEN), "{text}");
    Ok(serde_json::from_str(&text)?)
}

/// The requests `listener` received with `verb` to `route`, by body.
pub(crate) async fn bodies(
    listener: &Server,
    (verb, route): (&str, &str),
) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?
        .into_iter()
        .filter(|request| request.method.as_str() == verb && request.url.path() == route)
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect())
}

/// How many requests other than reads `listener` received.
pub(crate) async fn changes(listener: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?
        .iter()
        .filter(|request| request.method.as_str() != "GET")
        .count())
}

/// The page at `path` as the signed-in operator reads it.
pub(crate) async fn page(
    service: &axum::Router,
    session: &SessionId,
    path: &str,
) -> Result<String, Box<dyn Error>> {
    let (response, body) = send(service, get_as(path, session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{path}: {body}");
    Ok(body)
}

/// The fields of Query API endpoint row `row` of a member form: its key,
/// its `endpoint_id`, its base URL and whether it is in service.
pub(crate) fn query_row(
    row: usize,
    (id, url, in_service): (&str, &str, bool),
) -> Vec<(String, String)> {
    let at = |part: &str| format!("query_endpoints][{row}][{part}");
    vec![
        (at("key"), row.to_string()),
        (at("id"), id.to_owned()),
        (at("url"), url.to_owned()),
        (at("in_service"), in_service.to_string()),
    ]
}

/// The form body of a configuration form with `fields` and the endpoint
/// rows `rows`.
pub(crate) fn form_with(fields: &[(&str, &str)], rows: &[(String, String)]) -> String {
    let mut all: Vec<(&str, &str)> = fields.to_vec();
    all.extend(
        rows.iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    );
    form(&all)
}

/// The save of console member C, its endpoints two rows, the second out of
/// service.
pub(crate) fn save_c() -> String {
    let mut rows = query_row(0, ("node-c-pub", "https://cdr-c.example.org/openehr", true));
    rows.extend(query_row(
        1,
        ("node-c-old", "https://old.example.org/openehr", false),
    ));
    form_with(
        &[
            ("action", "save-member"),
            ("base", "2"),
            ("organisation", "org-c"),
            ("organisation_name", "Organisation C"),
            ("node", "node-c"),
            ("system_id", "cdr-c.example.org"),
            ("reason", "add C"),
        ],
        &rows,
    )
}

/// The body of a member's save, as the gateway reads it.
#[derive(serde::Deserialize)]
struct SaveBody {
    base: Option<u64>,
    reason: String,
    member: ferrofed_registry::console::ConsoleMember,
}

#[tokio::test]
async fn a_save_reaches_the_replica_with_the_member_and_previews_its_changes()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("PUT", "/admin/config/members/node-c"),
        201,
        SAVED,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(&service, &session, save_c()).await?;
    assert!(
        html.contains("Replica gateway-0 saved revision 2."),
        "{html}"
    );
    assert!(html.contains("It is not applied"), "{html}");
    assert!(html.contains("What revision 2 changes, by key"), "{html}");
    let sent = bodies(&listener, ("PUT", "/admin/config/members/node-c")).await?;
    let [sent] = sent.as_slice() else {
        return Err(format!("one save, not {}", sent.len()).into());
    };
    let body: SaveBody = serde_json::from_str(sent)?;
    assert_eq!(Some(2), body.base);
    assert_eq!("add C", body.reason);
    assert_eq!("org-c", body.member.organisation.id.as_str());
    assert_eq!(2, body.member.endpoints.len());
    assert_eq!(
        ferrofed_registry::snapshot::EndpointStatus::Suspended,
        body.member.endpoints[1].status
    );
    Ok(())
}

/// A revision applied, as an apply answers it.
const APPLIED: &str = r#"{"revision":{"id":2,"base":1,"origin":"operator","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},"at":"2026-10-15T09:00:00Z","reason":"add C","toml":""},
    "current":{"revision":2,"previous":1,"outcome":"pending-restart","needs_restart":["federation.id"],
    "applied_by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},"applied_at":"2026-10-15T09:01:00Z"}}"#;

#[tokio::test]
async fn an_apply_and_a_rollback_post_to_their_routes() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("POST", "/admin/config/revisions/2/apply"),
        200,
        APPLIED,
    )
    .await;
    answers(&listener, ("POST", "/admin/config/rollback"), 200, APPLIED).await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        form(&[("action", "apply"), ("revision", "2"), ("confirm", "yes")]),
    )
    .await?;
    assert!(
        html.contains("Replica gateway-0 applied revision 2."),
        "{html}"
    );
    assert!(html.contains("only at a restart: federation.id"), "{html}");
    let applied = bodies(&listener, ("POST", "/admin/config/revisions/2/apply")).await?;
    assert_eq!(vec![String::new()], applied, "an apply sends no body");

    configured(
        &service,
        &session,
        form(&[
            ("action", "rollback"),
            ("current", "2"),
            ("reason", "C was premature"),
            ("confirm", "yes"),
        ]),
    )
    .await?;
    let rolled = bodies(&listener, ("POST", "/admin/config/rollback")).await?;
    assert_eq!(
        vec![String::from(r#"{"current":2,"reason":"C was premature"}"#)],
        rolled
    );
    Ok(())
}

#[tokio::test]
async fn a_stored_query_version_is_put_with_its_aql() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("PUT", "/admin/config/stored-queries/org.example::q/1.0.1"),
        201,
        r#"{"name":"org.example::q","type":"AQL","version":"1.0.1","saved":"2026-10-15T09:00:00Z",
            "q":"SELECT c FROM EHR e CONTAINS COMPOSITION c"}"#,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        form(&[
            ("action", "store-query"),
            ("name", "org.example::q"),
            ("version", "1.0.1"),
            ("aql", "SELECT c FROM EHR e CONTAINS COMPOSITION c"),
            ("confirm", "yes"),
        ]),
    )
    .await?;
    assert!(
        html.contains("Replica gateway-0 stored org.example::q version 1.0.1."),
        "{html}"
    );
    let sent = bodies(
        &listener,
        ("PUT", "/admin/config/stored-queries/org.example::q/1.0.1"),
    )
    .await?;
    assert_eq!(
        vec![String::from(
            r#"{"aql":"SELECT c FROM EHR e CONTAINS COMPOSITION c"}"#
        )],
        sent
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_gateway_whose_configuration_is_not_managed_shows_so_and_no_form()
-> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(
        &listener,
        ("GET", "/admin/config"),
        404,
        r#"{"message":"not managed","code":"configuration-unmanaged"}"#,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let (response, body) = send(&service, get_as("/configuration", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("its configuration is not managed"), "{body}");
    assert!(!body.contains("Save as a new revision"), "{body}");
    Ok(())
}

// The form shows a Query API endpoint's id, URL and status alone, so a save
// keeps the consent refusal codes of one it names (§11.1, N27), and a form
// drawn without the FHIR endpoints field, as a tab opened before it existed,
// keeps every held `hl7-fhir-rest` endpoint whole.
#[tokio::test]
async fn a_save_keeps_what_the_form_does_not_show() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(&listener, ("GET", "/admin/config"), 200, STANDING).await;
    answers(
        &listener,
        ("GET", "/admin/config/resources"),
        200,
        r#"{"running":1,"head":2,"stored_query_registry":"absent","stored_queries":[],"members":[
          {"node":"node-c","organisation":"org-c","system_id":"cdr-c.example.org",
           "endpoints":["node-c-pub","node-c-fhir"],"owner":"console","state":"pending",
           "member":{"organisation":{"id":"org-c"},"node":{"id":"node-c","system_id":"cdr-c.example.org"},
             "endpoints":[
               {"id":"node-c-pub","url":"https://old.example.org/openehr","connection_type":"openehr-rest-query",
                "consent_refusal_codes":["CONSENT_DENIED"]},
               {"id":"node-c-fhir","url":"https://cdr-c.example.org/fhir","connection_type":"hl7-fhir-rest",
                "categories":["Patient-Summaries"],"profiles":["http://example.org/ps"],
                "ehr_id_system":"urn:oid:2.999.1"}]}}]}"#,
    )
    .await;
    answers(
        &listener,
        ("PUT", "/admin/config/members/node-c"),
        201,
        SAVED,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    configured(&service, &session, save_c()).await?;
    let sent = bodies(&listener, ("PUT", "/admin/config/members/node-c")).await?;
    let [sent] = sent.as_slice() else {
        return Err(format!("one save, not {}", sent.len()).into());
    };
    let body: SaveBody = serde_json::from_str(sent)?;
    let ids: Vec<&str> = body
        .member
        .endpoints
        .iter()
        .map(|endpoint| endpoint.id.as_str())
        .collect();
    assert_eq!(vec!["node-c-pub", "node-c-old", "node-c-fhir"], ids);
    let public = &body.member.endpoints[0];
    assert_eq!("https://cdr-c.example.org/openehr", public.url);
    assert_eq!(
        vec![String::from("CONSENT_DENIED")],
        public.consent_refusal_codes
    );
    assert_eq!(
        Some("urn:oid:2.999.1"),
        body.member.endpoints[2].ehr_id_system.as_deref()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_stored_query_registry_still_offers_a_version() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(&listener, ("GET", "/admin/config"), 200, STANDING).await;
    answers(
        &listener,
        ("GET", "/admin/config/resources"),
        200,
        r#"{"running":1,"head":2,"members":[],"stored_query_registry":"writable","stored_queries":[]}"#,
    )
    .await;
    answers(&listener, ("GET", "/admin/config/revisions"), 200, HISTORY).await;
    answers(
        &listener,
        ("GET", "/admin/config/revisions/1/diff/2"),
        200,
        DIFF,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let body = page(&service, &session, "/configuration/stored-queries").await?;
    assert!(!body.contains("offers no stored-query registry"), "{body}");
    assert!(body.contains("Store a new version"), "{body}");
    let form = page(&service, &session, "/configuration/stored-queries/new").await?;
    assert!(
        form.contains("cannot be changed or removed afterwards"),
        "{form}"
    );
    assert!(
        form.contains(r#"<li aria-current="page">Store a new version</li>"#),
        "{form}"
    );
    let member = page(&service, &session, "/configuration/members/new").await?;
    assert!(
        member.contains(r#"<li aria-current="page">Add a member</li>"#),
        "{member}"
    );
    assert!(!member.contains(">Reason</legend>"), "{member}");
    assert!(member.contains(">Reason</label>"), "{member}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_console_with_no_admin_listener_offers_no_configuration_link()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(
        "[gateway]\nbase_url = \"http://127.0.0.1:9/\"\n\n[session]\nsecure_cookie = false\n",
    )?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/members", &session)?).await?;
    assert!(!body.contains(r#"href="/configuration""#), "{body}");
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let (_response, body) = send(&service, get_as("/members", &session)?).await?;
    assert!(body.contains(r#"href="/configuration""#), "{body}");
    Ok(())
}
