// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials and binding sections of the configuration page in the
//! browser, against a stub admin listener: a credential saved with a secret
//! value the page never shows again, the confirmation the running revision
//! awaits, and the approval the newest revision takes, each without a
//! console error.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::mock::Server;
use ferrofed_viewer::app::PRODUCT;
use thirtyfour::By;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// A synthetic secret value the page never shows.
const VALUE: &str = "synthetic-journey-secret-3d8e";

/// Revision 2 runs awaiting its confirmation, and revision 3, saved, takes
/// a second operator's approval.
const STANDING: &str = r#"{"ui":"managed","head":3,"current":{"revision":2,"previous":1,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "applied_at":"2026-10-15T09:00:00Z",
    "awaiting":{"deadline":"2026-10-15T09:02:00Z","resources":["[credentials.\"node-a-pub\"]"]}},
    "running":2,"in_step":true,"refused":null,
    "review":{"revision":3,"author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
      "confirms":["[federation.consent]"],"approval":["[federation.consent]"],"approvals":[],"approved":false}}"#;

/// The console's credential of member A, and the bindings offered.
const RESOURCES: &str = r#"{"running":2,"head":3,"members":[],"stored_query_registry":"absent",
    "stored_queries":[],
    "sections":[{"resource":{"credentials":"node-a-pub"},"owner":"console","state":"running",
      "settings":"{}","secrets":[{"key":"bearer_token","reference":"/var/lib/ferrofed/secrets/a.secret"}]}],
    "bindings":["localization","consent"]}"#;

/// No history.
const HISTORY: &str = r#"{"items":[],"offset":0,"total":0}"#;

/// What revision 3 changes against revision 2.
const DIFF_3: &str = r#"{"from":2,"to":3,"changes":[
    {"key":"federation.consent.disclose","change":"added","from":null,"to":"false"}]}"#;

/// What revision 4, the save, changes against revision 2.
const DIFF_4: &str = r#"{"from":2,"to":4,"changes":[
    {"key":"credentials.node-a-pub.bearer_token_file","change":"changed",
     "from":"\"/var/lib/ferrofed/secrets/a.secret\"","to":"\"/var/lib/ferrofed/secrets/b.secret\""}]}"#;

/// Revision 4 as the save answers it.
const SAVED: &str = r#"{"id":4,"base":3,"origin":"operator","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:01:00Z","reason":"rotate","toml":""}"#;

/// Revision 2 confirmed.
const CONFIRMED: &str = r#"{"revision":2,"previous":1,"outcome":"applied","needs_restart":[],
    "applied_by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "applied_at":"2026-10-15T09:00:00Z",
    "confirmed":{"by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},"at":"2026-10-15T09:01:00Z"}}"#;

/// Revision 3 approved by a second operator.
const APPROVED: &str = r#"{"revision":3,"author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "confirms":["[federation.consent]"],"approval":["[federation.consent]"],
    "approvals":[{"by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-2"},"at":"2026-10-15T09:01:00Z"}],
    "approved":true}"#;

/// A stub admin listener answering every route the page takes.
async fn listener() -> Server {
    let listener = Server::start().await;
    for (verb, route, status, body) in [
        ("GET", "/admin/config", 200, STANDING),
        ("GET", "/admin/config/resources", 200, RESOURCES),
        ("GET", "/admin/config/revisions", 200, HISTORY),
        ("GET", "/admin/config/revisions/2/diff/3", 200, DIFF_3),
        ("GET", "/admin/config/revisions/2/diff/4", 200, DIFF_4),
        ("PUT", "/admin/config/credentials/node-a-pub", 201, SAVED),
        ("POST", "/admin/config/revisions/2/confirm", 200, CONFIRMED),
        ("POST", "/admin/config/revisions/3/approve", 200, APPROVED),
    ] {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_raw(body.as_bytes().to_vec(), "application/json"),
            )
            .mount(&listener)
            .await;
    }
    listener
}

/// The submit of the form under the heading `heading`.
fn submit(heading: &str) -> By {
    By::XPath(format!(
        "//section[h2 = '{heading}']//button[@type = 'submit']"
    ))
}

/// Replaces the text of the field `id` with `text`.
async fn fill(browser: &Browser, id: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let field = browser.element(By::Id(id)).await?;
    field.clear().await?;
    field.send_keys(text).await?;
    Ok(())
}

/// Loads the page and waits until its forms run.
async fn hydrated(browser: &Browser, stack: &Stack) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url("/configuration")).await?;
    browser
        .titled(&format!("Configuration · {PRODUCT}"))
        .await?;
    browser
        .enabled(submit("Add onward credentials"))
        .await
        .map_err(|error| format!("the configuration page never hydrated: {error}").into())
}

/// Ticks the confirmation `id` and submits the form under `heading`.
async fn submitted(browser: &Browser, id: &str, heading: &str) -> Result<(), Box<dyn Error>> {
    browser.element(By::Id(id)).await?.click().await?;
    browser.element(submit(heading)).await?.click().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_credential_is_saved_with_a_secret_and_the_running_change_confirmed_and_approved()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        hydrated(browser, &stack).await?;
        browser
            .text(
                "td",
                "bearer_token: set, read from /var/lib/ferrofed/secrets/a.secret",
            )
            .await?;
        browser.text("h2", "Confirm revision 2").await?;
        browser.text("h2", "Approve revision 3").await?;
        let password = browser
            .element(By::Id("edit-credentials-node-a-pub-secret_value"))
            .await?;
        assert_eq!(Some(String::new()), password.prop("value").await?);
        browser
            .console_clean("the hydration of the credentials and bindings")
            .await?;

        // A secret value goes to the replica in the save and is never shown
        // again: the answer and the page drawn afterwards hold none.
        fill(
            browser,
            "edit-credentials-node-a-pub-secret_key",
            "bearer_token",
        )
        .await?;
        fill(browser, "edit-credentials-node-a-pub-secret_value", VALUE).await?;
        fill(browser, "edit-credentials-node-a-pub-reason", "rotate").await?;
        submitted(
            browser,
            "edit-credentials-node-a-pub-confirm",
            "Edit the onward credentials of node-a-pub",
        )
        .await?;
        browser
            .text("strong", "Replica gateway-0 saved revision 4.")
            .await?;
        let source = browser.driver().source().await?;
        assert!(!source.contains(VALUE), "the page shows the secret");
        browser.console_clean("a saved credential").await?;

        // The confirmation of the running revision before its deadline.
        hydrated(browser, &stack).await?;
        submitted(browser, "confirm-revision-confirm", "Confirm revision 2").await?;
        browser
            .text(
                "strong",
                "Replica gateway-0 recorded the confirmation of revision 2.",
            )
            .await?;
        browser.console_clean("a confirmed revision").await?;

        // The approval of the newest revision by a second operator.
        hydrated(browser, &stack).await?;
        submitted(browser, "approve-revision-confirm", "Approve revision 3").await?;
        browser
            .text(
                "strong",
                "Replica gateway-0 recorded the approval of revision 3.",
            )
            .await?;
        let source = browser.driver().source().await?;
        assert!(!source.contains(VALUE), "the page shows the secret");
        browser.console_clean("an approved revision").await
    })
    .await
}
