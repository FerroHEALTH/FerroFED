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

use crate::configure::{
    answers, bodies, changes, configured, form, form_with, page, posted, refusal_of,
    signed_in_console,
};

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
       "secrets":[{"key":"oauth.client_secret","reference":"/var/lib/s/\"><img src=x onerror=1>"}]}],
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
/// Asserts that no password field of `body` is filled.
fn no_password_filled(body: &str) {
    for input in body.split("<input").skip(1) {
        let tag = input.split('>').next().unwrap_or_default();
        if tag.contains(r#"type="password""#) {
            assert!(
                !tag.contains("value="),
                "a password field is never filled: {tag}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_page_shows_each_owner_the_secrets_by_reference_and_what_awaits()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let body = page(&service, &session, "/configuration/credentials").await?;
    assert!(body.contains("Credentials and bindings"), "{body}");
    assert!(
        body.contains("<code>password</code>: set, read from <code>/run/secrets/node-a</code>"),
        "{body}"
    );
    assert!(body.contains("Add credentials"), "{body}");
    assert!(
        body.contains(r#"href="/configuration/credentials/node-c-pub""#),
        "{body}"
    );
    assert!(
        !body.contains(r#"href="/configuration/credentials/node-a-pub""#),
        "{body}"
    );
    assert!(body.contains("Add binding section"), "{body}");
    assert!(
        body.contains(r#"commandfor="cfg-remove-credentials-node-c-pub-dialog""#),
        "a console section's row offers its own confirmed removal: {body}"
    );
    assert!(
        body.contains(r#"name="form[resource]" value="credentials:node-c-pub""#),
        "{body}"
    );
    let cell = body
        .split("<tr>")
        .filter_map(|row| row.split_once(r#"data-label="Actions""#))
        .map(|(_, cell)| cell)
        .find(|cell| cell.contains("cfg-remove-credentials-node-c-pub-menu"))
        .ok_or("console section node-c-pub's actions cell")?;
    assert!(
        cell.contains(r#"<dialog id="cfg-remove-credentials-node-c-pub-dialog""#),
        "the removal dialog sits in the section's actions cell, beside its row menu: {cell}"
    );
    assert!(
        !body.contains("cfg-remove-credentials-node-a-pub-dialog"),
        "a section the file provides offers none: {body}"
    );
    assert!(
        body.contains("Revision 2 awaits your confirmation"),
        "{body}"
    );
    assert!(
        body.contains(
            r#"title="2026-10-15T09:02:00Z" class="fui-time">2026-10-15 09:02:00 UTC</time>"#
        ),
        "the deadline reads in UTC, the instant as sent in its title: {body}"
    );
    assert!(body.contains(r#"id="confirm-countdown""#), "{body}");
    assert!(body.contains("Approve revision 3"), "{body}");
    assert!(body.contains("Approved by: none yet"), "{body}");
    no_password_filled(&body);
    let binding = page(&service, &session, "/configuration/bindings/new").await?;
    assert!(binding.contains(r#"<option value="consent">"#), "{binding}");
    let edit = page(&service, &session, "/configuration/credentials/node-c-pub").await?;
    assert!(
        edit.contains(r#"<option value="basic" selected>"#),
        "the held grant is chosen: {edit}"
    );
    assert!(edit.contains(r#"type="password""#), "{edit}");
    assert!(
        edit.contains("fui-input-hold"),
        "a secret's value field carries the hold-to-show eye: {edit}"
    );
    assert!(
        edit.contains(r#"name="ferroui_secret" value="form[secret."#),
        "a secret's value field is marked, so a plain post never draws it back: {edit}"
    );
    assert!(
        edit.contains(r#"name="form[resource]" value="credentials:node-c-pub""#),
        "the removal sits on the credential's own page: {edit}"
    );
    assert!(edit.contains("Read from a file"), "{edit}");
    no_password_filled(&edit);
    let added = page(&service, &session, "/configuration/credentials/new").await?;
    no_password_filled(&added);
    assert_eq!(0, changes(&listener).await?);
    Ok(())
}

// NOTE: no specification governs this: our own design; every value the gateway
// answers reaches the page as text, the obligation of an `inner_html` sink.
#[tokio::test(flavor = "multi_thread")]
async fn every_value_of_a_section_is_escaped() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let mut body = page(&service, &session, "/configuration/credentials").await?;
    body.push_str(&page(&service, &session, "/configuration/credentials/node-c-pub").await?);
    for raw in ["<script>u</script>", "<img src=x onerror=1>", "<b>1</b>"] {
        assert!(!body.contains(raw), "{raw} reached the page raw: {body}");
    }
    assert!(body.contains("&lt;script&gt;u&lt;/script&gt;"), "{body}");
    Ok(())
}

/// The save of node C's credentials as basic authentication, the user `u`,
/// with the secret rows `secrets`.
fn save_credentials(secrets: &[(&str, &str, &str, &str)]) -> String {
    let mut fields: Vec<(String, String)> = vec![
        (String::from("setting_rows"), String::from("c0")),
        (String::from("setting][c0][key"), String::from("user")),
        (String::from("setting][c0][value"), String::from("u")),
    ];
    for (row, (key, choice, value, file)) in secrets.iter().enumerate() {
        let at = |part: &str| format!("secret][x{row}][{part}");
        fields.extend([
            (String::from("secret_rows"), format!("x{row}")),
            (at("key"), (*key).to_owned()),
            (at("choice"), (*choice).to_owned()),
            (at("value"), (*value).to_owned()),
            (at("file"), (*file).to_owned()),
        ]);
    }
    form_with(
        &[
            ("action", "save-credentials"),
            ("base", "3"),
            ("endpoint", "node-c-pub"),
            ("grant", "basic"),
            ("reason", "rotate the token"),
        ],
        &fields,
    )
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
    let html = posted(
        &service,
        &session,
        "/configuration/credentials/node-c-pub",
        save_credentials(&[
            ("password", "set", VALUE, ""),
            ("client_identity", "keep", "", ""),
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
    let changes: Vec<(&str, bool, bool)> = request
        .secrets
        .iter()
        .map(|change| (change.key.as_str(), change.value.is_some(), change.clear))
        .collect();
    assert_eq!(
        vec![
            ("password", true, false),
            ("oauth.client_secret", false, true)
        ],
        changes,
        "the value is set, and the client secret basic authentication no longer uses is cleared"
    );
    assert_eq!(
        Some(VALUE),
        request
            .secrets
            .first()
            .and_then(|change| change.value.as_ref())
            .map(ferrohealth_secret::secret::WriteOnly::expose)
    );
    for path in [
        "/configuration/credentials",
        "/configuration/credentials/node-c-pub",
    ] {
        let shown = page(&service, &session, path).await?;
        assert!(!shown.contains(VALUE), "{shown}");
    }
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
    for (secret, said, at) in [
        (
            ("password", "set", "", ""),
            "Type the value of password",
            "cfg-x-x0-value",
        ),
        (
            ("password", "file", VALUE, "secrets/p"),
            "is not an absolute path",
            "cfg-x-x0-file",
        ),
        (
            ("password", "file", "", ""),
            "Give the absolute path",
            "cfg-x-x0-file",
        ),
        (
            ("user", "set", VALUE, ""),
            "Name a secret this resource takes",
            "",
        ),
        (
            ("password", "guess", VALUE, ""),
            "Choose Keep, Set a value",
            "cfg-x-x0-choice",
        ),
    ] {
        let html = posted(
            &service,
            &session,
            "/configuration/credentials/node-c-pub",
            save_credentials(&[secret]),
        )
        .await?;
        assert!(html.contains(said), "{said}: {html}");
        assert!(!html.contains(VALUE), "{html}");
        // NOTE: no specification governs this: our own design; a secret the form does
        // not draw has no field to mark, and the answer at the top names it.
        if !at.is_empty() {
            let line = refusal_of(&html, at).ok_or_else(|| format!("{said} marks {at}: {html}"))?;
            assert!(line.contains(said), "{line}");
        }
        no_password_filled(&html);
    }
    for (fields, said, at) in [
        (
            [("endpoint", "node c"), ("grant", "basic")],
            "The endpoint_id is not",
            "cfg-endpoint",
        ),
        (
            [("endpoint", "node-c-pub"), ("grant", "carrier-pigeon")],
            "Choose how the gateway reaches the endpoint.",
            "cfg-grant",
        ),
    ] {
        let mut all = vec![("action", "save-credentials"), ("reason", "r")];
        all.extend(fields);
        let html = posted(
            &service,
            &session,
            "/configuration/credentials/new",
            form(&all),
        )
        .await?;
        assert!(html.contains(said), "{said}: {html}");
        let line = refusal_of(&html, at).ok_or_else(|| format!("{said} marks {at}: {html}"))?;
        assert!(line.contains(said), "{line}");
    }
    let html = configured(
        &service,
        &session,
        form(&[
            ("action", "save-binding"),
            ("section", "auth"),
            ("reason", "r"),
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
