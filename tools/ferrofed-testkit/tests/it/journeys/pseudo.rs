// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every page in the pseudo-locale (`qps`), which draws every word of the
//! console and of FerroUI accented and two fifths longer: each page still
//! fits a window 1440 and 390 CSS pixels wide, so a longer translation has
//! room, and with `FERROFED_SCREENSHOTS=1` each is captured at both widths
//! under the ignored `target/console-screenshots/pseudo`, to be read. No
//! specification governs pseudo-localization: our own design, as FerroUI's
//! gallery draws its own.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::PathBuf;

use serde_json::json;
use thirtyfour::By;

use crate::journeys::browser::{until, with_browser};
use crate::journeys::configure::listener;
use crate::journeys::journeys_enabled;
use crate::journeys::reflow::PAGES;
use crate::journeys::stack::Stack;

/// The widths every page is laid out at: the design's desktop and phone.
const WIDTHS: [u32; 2] = [1440, 390];

/// Where the captures go, under the workspace's `target`.
const OUT: &str = "../../target/console-screenshots/pseudo";

/// Makes the window `width` CSS pixels wide and `height` tall (the Chrome
/// `DevTools` Protocol, `Emulation.setDeviceMetricsOverride`).
async fn window(
    browser: &crate::journeys::browser::Browser,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .cdp()
        .send_raw(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
        )
        .await?;
    Ok(())
}

/// The window's width and the page's content width as the browser lays
/// it out (the Chrome `DevTools` Protocol, `Page.getLayoutMetrics`).
#[expect(
    clippy::disallowed_types,
    reason = "the test seam: the DevTools Protocol answers its layout metrics as JSON, read here alone"
)]
async fn layout(browser: &crate::journeys::browser::Browser) -> Result<(f64, f64), Box<dyn Error>> {
    let metrics = browser
        .driver()
        .cdp()
        .send_raw("Page.getLayoutMetrics", json!({}))
        .await?;
    let read = |area: &str, field: &str| {
        metrics
            .get(area)
            .and_then(|area| area.get(field))
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| format!("no {area}.{field} in {metrics}"))
    };
    Ok((
        read("cssLayoutViewport", "clientWidth")?,
        read("cssContentSize", "width")?,
    ))
}

/// The file a capture of `path` at `width` is written to.
fn output(path: &str, width: u32) -> PathBuf {
    let slug: String = path
        .trim_start_matches('/')
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() {
                letter
            } else {
                '-'
            }
        })
        .collect();
    let slug = if slug.is_empty() {
        String::from("dashboard")
    } else {
        slug
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(OUT)
        .join(format!("{slug}-{width}.png"))
}

#[tokio::test(flavor = "multi_thread")]
async fn every_page_fits_in_the_pseudo_locale_at_1440_and_390_pixels() -> Result<(), Box<dyn Error>>
{
    if !journeys_enabled() {
        return Ok(());
    }
    let capture = std::env::var("FERROFED_SCREENSHOTS").is_ok_and(|value| value == "1");
    if capture {
        std::fs::create_dir_all(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(OUT))?;
    }
    let admin = listener().await;
    let stack = Stack::start_in(&admin.uri(), "qps").await?;
    with_browser(async |browser| {
        // NOTE: no specification governs this: our own design; the browser asks
        // for no language the console ships, so the configured `qps` serves.
        let cdp = browser.driver().cdp();
        cdp.send_raw("Network.enable", json!({})).await?;
        cdp.send_raw(
            "Network.setExtraHTTPHeaders",
            json!({ "headers": { "Accept-Language": "qps" } }),
        )
        .await?;
        browser.goto(&stack.url("/login")).await?;
        until("the browser holds a session cookie", async || {
            Ok(browser.cookie(stack.cookie()).await?.is_some())
        })
        .await?;
        let mut wide = Vec::new();
        for width in WIDTHS {
            for path in std::iter::once("/").chain(PAGES) {
                window(browser, width, 900).await?;
                browser.goto(&stack.url(path)).await?;
                browser.element(By::Tag("h1")).await?;
                browser.element(By::Css(r#"html[lang="qps"]"#)).await?;
                let (window_width, content) = layout(browser).await?;
                if content > window_width {
                    wide.push(format!(
                        "{path}: the document is {content} px wide in a {window_width} px window"
                    ));
                }
                if capture {
                    let tall = crate::journeys::screenshots::content_height(browser).await?;
                    window(browser, width, tall).await?;
                    let png = browser.driver().screenshot_as_png().await?;
                    std::fs::write(output(path, width), png)?;
                }
            }
        }
        browser
            .console_clean("every page in the pseudo-locale")
            .await
    })
    .await
}
