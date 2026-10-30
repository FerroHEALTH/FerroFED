// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings tabs of the configuration page in the browser, against a
//! stub admin listener: each tab a link with the running settings, its
//! Edit a page whose form posts the changed setting alone and is answered
//! with the revision saved, with no console error, and the same edit with
//! no script.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use thirtyfour::By;

use ferrofed_console::app::PRODUCT;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::configure::{bodies, listener, scripted, submit};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// Replaces the text of the field `id` with `text`, by the `WebDriver` clear,
/// which needs no page script.
async fn typed(browser: &Browser, id: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let field = browser.element(By::Id(id)).await?;
    field.clear().await?;
    field.send_keys(text).await?;
    Ok(())
}

/// Changes the per-node timeout on the Federation behaviour tab's edit to
/// `millis`, saves it, and checks the answer and what the stub received.
async fn edit_and_save(
    browser: &Browser,
    admin: &ferrofed_testkit::mock::Server,
    millis: &str,
) -> Result<(), Box<dyn Error>> {
    typed(browser, "cfg-setting-0", millis).await?;
    typed(browser, "cfg-reason", "synthetic longer budget").await?;
    browser.element(submit("Save as")).await?.click().await?;
    browser
        .text("strong", "Replica gateway-0 saved revision 3.")
        .await?;
    let sent = bodies(admin, "/admin/config/settings").await?;
    let expected = format!(r#""settings":{{"federation.per_node_timeout_ms":"{millis}"}}"#);
    assert!(sent.iter().any(|body| body.contains(&expected)), "{sent:?}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settings_tab_shows_the_running_settings_and_its_edit_saves_a_revision()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        scripted(browser, &stack, "/configuration").await?;
        browser
            .element(By::Css(
                r#"a[data-tab="tab-federation"][aria-current="page"]"#,
            ))
            .await?;
        browser.text("span", "Per-node timeout").await?;
        browser
            .element(By::Css(r#"a[data-tab="tab-access-log"]"#))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/configuration/access-log")).await?;
        browser.text("span", "Retention").await?;
        browser
            .element(By::Css(r#"a[data-tab="tab-federation"]"#))
            .await?
            .click()
            .await?;
        browser
            .element(By::Css(r#"a[href="/configuration/federation/edit"]"#))
            .await?
            .click()
            .await?;
        browser
            .at(&stack.url("/configuration/federation/edit"))
            .await?;
        browser.console_clean("the settings tabs").await?;
        scripted(browser, &stack, "/configuration/federation/edit").await?;
        edit_and_save(browser, &admin, "3000").await?;
        browser.console_clean("a saved setting").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_setting_is_saved_with_no_script() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.scripts(false).await?;
        browser
            .goto(&stack.url("/configuration/federation/edit"))
            .await?;
        browser
            .titled(&format!("Configuration · {PRODUCT}"))
            .await?;
        edit_and_save(browser, &admin, "3500").await?;
        browser.scripts(true).await
    })
    .await
}
