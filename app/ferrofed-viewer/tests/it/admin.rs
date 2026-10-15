// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's admin actions against stub admin listeners: the
//! collision clear and the drift repair reach the replica the operator
//! picked with the operator's own token, ask for confirmation first, name
//! the replica they acted on, show a refusal's status and code, and are
//! offered only where the console is configured with an admin listener.

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_viewer::admin::model::ActionAnswer;
use ferrofed_viewer::session::{COOKIE, SessionId};
use http::{Request, StatusCode};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, console, get_as, send, signed_in};

/// A synthetic `ehr_id` two members claim.
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The path of the clear of [`EHR_ID`] on an admin listener.
fn clear_path() -> String {
    format!("/admin/ehr-ids/{EHR_ID}/clear")
}

/// The path of the drift repair of the synthetic stored query.
const DISTRIBUTE: &str = "/admin/stored-queries/org.example::compositions/1.0.0/distribute";

/// The clear a replica that held a collision answers.
fn cleared(claimants: &[&str]) -> String {
    let claimants = claimants
        .iter()
        .map(|claimant| format!("\"{claimant}\""))
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
    replicas
        .iter()
        .map(|(name, url)| {
            [
                "[[admin.replicas]]\nname = \"",
                name,
                "\"\nadmin_url = \"",
                url,
                "\"\n",
            ]
            .concat()
        })
        .collect()
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

/// The form body of an admin action with `fields`.
fn form(fields: &[(&str, &str)]) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        form.append_pair(&format!("form[{name}]"), value);
    }
    form.finish()
}

/// A `POST` of the server function at `route` with `body`, as the console's
/// own hydrated page sends it, carrying `session`'s cookie.
fn act(route: &str, body: String, session: &SessionId) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(body))?)
}

