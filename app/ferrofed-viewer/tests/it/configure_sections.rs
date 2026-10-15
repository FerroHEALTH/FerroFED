// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials and binding sections of the configuration page against a
//! stub admin listener: each marked with its owner and its secrets shown as
//! set with their files and never by value, the password field never
//! filled, a secret value sent to the replica alone and never on a page, in
//! an answer or in a log, the confirmation a running revision awaits and
//! the approval the newest one takes offered with their deadline and who
//! acts, and every value the gateway answers escaped.

use std::error::Error;

use ferrofed_registry::resource::SectionRequest;
use ferrofed_testkit::mock::Server;
use http::StatusCode;

use crate::configure::{answers, bodies, changes, configured, form, signed_in_console};
use crate::support::{get_as, send};

/// A synthetic secret value no page, answer or log may hold.
const VALUE: &str = "synthetic-console-secret-91f2";

/// Revision 2 runs awaiting its confirmation, and revision 3, saved, takes
/// a second operator's approval.
const STANDING: &str = r#"{"ui":"managed","head":3,"current":{"revision":2,"previous":1,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "applied_at":"2026-10-15T09:00:00Z",
    "awaiting":{"deadline":"2026-10-15T09:02:00Z","resources":["[credentials.\"node-c-pub\"]"]}},
    "running":2,"in_step":true,"refused":null,
    "review":{"revision":3,"author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-<b>1</b>"},
      "confirms":["[federation.consent]"],"approval":["[federation.consent]"],"approvals":[],"approved":false}}"#;

/// A credential the file provides, one the console holds with markup in
/// its settings and its reference, and the bindings this gateway offers.
const RESOURCES: &str = r#"{"running":2,"head":3,"members":[],"stored_query_registry":"absent",
    "stored_queries":[],
    "sections":[
      {"resource":{"credentials":"node-a-pub"},"owner":"file",
       "secrets":[{"key":"password","reference":"/run/secrets/node-a"}]},
      {"resource":{"credentials":"node-c-pub"},"owner":"console","state":"running",
       "settings":"{ user = \"<script>u</script>\" }",
       "secrets":[{"key":"bearer_token","reference":"/var/lib/s/\"><img src=x onerror=1>"}]}],
    "bindings":["localization","consent"]}"#;

/// The history, empty of markup.
const HISTORY: &str = r#"{"items":[],"offset":0,"total":0}"#;

/// What revision 3 changes against revision 2.
const DIFF: &str = r#"{"from":2,"to":3,"changes":[
    {"key":"federation.consent.disclose","change":"added","from":null,"to":"false"}]}"#;

/// Revision 3 as a save answers it.
const SAVED: &str = r#"{"id":3,"base":2,"origin":"operator","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:01:00Z","reason":"token","toml":"x = 1"}"#;

/// An admin listener with the standing above.
async fn listener() -> Server {
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
        ("GET", "/admin/config/revisions/2/diff/3"),
        200,
        DIFF,
    )
    .await;
    listener
}

