// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's admin actions against stub admin listeners: the
//! collision clear on Integrity and the drift repair on a stored query's
//! page post to their page, reach the replica the operator picked with the
//! operator's own token, ask for confirmation first, name the replica they
//! acted on, show a refusal's status and code, and are offered only where
//! the console is configured with an admin listener.

use std::error::Error;

use axum::body::Body;
use ferrofed_console::session::SessionId;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::support::{Captured, OPERATOR_TOKEN, console, get_as, send, signed_in};

/// A synthetic `ehr_id` two members claim.
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The synthetic stored query the repair sends again.
const NAME: &str = "org.example::compositions";

/// The path of the clear of [`EHR_ID`] on an admin listener.
fn clear_path() -> String {
    format!("/admin/ehr-ids/{EHR_ID}/clear")
}

/// The path of the drift repair of the synthetic stored query.
const DISTRIBUTE: &str = "/admin/stored-queries/org.example::compositions/1.0.0/distribute";

/// The page the drift repair posts to.
const REPAIR_PAGE: &str = "/stored-queries/org.example::compositions";

/// The clear a replica that held a collision answers.
fn cleared(claimants: &[&str]) -> String {
    let claimants = claimants
        .iter()
        .map(|claimant| serde_json::to_string(claimant).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"ehr_id":"{EHR_ID}","held":"collision","claimants":[{claimants}],"indexed":true,"sessions":2,"scope":"replica"}}"#
    )
}

/// A distribution one member took and one did not, as the gateway answers
/// it with `207` (§12.7, §11.4).
const PARTIAL_DISTRIBUTION: &str = r#"{"name":"org.example::compositions","type":"AQL","version":"1.0.0",
    "saved":"2026-10-05T08:00:00Z","q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
    "meta":{"federation":{"complete":false,"endpoints":[
      {"id":"node-a-pub","status":"active","latency_ms":12},
      {"id":"node-b-pub","status":"time-out","error":"no answer within the per-node budget","latency_ms":2000}]},
    "registry":"held"}}"#;

/// Mounts on `listener` an answer of `status` with `body` to a `POST` of
/// `route` that carries the operator's own token.
async fn answers(listener: &Server, route: &str, status: u16, body: String) {
    Mock::given(method("POST"))
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

/// The `[admin]` table naming each of `replicas` by its name and URL.
fn admin_table(replicas: &[(&str, String)]) -> String {
    let mut table = String::new();
    for (name, url) in replicas {
        table.push_str("[[admin.replicas]]\nname = \"");
        table.push_str(name);
        table.push_str("\"\nadmin_url = \"");
        table.push_str(url);
        table.push_str("\"\n");
    }
    table
}

/// A console over the gateway at `gateway` with the admin listeners
/// `replicas`, and one signed-in operator.
fn signed_in_console(
    gateway: &str,
    replicas: &[(&str, String)],
) -> Result<(axum::Router, SessionId), Box<dyn Error>> {
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{gateway}\"\n\n[session]\nsecure_cookie = false\n\n{}",
        admin_table(replicas)
    ))?;
    let session = state.sessions().establish(signed_in())?;
    Ok((service, session))
}

/// The form body of an action with `fields`.
fn form(fields: &[(&str, &str)]) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        form.append_pair(name, value);
    }
    form.finish()
}

/// The server function the action form on the page `route` calls, and the
/// body it posts: the clear on Integrity, and the drift repair on a stored
/// query's page, whose form names the query in a hidden field.
fn function_of(route: &str, body: String) -> (&'static str, String) {
    let path = route.split('?').next().unwrap_or(route);
    match path.strip_prefix("/stored-queries/") {
        Some(name) => {
            let name = url::form_urlencoded::parse(format!("name={name}").as_bytes())
                .map(|(_, value)| value.into_owned())
                .next()
                .unwrap_or_default();
            let mut form = url::form_urlencoded::Serializer::new(body);
            form.append_pair("name", &name);
            ("/api/repair", form.finish())
        }
        None => ("/api/clear", body),
    }
}