/// Runs the action at `route` with `body` and reads its rendered answer.
async fn acted(
    service: &axum::Router,
    session: &SessionId,
    route: &str,
    body: String,
) -> Result<String, Box<dyn Error>> {
    let (response, text) = send(service, act(route, body, session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{text}");
    assert!(!text.contains(OPERATOR_TOKEN), "{text}");
    let answer: ActionAnswer = serde_json::from_str(&text)?;
    Ok(answer.html)
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

#[tokio::test]
async fn the_clear_at_every_replica_reaches_each_with_the_operators_token_and_names_it()
-> Result<(), Box<dyn Error>> {
    let first = Server::start().await;
    let second = Server::start().await;
    answers(&first, &clear_path(), 200, cleared(&["node-a", "node-b"])).await;
    answers(&second, &clear_path(), 200, cleared(&["node-a", "node-c"])).await;
    let replicas = [("gateway-0", first.uri()), ("gateway-1", second.uri())];
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &replicas)?;
    let html = acted(&service, &session, "/api/clear-ehr-id", clear_at("*")).await?;
    assert!(
        html.contains(&format!("Replica gateway-0 cleared {EHR_ID}.")),
        "{html}"
    );
    assert!(
        html.contains("a collision between node-a, node-b"),
        "{html}"
    );
    assert!(
        html.contains(&format!("Replica gateway-1 cleared {EHR_ID}.")),
        "{html}"
    );
    assert!(
        html.contains("a collision between node-a, node-c"),
        "{html}"
    );
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
    let html = acted(
        &service,
        &session,
        "/api/clear-ehr-id",
        clear_at("gateway-1"),
    )
    .await?;
    assert!(html.contains("Replica gateway-1 cleared"), "{html}");
    assert!(!html.contains("gateway-0"), "{html}");
    assert_eq!(0, asked(&first).await?);
    assert_eq!(1, asked(&second).await?);
    let received = second
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    let request = received.first().ok_or("the replica was asked")?;
    assert_eq!("POST", request.method.as_str());
    assert!(request.body.is_empty());
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
    let body = form(&[
        ("name", "org.example::compositions"),
        ("version", "1.0.0"),
        ("endpoints", "*"),
        ("confirm", "yes"),
    ]);
    let html = acted(&service, &session, "/api/repair-drift", body).await?;
    assert!(
        html.contains("Replica gateway-0 sent org.example::compositions version 1.0.0 again: 207."),
        "{html}"
    );
    assert!(html.contains("The registry's copy: held."), "{html}");
    assert!(html.contains("Incomplete."), "{html}");
    assert!(
        html.contains(r#"<th scope="row">node-b-pub</th><td>time-out</td>"#),
        "{html}"
    );
    assert!(html.contains("<tbody>"), "{html}");
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
    let html = acted(&service, &session, "/api/clear-ehr-id", unconfirmed).await?;
    assert!(
        html.contains("Tick the confirmation to run the clear."),
        "{html}"
    );
    let unconfirmed = form(&[
        ("name", "org.example::compositions"),
        ("version", "1.0.0"),
        ("endpoints", "*"),
    ]);
    let html = acted(&service, &session, "/api/repair-drift", unconfirmed).await?;
    assert!(
        html.contains("Tick the confirmation to run the drift repair."),
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
        let html = acted(&service, &session, "/api/clear-ehr-id", clear_at("*")).await?;
        assert!(html.contains(r#"<p role="alert">"#), "{status}: {html}");
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
    let html = acted(&service, &session, "/api/clear-ehr-id", clear_at("*")).await?;
    assert!(
        html.contains(
            "Replica gateway-0: its admin listener gave no answer, so whether the clear ran there is unknown."
        ),
        "{html}"
    );
    assert!(html.contains("Replica gateway-1 cleared"), "{html}");
    Ok(())
}

// What a replica answers reaches the page through `inner_html`, so a code,
// a claimant or a node's error arrives escaped, never as markup.
#[tokio::test]
async fn hostile_values_in_a_replicas_answer_render_escaped() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(
        &listener,
        &clear_path(),
        200,
        cleared(&["<script>alert(1)</script>", r#"\" onmouseover=\"x"#]),
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
    let html = acted(&service, &session, "/api/clear-ehr-id", clear_at("*")).await?;
    for escaped in [
        "&lt;script&gt;alert(1)&lt;/script&gt;",
        "&lt;img src=x onerror=alert(1)&gt;",
    ] {
        assert!(html.contains(escaped), "{escaped}: {html}");
    }
    for raw in ["<script", "<img"] {
        assert!(!html.contains(raw), "{raw}: {html}");
    }
    Ok(())
}

/// The confirmed drift repair of the synthetic stored query at every
/// member.
fn repair_every_member() -> String {
    form(&[
        ("name", "org.example::compositions"),
        ("version", "1.0.0"),
        ("endpoints", "*"),
        ("confirm", "yes"),
    ])
}

// A refusal's code reaches the page through `inner_html` in either refusal
// line, the 401's and every other's, so a hostile code arrives as text.
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
        let html = acted(
            &service,
            &session,
            "/api/repair-drift",
            repair_every_member(),
        )
        .await?;
        for escaped in [
            "&lt;script&gt;alert(1)&lt;/script&gt;",
            "&lt;img src=x onerror=alert(2)&gt;",
        ] {
            assert!(html.contains(escaped), "{status}: {escaped}: {html}");
        }
        for raw in ["<script", "<img"] {
            assert!(!html.contains(raw), "{status}: {raw}: {html}");
        }
    }
    Ok(())
}

// A drift repair's report carries the stored query's name, the registry's
// word and each member's error text, each a hostile value that arrives as
// text, the member's id in a table header included.
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
    let html = acted(
        &service,
        &session,
        "/api/repair-drift",
        repair_every_member(),
    )
    .await?;
    for escaped in [
        "&lt;script&gt;alert(1)&lt;/script&gt;",
        "&lt;img src=x onerror=alert(2)&gt;",
        "&lt;b onmouseover=alert(3)&gt;held&lt;/b&gt;",
    ] {
        assert!(html.contains(escaped), "{escaped}: {html}");
    }
    for raw in ["<script", "<img", "<b "] {
        assert!(!html.contains(raw), "{raw}: {html}");
    }
    Ok(())
}

// A page without its bundle posts the form plainly, and the console asks no
// replica for an answer no page would show.
#[tokio::test]
async fn a_plain_form_post_asks_no_replica() -> Result<(), Box<dyn Error>> {
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
    let request = Request::post("/api/clear-ehr-id")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/html")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(clear_at("*")))?;
    let (response, _body) = send(&service, request).await?;
    assert_ne!(StatusCode::OK, response.status());
    assert_eq!(0, asked(&listener).await?);
    Ok(())
}

/// A stub gateway answering the integrity and stored-query views, with one
/// collision naming [`EHR_ID`].
async fn views_gateway() -> Result<Server, Box<dyn Error>> {
    views_gateway_naming(EHR_ID, "org.example::compositions").await
}

/// A stub gateway answering the integrity and stored-query views, with one
/// collision naming `ehr_id` and one stored query called `name`.
async fn views_gateway_naming(ehr_id: &str, name: &str) -> Result<Server, Box<dyn Error>> {
    let gateway = Server::start().await;
    let (ehr_id, name) = (serde_json::to_string(ehr_id)?, serde_json::to_string(name)?);
    let reads = [
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
            "/operator/creating-systems",
            String::from(r#"{"items":[],"offset":0,"total":0}"#),
        ),
        (
            "/operator/stored-queries",
            format!(
                r#"{{"items":[{{"name":{name},"type":"AQL","version":"1.0.0","saved":"2026-10-05T08:00:00Z",
                "q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c"}}],"offset":0,"total":1}}"#,
            ),
        ),
    ];
    for (route, body) in reads {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(body),
            )
            .mount(&gateway)
            .await;
    }
    Ok(gateway)
}

