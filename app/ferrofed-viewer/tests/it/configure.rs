// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page against a stub admin listener: each member and
//! stored query is marked with its owner, a member the file provides offers
//! no edit, a console change reaches the first replica with the operator's
//! own token and the body the gateway takes, a saved revision is previewed
//! before it is applied, a refusal shows its status and code, a form that
//! is not confirmed asks no replica, and every value the gateway answers is
//! escaped.

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_viewer::admin::model::ActionAnswer;
use ferrofed_viewer::session::{COOKIE, SessionId};
use http::{Request, StatusCode};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::support::{Captured, OPERATOR_TOKEN, console, get_as, send, signed_in};

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
fn save_c() -> String {
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

#[tokio::test]
async fn a_change_the_gateway_refuses_shows_its_status_and_code() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("PUT", "/admin/config/members/node-c"),
        409,
        r#"{"message":"member node-c is provided by the file","code":"resource-file-owned"}"#,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(&service, &session, save_c()).await?;
    assert!(
        html.contains("Replica gateway-0 refused the save: 409 (resource-file-owned)."),
        "{html}"
    );
    Ok(())
}

// NOTE: no specification governs this: our own design; the gateway's message
// names the key it refused, as `config check` does, and reaches the page as text.
#[tokio::test]
async fn a_configuration_invalid_refusal_shows_the_key_the_gateway_names()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("PUT", "/admin/config/members/node-c"),
        422,
        r#"{"message":"the revision's configuration is refused: registry.timeout_ms <b>is</b> zero; it must be positive","code":"configuration-invalid"}"#,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(&service, &session, save_c()).await?;
    assert!(
        html.contains("Replica gateway-0 refused the save: 422 (configuration-invalid)."),
        "{html}"
    );
    assert!(html.contains("registry.timeout_ms"), "{html}");
    assert!(html.contains("&lt;b&gt;is&lt;/b&gt;"), "{html}");
    assert!(!html.contains("<b>is</b>"), "{html}");
    Ok(())
}

// NOTE: no specification governs this: our own design; only a
// configuration-invalid refusal shows the gateway's message.
#[tokio::test]
async fn another_refusal_shows_its_code_and_not_the_gateway_message() -> Result<(), Box<dyn Error>>
{
    for (status, code, said) in [
        (
            409,
            "revision-stale",
            "Replica gateway-0 refused the save: 409 (revision-stale).",
        ),
        (
            403,
            "scope-insufficient",
            "Replica gateway-0 refused the save: 403 (scope-insufficient). Your access token may carry no operator scope.",
        ),
    ] {
        let listener = listener().await;
        let body = format!(r#"{{"message":"the hidden text","code":"{code}"}}"#);
        answers(
            &listener,
            ("PUT", "/admin/config/members/node-c"),
            status,
            &body,
        )
        .await;
        let (service, session) = signed_in_console(&listener)?;
        let html = configured(&service, &session, save_c()).await?;
        assert!(html.contains(said), "{html}");
        assert!(!html.contains("the hidden text"), "{html}");
    }
    Ok(())
}

#[tokio::test]
async fn a_form_not_confirmed_or_not_readable_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    for unconfirmed in [
        form(&[("action", "apply"), ("revision", "2")]),
        form(&[("action", "rollback"), ("current", "2"), ("reason", "r")]),
        form(&[
            ("action", "remove-member"),
            ("node", "node-c"),
            ("reason", "r"),
        ]),
        form(&[("action", "confirm"), ("revision", "2")]),
        form(&[
            ("action", "store-query"),
            ("name", "org.example::q"),
            ("version", "1.0.1"),
            ("aql", "SELECT 1"),
        ]),
    ] {
        let html = configured(&service, &session, unconfirmed).await?;
        assert!(
            html.contains("Confirm the change in its confirmation step"),
            "{html}"
        );
    }
    for (field, value, said, at) in [
        ("node", "node c", "The node_id is not", "cfg-node"),
        ("system_id", "", "The system_id is not", "cfg-system_id"),
        (
            "query_endpoints][0][url",
            "",
            "The base URL of Query API endpoint 1 is not",
            "cfg-q-0-url",
        ),
        (
            "query_endpoints][0][key",
            "",
            "endpoint rows do not read",
            "",
        ),
        ("reason", " ", "Give the reason", "cfg-reason"),
    ] {
        let mut fields = vec![
            ("action", "save-member"),
            ("organisation", "org-c"),
            ("node", "node-c"),
            ("system_id", "cdr-c.example.org"),
            ("reason", "add C"),
        ];
        let mut rows = query_row(0, ("node-c-pub", "https://cdr-c.example.org/openehr", true));
        for slot in &mut fields {
            if slot.0 == field {
                slot.1 = value;
            }
        }
        for slot in &mut rows {
            if slot.0 == field {
                value.clone_into(&mut slot.1);
            }
        }
        let answer = answered(&service, &session, form_with(&fields, &rows)).await?;
        assert!(answer.html.contains(said), "{field}: {}", answer.html);
        assert_eq!(
            (!at.is_empty()).then(|| at.to_owned()),
            answer.field.map(|note| note.id),
            "{field}"
        );
    }
    let answer = answered(
        &service,
        &session,
        form(&[
            ("action", "save-member"),
            ("organisation", "org-c"),
            ("node", "node-c"),
            ("system_id", "cdr-c.example.org"),
            ("reason", "add C"),
        ]),
    )
    .await?;
    assert!(
        answer
            .html
            .contains("Give at least one Query API endpoint.")
    );
    assert_eq!(
        Some(String::from("cfg-add-query")),
        answer.field.map(|note| note.id)
    );
    assert_eq!(0, changes(&listener).await?);
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

// A 404 without the gateway's code, as from a wrong admin URL, is a refusal
// shown with its status, never read as a gateway that is not managed.
#[tokio::test(flavor = "multi_thread")]
async fn a_bare_not_found_is_shown_as_a_refusal() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(&listener, ("GET", "/admin/config"), 404, "").await;
    let (service, session) = signed_in_console(&listener)?;
    let (_response, body) = send(&service, get_as("/configuration", &session)?).await?;
    assert!(!body.contains("its configuration is not managed"), "{body}");
    assert!(body.contains("404 (no code)"), "{body}");
    Ok(())
}