/// A `POST` of `body` from the page `route`, as the console's own page
/// sends it with no WebAssembly, carrying `session`'s cookie.
fn act(route: &str, body: String, session: &SessionId) -> Result<Request<Body>, Box<dyn Error>> {
    let (function, body) = function_of(route, body);
    crate::support::answer_post(
        (function, route),
        &body,
        Some(session),
        &[("sec-fetch-site", "same-origin")],
    )
}

/// Posts the action `body` to the page `route` and reads the page it
/// answers with.
async fn acted(
    service: &axum::Router,
    session: &SessionId,
    route: &str,
    body: String,
) -> Result<String, Box<dyn Error>> {
    let (response, text) = send(service, act(route, body, session)?).await?;
    assert!(!text.contains(OPERATOR_TOKEN), "{text}");
    assert!(text.contains("</html>"), "{}: {text}", response.status());
    Ok(text)
}

/// How many requests `listener` received.
async fn asked(listener: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?
        .len())
}

/// The confirmed clear of [`EHR_ID`] at `replica`.
fn clear_at(replica: &str) -> String {
    form(&[("replica", replica), ("ehr_id", EHR_ID), ("confirm", "yes")])
}

/// The confirmed drift repair of the synthetic stored query at every
/// member.
fn repair_every_member() -> String {
    form(&[("version", "1.0.0"), ("endpoint", "*"), ("confirm", "yes")])
}

#[tokio::test]
async fn the_clear_at_every_replica_reaches_each_with_the_operators_token_and_names_it()
-> Result<(), Box<dyn Error>> {
    let first = Server::start().await;
    let second = Server::start().await;
    answers(&first, &clear_path(), 200, cleared(&["node-a", "node-b"])).await;
    answers(&second, &clear_path(), 200, cleared(&["node-a", "node-c"])).await;
    let replicas = [("gateway-0", first.uri()), ("gateway-1", second.uri())];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let html = acted(&service, &session, "/integrity", clear_at("*")).await?;
    assert!(
        html.contains("gateway-0 cleared it: it held it at node-a, node-b."),
        "{html}"
    );
    assert!(
        html.contains("gateway-1 cleared it: it held it at node-a, node-c."),
        "{html}"
    );
    assert!(html.contains("The EHR id was cleared."), "{html}");
    assert_eq!(1, asked(&first).await?);
    assert_eq!(1, asked(&second).await?);
    Ok(())
}

#[tokio::test]
async fn the_clear_at_one_replica_reaches_that_replica_alone() -> Result<(), Box<dyn Error>> {
    let first = Server::start().await;
    let second = Server::start().await;
    answers(&first, &clear_path(), 200, cleared(&["node-a", "node-b"])).await;
    answers(&second, &clear_path(), 200, cleared(&["node-a", "node-b"])).await;
    let replicas = [("gateway-0", first.uri()), ("gateway-1", second.uri())];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let html = acted(&service, &session, "/integrity", clear_at("gateway-1")).await?;
    assert!(html.contains("gateway-1 cleared it"), "{html}");
    assert!(!html.contains("gateway-0 cleared it"), "{html}");
    assert_eq!(0, asked(&first).await?);
    assert_eq!(1, asked(&second).await?);
    let received = second
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    let request = received.first().ok_or("the replica was asked")?;
    assert_eq!("POST", request.method.as_str());
    assert_eq!(request.body, Vec::<u8>::new());
    Ok(())
}

