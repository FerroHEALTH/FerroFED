// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials and binding sections of the configuration page in the
//! browser, against a stub admin listener: a credential saved with a secret
//! value the page never shows again, chosen with its Set a value choice;
//! the confirmation the running revision awaits, with its countdown; the
//! roll back of a credential change through its confirmation step; and the
//! approval the newest revision takes, each without a console error.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::mock::Server;
use thirtyfour::By;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::configure::{bodies, confirmed, fill, focused_on_outcome, hydrated, submit};
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
    "awaiting":{"deadline":"2099-10-15T09:02:00Z","resources":["[credentials.\"node-a-pub\"]"]}},
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

/// The rollback of revision 2: revision 5 restores revision 1 and runs.
const ROLLED_BACK: &str = r#"{"revision":{"id":5,"base":4,"origin":"rollback","restores":1,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:01:30Z","reason":"the token is wrong","toml":""},
    "current":{"revision":5,"previous":2,"outcome":"applied","needs_restart":[],
    "applied_by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "applied_at":"2026-10-15T09:01:30Z"}}"#;

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
        ("POST", "/admin/config/rollback", 200, ROLLED_BACK),
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

/// The choice `label` of the secret `key`.
fn choice(key: &str, label: &str) -> By {
    By::XPath(format!(
        "//fieldset[contains(@class, 'secret') and .//code[. = '{key}']]//label[contains(., '{label}')]"
    ))
}

/// The value field of the secret `key`.
fn value_field(key: &str) -> By {
    By::XPath(format!(
        "//fieldset[contains(@class, 'secret') and .//code[. = '{key}']]//input[@type = 'password']"
    ))
}

/// Saves the credentials of node A with a bearer token typed into its Set
/// a value field: the value goes to the replica in the save and is never
/// shown again, in the answer or the page drawn afterwards.
async fn secret_saved(
    browser: &Browser,
    stack: &Stack,
    admin: &Server,
) -> Result<(), Box<dyn Error>> {
    hydrated(
        browser,
        stack,
        (
            "/configuration/credentials/node-a-pub",
            submit("Save as revision 4"),
        ),
    )
    .await?;
    let password = browser.element(value_field("bearer_token")).await?;
    assert!(
        !password.is_displayed().await?,
        "the value field shows once Set a value is chosen"
    );
    assert_eq!(
        Some(String::new()),
        password.prop("value").await?,
        "a password field is never filled"
    );
    browser
        .element(choice("bearer_token", "Set a value"))
        .await?
        .click()
        .await?;
    assert!(
        password.is_displayed().await?,
        "Set a value shows its field"
    );
    password.send_keys(VALUE).await?;
    fill(browser, "cfg-reason", "rotate").await?;
    browser
        .element(submit("Save as revision 4"))
        .await?
        .click()
        .await?;
    browser
        .text("strong", "Replica gateway-0 saved revision 4.")
        .await?;
    focused_on_outcome(browser).await?;
    let source = browser.driver().source().await?;
    assert!(!source.contains(VALUE), "the page shows the secret");
    let saved = bodies(admin, "/admin/config/credentials/node-a-pub").await?;
    let [saved] = saved.as_slice() else {
        return Err(format!("one save, not {}", saved.len()).into());
    };
    assert!(
        saved.contains(VALUE),
        "the value goes to the replica: {saved}"
    );
    browser.console_clean("a saved credential").await
}

/// Rolls the credential change back, its reason asked in its confirmation
/// step.
async fn rolled_back(
    browser: &Browser,
    stack: &Stack,
    admin: &Server,
) -> Result<(), Box<dyn Error>> {
    hydrated(
        browser,
        stack,
        (
            "/configuration/credentials",
            By::Css(r#"button[popovertarget="rollback-confirm"]"#),
        ),
    )
    .await?;
    browser
        .element(By::Css(
            r#"button[popovertarget="rollback-confirm"][aria-haspopup]"#,
        ))
        .await?
        .click()
        .await?;
    browser
        .element(By::Css("#rollback-confirm:popover-open"))
        .await?;
    fill(browser, "cfg-rollback-reason", "the token is wrong").await?;
    browser
        .element(By::XPath(
            "//div[@id = 'rollback-confirm']//button[@type = 'submit']",
        ))
        .await?
        .click()
        .await?;
    browser
        .text(
            "strong",
            "Replica gateway-0 applied revision 5, restoring the content of revision 1.",
        )
        .await?;
    focused_on_outcome(browser).await?;
    assert_eq!(
        vec![String::from(
            r#"{"current":2,"reason":"the token is wrong"}"#
        )],
        bodies(admin, "/admin/config/rollback").await?,
        "the rollback names the running revision and the reason"
    );
    browser.console_clean("a rolled-back revision").await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_credential_is_saved_with_a_secret_and_the_running_change_confirmed_rolled_back_and_approved()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        hydrated(
            browser,
            &stack,
            (
                "/configuration/credentials",
                By::Css(r#"button[popovertarget="confirm-confirm"]"#),
            ),
        )
        .await?;
        browser
            .text("span", "/var/lib/ferrofed/secrets/a.secret")
            .await?;
        browser
            .text("h2", "Revision 2 awaits your confirmation")
            .await?;
        browser.text("span", "left").await?;
        browser.text("h2", "Revision 3 changes 1 setting").await?;
        browser
            .console_clean("the hydration of the credentials and bindings")
            .await?;

        secret_saved(browser, &stack, &admin).await?;

        // The confirmation of the running revision before its deadline.
        hydrated(
            browser,
            &stack,
            (
                "/configuration/credentials",
                By::Css(r#"button[popovertarget="confirm-confirm"]"#),
            ),
        )
        .await?;
        confirmed(browser, "confirm-confirm").await?;
        browser
            .text(
                "strong",
                "Replica gateway-0 recorded the confirmation of revision 2.",
            )
            .await?;
        browser.console_clean("a confirmed revision").await?;

        rolled_back(browser, &stack, &admin).await?;

        // The approval of the newest revision by a second operator.
        hydrated(
            browser,
            &stack,
            (
                "/configuration/credentials",
                By::Css(r#"button[popovertarget="approve-confirm"]"#),
            ),
        )
        .await?;
        confirmed(browser, "approve-confirm").await?;
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
