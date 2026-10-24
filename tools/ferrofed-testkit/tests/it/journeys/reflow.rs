// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reflow: every page, signed out and signed in, fits a window 320 CSS
//! pixels wide without scrolling the document sideways (WCAG 2.2 SC 1.4.10,
//! <https://www.w3.org/TR/WCAG22/#reflow>). A table wider than that scrolls
//! in its own box, which SC 1.4.10 allows for content that needs two
//! dimensions; nothing else scrolls, so any other overflow widens the
//! document this journey measures. At a phone's 390 pixels, a table row's
//! removal dialog wraps its sentence inside the dialog.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use serde_json::json;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::configure::listener;
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// The width SC 1.4.10 names, in CSS pixels.
const WIDTH: u32 = 320;

/// The pages a signed-in operator reads.
const PAGES: [&str; 33] = [
    "/about",
    "/",
    "/topology",
    "/organisations",
    "/organisations/nodes",
    "/organisations/org-a",
    "/organisations/org-a/node-a",
    "/organisations/org-a/node-a/endpoints",
    "/organisations/org-a/node-a/routes",
    "/services",
    "/integrity",
    "/stored-queries",
    "/stored-queries/org.example::journey_001",
    "/stored-queries/org.example::journey_001/versions",
    "/self-description",
    "/self-description/limits",
    "/query",
    "/query/stored",
    "/preferences",
    "/search?q=node",
    "/configuration",
    "/configuration/federation/edit",
    "/configuration/access-log",
    "/configuration/stored-queries",
    "/configuration/members",
    "/configuration/members/new",
    "/configuration/members/node-b",
    "/configuration/credentials",
    "/configuration/credentials/new",
    "/configuration/bindings/new",
    "/configuration/stored-queries/new",
    "/configuration/history",
    "/configuration/import-export",
];

/// Makes the window `WIDTH` CSS pixels wide (the Chrome `DevTools` Protocol,
/// `Emulation.setDeviceMetricsOverride`).
///
/// The window is a desktop one: a phone's emulation zooms a page wider than
/// the screen out until it fits, which would hide the overflow under test.
async fn narrow(browser: &Browser) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .cdp()
        .send_raw(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": WIDTH, "height": 640, "deviceScaleFactor": 1, "mobile": false }),
        )
        .await?;
    Ok(())
}

/// Fails when the page at `path` is wider than the window: the document's
/// content as the browser lays it out (the Chrome `DevTools` Protocol,
/// `Page.getLayoutMetrics`) against the width of the window.
#[expect(
    clippy::disallowed_types,
    reason = "the test seam: the DevTools Protocol answers its layout metrics as JSON, read here alone"
)]
async fn fits(browser: &Browser, path: &str) -> Result<(), Box<dyn Error>> {
    let metrics = browser
        .driver()
        .cdp()
        .send_raw("Page.getLayoutMetrics", json!({}))
        .await?;
    let width = |area: &str, field: &str| {
        metrics
            .get(area)
            .and_then(|area| area.get(field))
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| format!("no {area}.{field} in {metrics}"))
    };
    let window = width("cssLayoutViewport", "clientWidth")?;
    let content = width("cssContentSize", "width")?;
    assert!(
        content <= window,
        "{path}: the document is {content} px wide in a {window} px window"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn every_page_fits_a_window_320_pixels_wide() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        narrow(browser).await?;
        browser.goto(&stack.url("/")).await?;
        browser
            .element(By::LinkText("Sign in with your organisation's account"))
            .await?;
        fits(browser, "/ (signed out)").await?;

        sign_in(browser, &stack).await?;
        for path in PAGES {
            browser.goto(&stack.url(path)).await?;
            browser.element(By::Tag("h1")).await?;
            fits(browser, path).await?;
        }
        browser.console_clean("the reflow of every page").await?;

        browser.goto(&stack.url("/no/such/page")).await?;
        browser.text("h1", "Not found").await?;
        fits(browser, "/no/such/page").await?;
        // The page answers 404, which the browser logs as an error: that
        // answer is the page under test, so it is the one entry allowed.
        let errors = browser.console_errors().await?;
        let [error] = errors.as_slice() else {
            panic!("the page for an unknown path logged {errors:?}, not its one 404");
        };
        assert!(
            error.contains("/no/such/page") && error.contains("404"),
            "the page for an unknown path logged {error}, not its 404"
        );
        Ok(())
    })
    .await
}

