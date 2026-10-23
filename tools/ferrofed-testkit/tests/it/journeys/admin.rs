// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin actions in the browser, against a stub admin listener: the
//! clear of a held collision on Integrity and the drift repair on a stored
//! query's page, each a form in a confirmation dialog that posts to its
//! page and is answered there with what the replica did, without a console
//! error.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_console::app::PRODUCT;
use ferrofed_testkit::mock::Server;
use thirtyfour::{By, Key};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::journeys::browser::{Browser, hydrated, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// A synthetic `ehr_id` two members claim.
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The stored query the drift repair sends again, one the stack's gateway
/// holds.
const NAME: &str = "org.example::journey_001";

/// What the replica answers the clear of [`EHR_ID`]: it held a collision.
fn cleared() -> String {
    format!(
        r#"{{"ehr_id":"{EHR_ID}","held":"collision","claimants":["node-a","node-b"],"indexed":true,"sessions":1,"scope":"replica"}}"#
    )
}

/// What the replica answers the drift repair: every member took the
/// version again (§12.7, §11.4).
fn distributed() -> String {
    format!(
        r#"{{"name":"{NAME}","type":"AQL","version":"1.0.0","saved":"2026-10-15T08:00:00Z",
        "q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
        "meta":{{"federation":{{"complete":true,"endpoints":[
          {{"id":"node-a-pub","status":"active","latency_ms":12}},
          {{"id":"node-b-pub","status":"active","latency_ms":14}}]}},
        "registry":"held"}}}}"#
    )
}

/// A stub admin listener answering the clear and the drift repair, each
/// once, the repair only when it names every member.
async fn listener() -> Server {
    let listener = Server::start().await;
    let reply = |body: String| {
        ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json")
    };
    Mock::given(method("POST"))
        .and(path(format!("/admin/ehr-ids/{EHR_ID}/clear").as_str()))
        .respond_with(reply(cleared()))
        .expect(1)
        .mount(&listener)
        .await;
    Mock::given(method("POST"))
        .and(path(
            format!("/admin/stored-queries/{NAME}/1.0.0/distribute").as_str(),
        ))
        .and(header("openEHR-federation-endpoint", "*"))
        .respond_with(reply(distributed()))
        .expect(1)
        .mount(&listener)
        .await;
    listener
}

/// Replaces the text of the field `id` with `text`.
async fn fill(browser: &Browser, id: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let field = browser.element(By::Id(id)).await?;
    browser
        .driver()
        .execute("arguments[0].select();", vec![field.to_json()?])
        .await?;
    field.send_keys(Key::Backspace).await?;
    field.send_keys(text).await?;
    Ok(())
}

/// Opens the confirmation dialog `id` from its button.
async fn open(browser: &Browser, id: &str) -> Result<(), Box<dyn Error>> {
    let opens = browser
        .element(By::Css(format!(
            r#"button[commandfor="{id}"][aria-haspopup="dialog"]"#
        )))
        .await?;
    browser.press(&opens).await?;
    browser
        .element(By::Css(format!("dialog#{id}[open]")))
        .await?;
    Ok(())
}

/// Runs the action of the open dialog `id` with its submit.
async fn run(browser: &Browser, id: &str) -> Result<(), Box<dyn Error>> {
    let submit = browser
        .element(By::Css(format!(
            r#"dialog#{id}[open] button[type="submit"]"#
        )))
        .await?;
    submit.click().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_collision_is_cleared_and_drift_repaired() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;

        // The clear of a held collision at every replica, answered with
        // what the replica held.
        browser.goto(&stack.url("/integrity")).await?;
        browser.titled(&format!("Integrity · {PRODUCT}")).await?;
        hydrated(browser).await?;
        browser.console_clean("the hydration of the integrity page").await?;
        open(browser, "clear-dialog").await?;
        fill(browser, "clear-ehr-id", EHR_ID).await?;
        run(browser, "clear-dialog").await?;
        browser.at(&stack.url("/integrity")).await?;
        browser.text("li", "gateway-0 cleared it: it held it at node-a, node-b.").await?;
        browser.console_clean("a cleared collision").await?;

        // The drift repair of a version the gateway holds, answered with
        // each member's outcome and the answer's completeness.
        browser.goto(&stack.url(&format!("/stored-queries/{NAME}?version=1.0.0"))).await?;
        browser.titled(&format!("{NAME} · {PRODUCT}")).await?;
        hydrated(browser).await?;
        open(browser, "repair-drift").await?;
        fill(browser, "repair-version", "1.0.0").await?;
        run(browser, "repair-drift").await?;
        browser
            .element(By::XPath(
                "//main//*[contains(text(), 'Version 1.0.0 was sent again by gateway-0, and every endpoint took it.')]",
            ))
            .await?;
        browser.text("span", "Complete").await?;
        browser
            .text("p", &format!("Version 1.0.0 of {NAME}"))
            .await?;
        browser.console_clean("a repaired drift").await
    })
    .await?;
    admin.verify().await;
    Ok(())
}