#[tokio::test]
async fn the_drift_repair_goes_to_the_first_replica_with_the_members_named()
-> Result<(), Box<dyn Error>> {
    let first = Server::start().await;
    let second = Server::start().await;
    Mock::given(method("POST"))
        .and(path(DISTRIBUTE))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .and(header("openEHR-federation-endpoint", "*"))
        .respond_with(
            ResponseTemplate::new(207)
                .insert_header("content-type", "application/json")
                .set_body_string(PARTIAL_DISTRIBUTION),
        )
        .expect(1)
        .mount(&first)
        .await;
    let replicas = [("gateway-0", first.uri()), ("gateway-1", second.uri())];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let html = acted(&service, &session, REPAIR_PAGE, repair_every_member()).await?;
    assert!(
        html.contains(
            "Version 1.0.0 was sent again by gateway-0, and some endpoints did not take it."
        ),
        "{html}"
    );
    assert!(html.contains("node-b-pub"), "{html}");
    assert!(
        html.contains("no answer within the per-node budget"),
        "{html}"
    );
    assert_eq!(0, asked(&second).await?);
    Ok(())
}

// The confirmation is a step of its own: a form posted without it asks no
// replica anything, whichever action it is.
#[tokio::test]
async fn an_action_without_its_confirmation_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(
        &listener,
        &clear_path(),
        200,
        cleared(&["node-a", "node-b"]),
    )
    .await;
    let replicas = [("gateway-0", listener.uri())];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let unconfirmed = form(&[("replica", "*"), ("ehr_id", EHR_ID)]);
    let html = acted(&service, &session, "/integrity", unconfirmed).await?;
    assert!(
        html.contains("Confirm the clear in its confirmation step to run it."),
        "{html}"
    );
    let unconfirmed = form(&[("version", "1.0.0"), ("endpoint", "*")]);
    let html = acted(&service, &session, REPAIR_PAGE, unconfirmed).await?;
    assert!(
        html.contains("Confirm the drift repair in its confirmation step to run it."),
        "{html}"
    );
    assert_eq!(0, asked(&listener).await?);
    Ok(())
}