// A plain form post, which a page without its bundle sends, asks no replica.
#[tokio::test]
async fn a_plain_form_post_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let request = Request::post("/api/configure")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/html")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(save_c()))?;
    let (response, body) = send(&service, request).await?;
    assert_ne!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("PlainPost"), "{body}");
    assert_eq!(0, changes(&listener).await?);
    let asked = listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(
        asked.is_empty(),
        "{} requests reached the replica",
        asked.len()
    );
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

// A save that cannot read the held member sends nothing: a save without it
// would drop what the form does not show.
#[tokio::test]
async fn a_save_whose_held_member_cannot_be_read_sends_nothing() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(&listener, ("GET", "/admin/config/resources"), 500, "").await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(&service, &session, save_c()).await?;
    assert!(
        html.contains("Replica gateway-0 refused the save: 500"),
        "{html}"
    );
    assert_eq!(0, changes(&listener).await?);
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
// A refused configuration action is logged as one, naming the action and the
// replica, with no member, reason, AQL text or token.
#[tokio::test]
async fn a_refused_configuration_action_is_logged_as_what_it_is() -> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let _logging = captured.logging();
    let listener = listener().await;
    let refused = r#"{"message":"refused","code":"revision-conflict"}"#;
    for route in [
        ("PUT", "/admin/config/members/node-c"),
        ("POST", "/admin/config/revisions/2/apply"),
        ("POST", "/admin/config/rollback"),
        ("PUT", "/admin/config/stored-queries/org.example::q/1.0.1"),
    ] {
        answers(&listener, route, 409, refused).await;
    }
    let (service, session) = signed_in_console(&listener)?;
    for body in [
        save_c(),
        form(&[("action", "apply"), ("revision", "2"), ("confirm", "yes")]),
        form(&[
            ("action", "rollback"),
            ("current", "2"),
            ("reason", "C was premature"),
            ("confirm", "yes"),
        ]),
        form(&[
            ("action", "store-query"),
            ("name", "org.example::q"),
            ("version", "1.0.1"),
            ("aql", "SELECT c FROM EHR e CONTAINS COMPOSITION c"),
            ("confirm", "yes"),
        ]),
    ] {
        configured(&service, &session, body).await?;
    }

    let logged = captured.text();
    for action in ["member save", "apply", "rollback", "stored-query store"] {
        let line = format!(
            r#"a replica refused an operator's configuration action status=409 Conflict code="revision-conflict" replica="gateway-0" action="{action}""#
        );
        assert!(logged.contains(&line), "{line}: {logged}");
    }
    for elsewhere in ["refused a view", "admin action"] {
        assert!(!logged.contains(elsewhere), "{elsewhere}: {logged}");
    }
    for named in [
        "node-c",
        "add C",
        "C was premature",
        "SELECT c FROM",
        OPERATOR_TOKEN,
    ] {
        assert!(!logged.contains(named), "{named}: {logged}");
    }
    Ok(())
}
