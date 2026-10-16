// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page's refusals against a stub admin listener: a
//! refusal shows its status and code, a configuration-invalid one the key
//! the gateway names, a bare `404` is a refusal, a form that is not
//! confirmed or not readable and a plain form post ask no replica, a save
//! whose held member cannot be read sends nothing, and a refused action is
//! logged as what it is.

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_viewer::session::COOKIE;
use http::{Request, StatusCode};

use crate::configure::{
    answered, answers, changes, configured, form, form_with, listener, query_row, save_c,
    signed_in_console,
};
use crate::support::{Captured, OPERATOR_TOKEN, get_as, send};
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
