// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin actions in the browser, against a stub admin listener: the
//! clear of a held collision beside the integrity incidents and the drift
//! repair beside the stored queries, each form disabled until the page has
//! hydrated and each answered with what the replica did, without a console
//! error.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::mock::Server;
use ferrofed_console::app::PRODUCT;
use thirtyfour::By;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// A synthetic `ehr_id` two members claim.
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The heading of the collision clear's section.
const CLEAR: &str = "Clear a held ehr_id";

/// The heading of the drift repair's section.
const REPAIR: &str = "Repair stored-query drift";

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

/// Opens the confirmation step `id` from its button and runs the action
/// with its submit, under the heading `heading`.
async fn confirmed(browser: &Browser, id: &str, heading: &str) -> Result<(), Box<dyn Error>> {
    let opens = browser
        .element(By::Css(format!(
            r#"button[popovertarget="{id}"][aria-haspopup]"#
        )))
        .await?;
    browser.press(&opens).await?;
    browser
        .element(By::Css(format!("#{id}:popover-open")))
        .await?;
    browser
        .press(&browser.element(submit(heading)).await?)
        .await?;
    Ok(())
}

/// Loads the view at `route`, titled `title`, and waits until the form
/// under `heading` runs.
async fn hydrated(
    browser: &Browser,
    stack: &Stack,
    (route, title): (&str, &str),
    heading: &str,
) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url(route)).await?;
    browser.titled(&format!("{title} · {PRODUCT}")).await?;
    browser
        .enabled(submit(heading))
        .await
        .map_err(|error| format!("the {title} page never hydrated: {error}").into())
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
        // what the replica held and forgot.
        hydrated(browser, &stack, ("/integrity", "Integrity"), CLEAR).await?;
        browser
            .console_clean("the hydration of the integrity page")
            .await?;
        fill(browser, "clear-ehr-id", EHR_ID).await?;
        confirmed(browser, "clear-confirm", CLEAR).await?;
        browser
            .text("strong", &format!("Replica gateway-0 cleared {EHR_ID}."))
            .await?;
        browser
            .text("div", "It held a collision between node-a, node-b.")
            .await?;
        browser.console_clean("a cleared collision").await?;

        // The drift repair of a version the gateway holds, answered with
        // each member's outcome and the answer's completeness.
        hydrated(
            browser,
            &stack,
            ("/stored-queries", "Stored queries"),
            REPAIR,
        )
        .await?;
        browser
            .console_clean("the hydration of the stored-query page")
            .await?;
        fill(browser, "repair-name", NAME).await?;
        fill(browser, "repair-version", "1.0.0").await?;
        confirmed(browser, "repair-confirm", REPAIR).await?;
        browser
            .text(
                "p",
                &format!("Replica gateway-0 sent {NAME} version 1.0.0 again: 200."),
            )
            .await?;
        browser.text("strong", "Complete.").await?;
        browser.console_clean("a repaired drift").await
    })
    .await?;
    admin.verify().await;
    Ok(())
}