#[tokio::test(flavor = "multi_thread")]
async fn the_page_shows_each_owner_the_secrets_by_reference_and_what_awaits()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let (response, body) = send(&service, get_as("/configuration", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("Credentials and bindings"), "{body}");
    assert!(
        body.contains("password: set, read from /run/secrets/node-a"),
        "{body}"
    );
    assert!(body.contains("Add onward credentials"), "{body}");
    assert!(
        body.contains("Edit the onward credentials of node-c-pub"),
        "{body}"
    );
    assert!(
        !body.contains("Edit the onward credentials of node-a-pub"),
        "{body}"
    );
    assert!(body.contains("Add a binding section"), "{body}");
    assert!(body.contains(r#"<option value="consent">"#), "{body}");
    assert!(
        body.contains(r#"<option value="credentials:node-c-pub">"#),
        "{body}"
    );
    assert!(
        !body.contains(r#"value="credentials:node-a-pub""#),
        "{body}"
    );
    assert!(body.contains("Confirm revision 2"), "{body}");
    assert!(
        body.contains("runs awaiting its confirmation until 2026-10-15T09:02:00Z"),
        "{body}"
    );
    assert!(body.contains("Approve revision 3"), "{body}");
    assert!(body.contains("Approved by: none yet"), "{body}");
    for input in body.split("<input").skip(1) {
        let tag = input.split('>').next().unwrap_or_default();
        if tag.contains(r#"type="password""#) {
            assert!(
                !tag.contains("value="),
                "a password field is never filled: {tag}"
            );
        }
    }
    assert_eq!(0, changes(&listener).await?);
    Ok(())
}

// NOTE: no specification governs this: our own design; every value the gateway
// answers reaches the page as text, the obligation of an `inner_html` sink.
#[tokio::test(flavor = "multi_thread")]
async fn every_value_of_a_section_is_escaped() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let (_response, body) = send(&service, get_as("/configuration", &session)?).await?;
    for raw in ["<script>u</script>", "<img src=x onerror=1>", "<b>1</b>"] {
        assert!(!body.contains(raw), "{raw} reached the page raw: {body}");
    }
    assert!(body.contains("&lt;script&gt;u&lt;/script&gt;"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_secret_value_goes_to_the_replica_alone() -> Result<(), Box<dyn Error>> {
    let logs = crate::secrets::Captured::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    let listener = listener().await;
    answers(
        &listener,
        ("PUT", "/admin/config/credentials/node-c-pub"),
        201,
        SAVED,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        form(&[
            ("action", "save-credentials"),
            ("base", "3"),
            ("endpoint", "node-c-pub"),
            ("settings", "{ user = \"u\" }"),
            ("secret_key", "bearer_token"),
            ("secret_value", VALUE),
            ("reason", "rotate the token"),
            ("confirm", "yes"),
        ]),
    )
    .await?;
    assert!(
        html.contains("Replica gateway-0 saved revision 3."),
        "{html}"
    );
    assert!(!html.contains(VALUE), "{html}");
    let sent = bodies(&listener, ("PUT", "/admin/config/credentials/node-c-pub")).await?;
    let [sent] = sent.as_slice() else {
        return Err(format!("one save, not {}", sent.len()).into());
    };
    let request: SectionRequest = serde_json::from_str(sent)?;
    assert_eq!("{ user = \"u\" }", request.settings);
    let [change] = request.secrets.as_slice() else {
        return Err("one secret change".into());
    };
    assert_eq!("bearer_token", change.key);
    assert_eq!(
        Some(VALUE),
        change
            .value
            .as_ref()
            .map(ferrofed_registry::secret::WriteOnly::expose)
    );
    let (_response, page) = send(&service, get_as("/configuration", &session)?).await?;
    assert!(!page.contains(VALUE), "{page}");
    let text = String::from_utf8_lossy(
        &logs
            .0
            .lock()
            .map_err(|_poisoned| "the capture buffer is poisoned")?,
    )
    .into_owned();
    assert!(!text.contains(VALUE), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_section_form_that_does_not_read_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    for (fields, said) in [
        (vec![("secret_value", VALUE)], "Name the key of the secret"),
        (
            vec![("secret_key", "bearer_token")],
            "Give the secret's value",
        ),
        (
            vec![
                ("secret_key", "bearer_token"),
                ("secret_value", VALUE),
                ("secret_file", "/run/secrets/t"),
            ],
            "exactly one of the secret",
        ),
        (vec![("endpoint", "node c")], "The endpoint_id is not"),
    ] {
        let mut all = vec![
            ("action", "save-credentials"),
            ("base", "3"),
            ("endpoint", "node-c-pub"),
            ("reason", "r"),
            ("confirm", "yes"),
        ];
        for (name, value) in fields {
            match all.iter_mut().find(|slot| slot.0 == name) {
                Some(slot) => slot.1 = value,
                None => all.push((name, value)),
            }
        }
        let html = configured(&service, &session, form(&all)).await?;
        assert!(html.contains(said), "{said}: {html}");
        assert!(!html.contains(VALUE), "{html}");
    }
    let html = configured(
        &service,
        &session,
        form(&[
            ("action", "save-binding"),
            ("section", "auth"),
            ("reason", "r"),
            ("confirm", "yes"),
        ]),
    )
    .await?;
    assert!(html.contains("Choose the binding section."), "{html}");
    assert_eq!(0, changes(&listener).await?);
    Ok(())
}

/// The running revision 2, confirmed.
const CONFIRMED: &str = r#"{"revision":2,"previous":1,"outcome":"applied","needs_restart":[],
    "applied_by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "applied_at":"2026-10-15T09:00:00Z",
    "confirmed":{"by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},"at":"2026-10-15T09:01:00Z"}}"#;

/// Revision 3 approved by a second operator.
const APPROVED: &str = r#"{"revision":3,"author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "confirms":["[federation.consent]"],"approval":["[federation.consent]"],
    "approvals":[{"by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-2"},"at":"2026-10-15T09:01:00Z"}],
    "approved":true}"#;

#[tokio::test]
async fn a_confirmation_an_approval_and_a_removal_post_to_their_routes()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("POST", "/admin/config/revisions/2/confirm"),
        200,
        CONFIRMED,
    )
    .await;
    answers(
        &listener,
        ("POST", "/admin/config/revisions/3/approve"),
        200,
        APPROVED,
    )
    .await;
    answers(
        &listener,
        ("POST", "/admin/config/bindings/consent/remove"),
        201,
        SAVED,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        form(&[("action", "confirm"), ("revision", "2"), ("confirm", "yes")]),
    )
    .await?;
    assert!(
        html.contains("Replica gateway-0 recorded the confirmation of revision 2."),
        "{html}"
    );
    let html = configured(
        &service,
        &session,
        form(&[("action", "approve"), ("revision", "3"), ("confirm", "yes")]),
    )
    .await?;
    assert!(html.contains("It may now be applied."), "{html}");
    assert_eq!(
        vec![String::new()],
        bodies(&listener, ("POST", "/admin/config/revisions/2/confirm")).await?
    );
    assert_eq!(
        vec![String::new()],
        bodies(&listener, ("POST", "/admin/config/revisions/3/approve")).await?
    );
    configured(
        &service,
        &session,
        form(&[
            ("action", "remove-section"),
            ("base", "3"),
            ("resource", "binding:consent"),
            ("reason", "back to the default"),
            ("confirm", "yes"),
        ]),
    )
    .await?;
    assert_eq!(
        vec![String::from(r#"{"base":3,"reason":"back to the default"}"#)],
        bodies(&listener, ("POST", "/admin/config/bindings/consent/remove")).await?
    );
    Ok(())
}

#[tokio::test]
async fn an_approval_the_gateway_refuses_shows_its_status_and_code() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("POST", "/admin/config/revisions/3/approve"),
        403,
        r#"{"message":"revision 3 is approved by an operator other than its author","code":"approval-own-revision"}"#,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        form(&[("action", "approve"), ("revision", "3"), ("confirm", "yes")]),
    )
    .await?;
    assert!(html.contains("403 (approval-own-revision)"), "{html}");
    Ok(())
}