// A refusal of the admin listener is shown with its status and stable code,
// never as an empty answer, and a 401 links back to sign-in.
#[tokio::test]
async fn a_refusal_shows_the_replica_its_status_and_its_code() -> Result<(), Box<dyn Error>> {
    for (status, code) in [
        (401, "unauthenticated"),
        (403, "scope-insufficient"),
        (404, "not-found"),
        (409, "conflict"),
    ] {
        let listener = Server::start().await;
        answers(
            &listener,
            &clear_path(),
            status,
            format!(r#"{{"code":"{code}","message":"refused"}}"#),
        )
        .await;
        let replicas = [("gateway-0", listener.uri())];
        let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
        let html = acted(&service, &session, "/integrity", clear_at("*")).await?;
        assert!(html.contains(r#"role="alert""#), "{status}: {html}");
        if status == 401 {
            assert!(
                html.contains(&format!(
                    "Replica gateway-0 did not accept your sign-in for the clear: 401 ({code})."
                )),
                "{html}"
            );
            assert!(html.contains("Sign in again"), "{html}");
        } else {
            assert!(
                html.contains(&format!(
                    "Replica gateway-0 refused the clear: {status} ({code})."
                )),
                "{html}"
            );
        }
    }
    Ok(())
}

// Each replica holds its own routing state, so one that cannot be reached
// is named, and the clear still runs at the others.
#[tokio::test]
async fn an_unreachable_replica_is_named_and_the_others_still_clear() -> Result<(), Box<dyn Error>>
{
    let listener = Server::start().await;
    answers(
        &listener,
        &clear_path(),
        200,
        cleared(&["node-a", "node-b"]),
    )
    .await;
    let replicas = [
        ("gateway-0", String::from("http://127.0.0.1:9/")),
        ("gateway-1", listener.uri()),
    ];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let html = acted(&service, &session, "/integrity", clear_at("*")).await?;
    assert!(
        html.contains("Replica gateway-0: its admin listener gave no answer, so whether the clear ran there is unknown."),
        "{html}"
    );
    assert!(html.contains("gateway-1 cleared it"), "{html}");
    assert!(
        html.contains("The EHR id was cleared at some replicas."),
        "{html}"
    );
    Ok(())
}

// What a replica answers reaches the page as text, so a code or a claimant
// arrives escaped, never as markup.
#[tokio::test]
async fn hostile_values_in_a_replicas_answer_render_escaped() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(
        &listener,
        &clear_path(),
        200,
        cleared(&["<script>alert(1)</script>", r#"" onmouseover="x"#]),
    )
    .await;
    let refusing = Server::start().await;
    answers(
        &refusing,
        &clear_path(),
        403,
        String::from(r#"{"code":"<img src=x onerror=alert(1)>"}"#),
    )
    .await;
    let replicas = [("gateway-0", listener.uri()), ("gateway-1", refusing.uri())];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let html = acted(&service, &session, "/integrity", clear_at("*")).await?;
    for escaped in [
        "&lt;script&gt;alert(1)&lt;/script&gt;",
        "&lt;img src=x onerror=alert(1)&gt;",
    ] {
        assert!(html.contains(escaped), "{escaped}: {html}");
    }
    for raw in ["<script>alert", "<img src=x"] {
        assert!(!html.contains(raw), "{raw}: {html}");
    }
    Ok(())
}

// A refused or unreachable action is logged as the action it is, naming the
// replica, the status and the code, and never the ehr_id or the stored query
// the operator named.
#[tokio::test]
async fn a_refused_action_is_logged_as_that_action_with_no_identifier() -> Result<(), Box<dyn Error>>
{
    let captured = Captured::default();
    let _logging = captured.logging();
    let refusing = Server::start().await;
    answers(
        &refusing,
        &clear_path(),
        403,
        String::from(r#"{"code":"scope-insufficient"}"#),
    )
    .await;
    answers(
        &refusing,
        DISTRIBUTE,
        409,
        String::from(r#"{"code":"conflict"}"#),
    )
    .await;
    let replicas = [
        ("gateway-0", refusing.uri()),
        ("gateway-1", String::from("http://127.0.0.1:9/")),
    ];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    acted(&service, &session, "/integrity", clear_at("*")).await?;
    acted(&service, &session, REPAIR_PAGE, repair_every_member()).await?;

    let logged = captured.text();
    for line in [
        r#"a replica refused an operator's admin action status=403 Forbidden code="scope-insufficient" replica="gateway-0" action="clear""#,
        r#"a replica refused an operator's admin action status=409 Conflict code="conflict" replica="gateway-0" action="drift repair""#,
        r#"a replica's admin listener could not be reached for an operator's admin action error=the gateway call failed replica="gateway-1" action="clear""#,
    ] {
        assert!(logged.contains(line), "{line}: {logged}");
    }
    for as_a_view in ["refused a view", "answered a view"] {
        assert!(!logged.contains(as_a_view), "{as_a_view}: {logged}");
    }
    for named in [EHR_ID, OPERATOR_TOKEN] {
        assert!(!logged.contains(named), "{named}: {logged}");
    }
    Ok(())
}

// A refusal's code reaches the page as text in either refusal line, the
// 401's and every other's, so a hostile code arrives escaped.
#[tokio::test]
async fn a_hostile_refusal_code_of_an_action_renders_escaped() -> Result<(), Box<dyn Error>> {
    for status in [401, 409] {
        let listener = Server::start().await;
        answers(
            &listener,
            DISTRIBUTE,
            status,
            String::from(r#"{"code":"\"><script>alert(1)</script><img src=x onerror=alert(2)>"}"#),
        )
        .await;
        let replicas = [("gateway-0", listener.uri())];
        let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
        let html = acted(&service, &session, REPAIR_PAGE, repair_every_member()).await?;
        for escaped in [
            "&lt;script&gt;alert(1)&lt;/script&gt;",
            "&lt;img src=x onerror=alert(2)&gt;",
        ] {
            assert!(html.contains(escaped), "{status}: {escaped}: {html}");
        }
        for raw in ["<script>alert", "<img src=x"] {
            assert!(!html.contains(raw), "{status}: {raw}: {html}");
        }
    }
    Ok(())
}

// A drift repair's report carries the registry's word and each member's
// error text, each a hostile value that arrives as text.
#[tokio::test]
async fn hostile_values_in_a_drift_repair_answer_render_escaped() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let body = r#"{"name":"<script>alert(1)</script>","type":"AQL","version":"1.0.0",
        "saved":"2026-10-05T08:00:00Z","q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
        "meta":{"federation":{"complete":false,"endpoints":[
          {"id":"node-a-pub","status":"node-error","error":"<img src=x onerror=alert(2)>","latency_ms":5}]},
        "registry":"<b onmouseover=alert(3)>held</b>"}}"#;
    answers(&listener, DISTRIBUTE, 207, String::from(body)).await;
    let replicas = [("gateway-0", listener.uri())];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let html = acted(&service, &session, REPAIR_PAGE, repair_every_member()).await?;
    for raw in ["<script>alert", "<img src=x", "<b onmouseover"] {
        assert!(!html.contains(raw), "{raw}: {html}");
    }
    Ok(())
}

// A clear another origin posts asks no replica (`Sec-Fetch-Site`, Fetch
// Metadata Request Headers §2.1).
#[tokio::test]
async fn a_clear_from_another_origin_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(
        &listener,
        &clear_path(),
        200,
        cleared(&["node-a", "node-b"]),
    )
    .await;
    let replicas = [("gateway-0", listener.uri())];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let request = crate::support::answer_post(
        ("/api/clear", "/integrity"),
        &clear_at("*"),
        Some(&session),
        &[("sec-fetch-site", "cross-site")],
    )?;
    let (response, _body) = send(&service, request).await?;
    assert_eq!(StatusCode::FORBIDDEN, response.status());
    assert_eq!(0, asked(&listener).await?);
    Ok(())
}

/// A stub gateway answering the integrity and stored-query reads, with one
/// collision naming `ehr_id` and one stored query called `name`.
async fn views_gateway_naming(ehr_id: &str, name: &str) -> Result<Server, Box<dyn Error>> {
    let gateway = Server::start().await;
    let (ehr_id, name) = (serde_json::to_string(ehr_id)?, serde_json::to_string(name)?);
    let options = crate::stub::options_example()?;
    let reads = [
        ("/", options),
        (
            "/operator/incidents",
            format!(
                r#"{{"counts":{{"EhrIdCollision":1}},"recent":[{{"kind":"EhrIdCollision","at":"2026-10-05T08:00:00Z",
                "description":"an ehr_id is claimed by endpoints [node-a-pub, node-b-pub]",
                "creating_system_id":null,"ehr_id":{ehr_id},"detection":"ask-all",
                "endpoints":["node-a-pub","node-b-pub"],"nodes":[]}}],"cleared":[]}}"#
            ),
        ),
        (
            "/operator/stored-queries",
            format!(
                r#"{{"items":[{{"name":{name},"type":"AQL","version":"1.0.0","saved":"2026-10-05T08:00:00Z",
                "q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","read_only":false}}],"offset":0,"total":1}}"#,
            ),
        ),
    ];
    for (route, body) in reads {
        let verb = if route == "/" { "OPTIONS" } else { "GET" };
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(body),
            )
            .mount(&gateway)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(format!("/operator/stored-queries/{}", name.trim_matches('"'))))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(format!(
                    r#"{{"items":[{{"name":{name},"type":"AQL","version":"1.0.0","saved":"2026-10-05T08:00:00Z",
                    "q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","read_only":false}}],"offset":0,"total":1}}"#,
                )),
        )
        .mount(&gateway)
        .await;
    Ok(gateway)
}