// The fields reach the page through `inner_html`, so an ehr_id an incident
// names and a stored query's name arrive escaped, never as markup, in a
// text and in an attribute alike.
#[tokio::test]
async fn hostile_values_in_the_form_fields_render_escaped() -> Result<(), Box<dyn Error>> {
    let gateway = views_gateway_naming(
        r#""><script>alert(1)</script>"#,
        "<img src=x onerror=alert(1)>",
    )
    .await?;
    let replicas = [("gateway-0", String::from("http://127.0.0.1:9/"))];
    let (service, session) = signed_in_console(&gateway.uri(), &replicas)?;
    let (_, integrity) = send(&service, get_as("/integrity", &session)?).await?;
    let (_, stored) = send(&service, get_as("/stored-queries", &session)?).await?;
    let fields = |page: &str, id: &str| -> Option<String> {
        let start = page.find(&format!(r#"<datalist id="{id}">"#))?;
        let rest = page.get(start..)?;
        let end = rest.find("</datalist>")?;
        rest.get(..end).map(str::to_owned)
    };
    let ehr_ids = fields(&integrity, "clear-ehr-ids").ok_or("the clear's ehr_ids")?;
    assert!(
        ehr_ids.contains("&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{ehr_ids}"
    );
    assert!(!ehr_ids.contains("<script"), "{ehr_ids}");
    let names = fields(&stored, "repair-names").ok_or("the repair's names")?;
    assert!(
        names.contains("&lt;img src=x onerror=alert(1)&gt;"),
        "{names}"
    );
    assert!(!names.contains("<img"), "{names}");
    Ok(())
}

// A console configured with no admin listener offers no admin action; one
// configured with replicas offers each beside its view, behind its
// confirmation, the clear naming the incidents' ehr_id.
#[tokio::test]
async fn the_actions_are_offered_only_with_an_admin_listener() -> Result<(), Box<dyn Error>> {
    let gateway = views_gateway().await?;
    let (service, session) = signed_in_console(&gateway.uri(), &[])?;
    let (_, integrity) = send(&service, get_as("/integrity", &session)?).await?;
    let (_, stored) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert!(integrity.contains("Recent incidents"), "{integrity}");
    assert!(!integrity.contains("Clear a held ehr_id"), "{integrity}");
    assert!(!integrity.contains("/api/clear-ehr-id"), "{integrity}");
    assert!(stored.contains("Held versions"), "{stored}");
    assert!(!stored.contains("Repair stored-query drift"), "{stored}");
    assert!(!stored.contains("/api/repair-drift"), "{stored}");

    let replicas = [
        ("gateway-0", String::from("http://127.0.0.1:9/")),
        ("gateway-1", String::from("http://127.0.0.1:9/")),
    ];
    let (service, session) = signed_in_console(&gateway.uri(), &replicas)?;
    let (_, integrity) = send(&service, get_as("/integrity", &session)?).await?;
    assert!(integrity.contains("Clear a held ehr_id"), "{integrity}");
    assert!(
        integrity.contains(r#"action="/api/clear-ehr-id""#),
        "{integrity}"
    );
    assert!(
        integrity.contains(&format!(r#"<option value="{EHR_ID}">"#)),
        "{integrity}"
    );
    assert!(
        integrity.contains(r#"<option value="gateway-1">gateway-1</option>"#),
        "{integrity}"
    );
    assert!(integrity.contains(r#"id="clear-confirm""#), "{integrity}");
    assert!(
        integrity.contains(r#"<button type="submit" disabled>Clear the ehr_id</button>"#),
        "{integrity}"
    );
    let (_, stored) = send(&service, get_as("/stored-queries", &session)?).await?;
    assert!(stored.contains("Repair stored-query drift"), "{stored}");
    assert!(stored.contains("It goes to replica gateway-0"), "{stored}");
    assert!(stored.contains(r#"id="repair-confirm""#), "{stored}");
    assert!(!stored.contains(OPERATOR_TOKEN), "{stored}");
    assert!(!integrity.contains(OPERATOR_TOKEN), "{integrity}");
    Ok(())
}

// A console with no admin listener answers an action posted to it anyway
// with a refusal that says so, and asks nothing.
#[tokio::test]
async fn an_action_on_a_console_with_no_admin_listener_is_refused() -> Result<(), Box<dyn Error>> {
    let (service, session) = signed_in_console("http://127.0.0.1:9/", &[])?;
    let html = acted(&service, &session, "/api/clear-ehr-id", clear_at("*")).await?;
    assert!(html.contains("configured with no admin listener"), "{html}");
    Ok(())
}