/// The width of a phone the FerroFED mockups draw, in CSS pixels.
const PHONE: u32 = 390;

/// What the open removal dialog of a row and that row's menu trigger do
/// wrong at the window's width, in words, empty when nothing: the dialog's
/// sentence does not wrap or runs past the dialog's edge, the dialog
/// runs past the window, or the trigger sits off its cell's first line.
const ROW_REMOVAL_LAYOUT: &str = r"
const [trigger, dialog] = arguments;
const wrong = [];
const sentence = dialog.querySelector('.fui-dialog-title + p');
if (!sentence) { return 'the dialog draws no sentence'; }
const box = dialog.getBoundingClientRect();
const said = sentence.getBoundingClientRect();
if (getComputedStyle(sentence).whiteSpace === 'nowrap') { wrong.push('the sentence does not wrap'); }
if (sentence.scrollWidth > sentence.clientWidth || said.right > box.right + 0.5) {
  wrong.push(`the sentence runs to ${said.right} past the dialog's edge at ${box.right}`);
}
if (box.right > document.documentElement.clientWidth + 0.5) {
  wrong.push(`the dialog runs to ${box.right} past the window`);
}
const cell = trigger.closest('td');
const style = getComputedStyle(cell);
const top = cell.getBoundingClientRect().top + parseFloat(style.paddingTop);
const menu = trigger.getBoundingClientRect();
const line = Math.max(parseFloat(style.lineHeight) || 0, menu.height);
const centre = menu.top + menu.height / 2;
if (centre < top || centre > top + line) {
  wrong.push(`the menu's centre at ${centre} is off its cell's first line, ${top} to ${top + line}`);
}
return wrong.join('; ');
";

// FerroUI 0.3.6 draws a dialog in a table's actions cell with its sentence
// wrapping (FerroHEALTH/FerroUI#388) and a row menu on the row's first line
// (FerroHEALTH/FerroUI#391); a console member's removal sits in its cell.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_removal_wraps_its_sentence_on_a_phone() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        browser
            .driver()
            .cdp()
            .send_raw(
                "Emulation.setDeviceMetricsOverride",
                json!({ "width": PHONE, "height": 844, "deviceScaleFactor": 1, "mobile": false }),
            )
            .await?;
        browser.goto(&stack.url("/")).await?;
        sign_in(browser, &stack).await?;
        let dialog = "cfg-remove-node-b-dialog";
        browser.goto(&stack.url("/configuration/members")).await?;
        browser
            .element(By::Css(format!("td dialog#{dialog}:not([open])")))
            .await?;
        let trigger = browser
            .element(By::Css(r#"summary[aria-label="Actions for node-b"]"#))
            .await?;
        trigger.click().await?;
        browser
            .element(By::Css(format!(
                r#"button[role="menuitem"][commandfor="{dialog}"]"#
            )))
            .await?
            .click()
            .await?;
        let opened = browser
            .element(By::Css(format!("dialog#{dialog}[open]")))
            .await?;
        let wrong: String = browser
            .driver()
            .execute(
                ROW_REMOVAL_LAYOUT,
                vec![trigger.to_json()?, opened.to_json()?],
            )
            .await?
            .convert()?;
        assert!(wrong.is_empty(), "node-b's removal at {PHONE} px: {wrong}");
        browser.console_clean("a row's removal on a phone").await
    })
    .await
}