// The clear's field reaches the page as text, so an ehr_id an incident names
// arrives escaped, never as markup, in a text and in an attribute alike.
#[tokio::test]
async fn hostile_values_in_the_form_fields_render_escaped() -> Result<(), Box<dyn Error>> {
    let gateway = views_gateway_naming(r#""><script>alert(1)</script>"#, NAME).await?;
    let replicas = [("gateway-0", String::from("http://127.0.0.1:9/"))];
    let (service, session) = signed_in_console(&gateway.uri(), &replicas)?;
    let (_, integrity) = send(&service, get_as("/integrity", &session)?).await?;
    let start = integrity
        .find(r#"<datalist id="clear-ehr-ids">"#)
        .ok_or("the clear's ehr_ids")?;
    let ehr_ids = integrity
        .get(start..)
        .and_then(|rest| rest.split_once("</datalist>"))
        .map(|(inside, _)| inside);
    let ehr_ids = ehr_ids.ok_or("the datalist ends")?;
    assert!(
        ehr_ids.contains("&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{ehr_ids}"
    );
    assert!(!ehr_ids.contains("<script"), "{ehr_ids}");
    Ok(())
}

// A console configured with no admin listener offers no admin action; one
// configured with replicas offers each beside its page, behind its
// confirmation, the clear naming the incidents' ehr_id.
#[tokio::test]
async fn the_actions_are_offered_only_with_an_admin_listener() -> Result<(), Box<dyn Error>> {
    let gateway = views_gateway_naming(EHR_ID, NAME).await?;
    let (service, session) = signed_in_console(&gateway.uri(), &[])?;
    let (_, integrity) = send(&service, get_as("/integrity", &session)?).await?;
    let (_, stored) = send(&service, get_as(REPAIR_PAGE, &session)?).await?;
    assert!(!integrity.contains("Clear a held EHR id"), "{integrity}");
    assert!(!integrity.contains(r#"name="ehr_id""#), "{integrity}");
    assert!(stored.contains("The AQL of version 1.0.0"), "{stored}");
    assert!(!stored.contains("Send a version again"), "{stored}");

    let replicas = [
        ("gateway-0", String::from("http://127.0.0.1:9/")),
        ("gateway-1", String::from("http://127.0.0.1:9/")),
    ];
    let (service, session) = signed_in_console(&gateway.uri(), &replicas)?;
    let (_, integrity) = send(&service, get_as("/integrity", &session)?).await?;
    assert!(integrity.contains("Clear a held EHR id"), "{integrity}");
    assert!(integrity.contains(r#"action="/api/clear""#), "{integrity}");
    assert!(
        integrity.contains(&format!(r#"<option value="{EHR_ID}">"#)),
        "{integrity}"
    );
    assert!(
        integrity.contains(r#"<option value="gateway-1">gateway-1</option>"#),
        "{integrity}"
    );
    assert!(
        integrity.contains(r#"name="confirm" value="yes""#),
        "{integrity}"
    );
    let (_, stored) = send(&service, get_as(REPAIR_PAGE, &session)?).await?;
    assert!(stored.contains("Send a version again"), "{stored}");
    assert!(stored.contains("Sent by replica gateway-0"), "{stored}");
    assert!(stored.contains(r#"name="confirm" value="yes""#), "{stored}");
    assert!(!stored.contains(OPERATOR_TOKEN), "{stored}");
    assert!(!integrity.contains(OPERATOR_TOKEN), "{integrity}");
    Ok(())
}

// A console with no admin listener answers an action posted to it anyway
// with a refusal that says so, and asks nothing.
#[tokio::test]
async fn an_action_on_a_console_with_no_admin_listener_is_refused() -> Result<(), Box<dyn Error>> {
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &[])?;
    let html = acted(&service, &session, "/integrity", clear_at("*")).await?;
    assert!(html.contains("configured with no admin listener"), "{html}");
    Ok(())
}
