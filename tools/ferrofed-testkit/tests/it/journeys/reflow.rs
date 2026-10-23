// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reflow: every page, signed out and signed in, fits a window 320 CSS
//! pixels wide without scrolling the document sideways (WCAG 2.2 SC 1.4.10,
//! <https://www.w3.org/TR/WCAG22/#reflow>). A table wider than that scrolls
//! in its own box, which SC 1.4.10 allows for content that needs two
//! dimensions; nothing else scrolls, so any other overflow widens the
//! document this journey measures.
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
const PAGES: [&str; 29] = [
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
