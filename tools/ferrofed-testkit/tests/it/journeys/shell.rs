// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shell in the browser: the theme toggle, which draws the page at once
//! and keeps the choice for the next page the server draws, before any
//! script runs; the navigation drawer below `md`, which the keyboard opens,
//! moves the focus into, closes with Escape and returns the focus from; the
//! operator's menu; and the sticky top bar, which never hides the control
//! that holds the focus (WCAG 2.2 SC 2.4.11).
use std::error::Error;

use serde_json::json;
use thirtyfour::{By, Key};

use crate::journeys::browser::{Browser, until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::provider::OPERATOR;
use crate::journeys::query::hydrated;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// Makes the window `width` CSS pixels wide, a desktop window as the reflow
/// journey's is (the Chrome `DevTools` Protocol,
/// `Emulation.setDeviceMetricsOverride`).
async fn wide(browser: &Browser, width: u32) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .cdp()
        .send_raw(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width, "height": 800, "deviceScaleFactor": 1, "mobile": false }),
        )
        .await?;
    Ok(())
}

/// Waits until `<html>` carries the theme `theme`.
async fn drawn_in(browser: &Browser, theme: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::Css(format!(r#"html[data-theme="{theme}"]"#)))
        .await
        .map_err(|error| format!("the page is never drawn in {theme}: {error}"))?;
    Ok(())
}

/// The text of the element that holds the keyboard focus.
async fn focused(browser: &Browser) -> Result<String, Box<dyn Error>> {
    Ok(browser.driver().active_element().await?.text().await?)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_theme_toggle_draws_the_page_and_the_server_keeps_the_choice()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser
            .driver()
            .cdp()
            .send_raw(
                "Emulation.setEmulatedMedia",
                json!({ "features": [{ "name": "prefers-color-scheme", "value": "light" }] }),
            )
            .await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        drawn_in(browser, "system").await?;
        until("the toggle swaps the theme in place", async || {
            let to_dark = browser.element(By::Id("theme-to-dark")).await?;
            if to_dark.is_displayed().await? {
                to_dark.click().await?;
            }
            Ok(!browser
                .driver()
                .find_all(By::Css(r#"html[data-theme="dark"]"#))
                .await?
                .is_empty())
        })
        .await?;
        browser.at(&stack.url("/query")).await?;
        browser.element(By::Id("theme-to-light")).await?;
        browser.console_clean("the theme toggle").await?;

        // The server draws the next page in the theme it keeps, before any
        // script runs, so the page never shows the wrong theme first.
        until("the console keeps the theme", async || {
            browser.scripts(false).await?;
            browser.goto(&stack.url("/members")).await?;
            let kept = browser
                .driver()
                .find_all(By::Css(r#"html[data-theme="dark"]"#))
                .await?;
            browser.scripts(true).await?;
            Ok(!kept.is_empty())
        })
        .await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        until("the toggle swaps back to the light theme", async || {
            let to_light = browser.element(By::Id("theme-to-light")).await?;
            if to_light.is_displayed().await? {
                to_light.click().await?;
            }
            Ok(!browser
                .driver()
                .find_all(By::Css(r#"html[data-theme="light"]"#))
                .await?
                .is_empty())
        })
        .await?;
        browser
            .console_clean("the way back to the light theme")
            .await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_keyboard_opens_the_drawer_moves_into_it_and_escape_returns()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        wide(browser, 390).await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        let menu = r#"button[popovertarget="sidebar"]"#;
        browser
            .element(By::Css(menu))
            .await?
            .send_keys(Key::Enter)
            .await?;
        browser.element(By::Css("#sidebar:popover-open")).await?;
        until("the focus moves to the drawer's first link", async || {
            Ok(focused(browser).await? == "Dashboard")
        })
        .await?;

        browser
            .driver()
            .active_element()
            .await?
            .send_keys(Key::Escape)
            .await?;
        until("Escape closes the drawer", async || {
            Ok(browser
                .driver()
                .find_all(By::Css("#sidebar:popover-open"))
                .await?
                .is_empty())
        })
        .await?;
        until("the focus returns to the menu button", async || {
            let active = browser.driver().active_element().await?;
            Ok(active.attr("aria-label").await?.as_deref() == Some("Menu"))
        })
        .await?;

        // A link the drawer leads to closes it as the page changes.
        browser.element(By::Css(menu)).await?.click().await?;
        browser.element(By::Css("#sidebar:popover-open")).await?;
        browser
            .element(By::XPath(
                "//nav[@id = 'sidebar']//a[normalize-space(.) = 'Members']",
            ))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/members")).await?;
        browser.text("caption", "Member endpoints").await?;
        until("following a link closes the drawer", async || {
            Ok(browser
                .driver()
                .find_all(By::Css("#sidebar:popover-open"))
                .await?
                .is_empty())
        })
        .await?;
        browser.console_clean("the navigation drawer").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_operators_menu_opens_from_the_keyboard_and_escape_closes_it()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        let button = r#"button[popovertarget="operator-menu"]"#;
        browser
            .element(By::Css(button))
            .await?
            .send_keys(Key::Enter)
            .await?;
        browser
            .element(By::Css("#operator-menu:popover-open"))
            .await?;
        // The next Tab goes into the open menu, to its sign-out.
        browser
            .driver()
            .active_element()
            .await?
            .send_keys(Key::Tab)
            .await?;
        until("Tab reaches the sign-out", async || {
            Ok(focused(browser).await?.trim() == "Sign out")
        })
        .await?;
        browser
            .driver()
            .active_element()
            .await?
            .send_keys(Key::Escape)
            .await?;
        until("Escape closes the operator's menu", async || {
            Ok(browser
                .driver()
                .find_all(By::Css("#operator-menu:popover-open"))
                .await?
                .is_empty())
        })
        .await?;
        until("the focus returns to the operator's button", async || {
            Ok(focused(browser).await?.contains(OPERATOR))
        })
        .await?;
        browser.console_clean("the operator's menu").await
    })
    .await
}

/// How far below the top of the window the element `element` starts, in
/// CSS pixels: its place in the document (`WebDriver`, Get Element Rect) less
/// how far the page is scrolled (the Chrome `DevTools` Protocol,
/// `Page.getLayoutMetrics`).
#[expect(
    clippy::disallowed_types,
    reason = "the test seam: the DevTools Protocol answers its layout metrics as JSON, read here alone"
)]
async fn from_top(
    browser: &Browser,
    element: &thirtyfour::WebElement,
) -> Result<f64, Box<dyn Error>> {
    let metrics = browser
        .driver()
        .cdp()
        .send_raw("Page.getLayoutMetrics", json!({}))
        .await?;
    let scrolled = metrics
        .get("cssVisualViewport")
        .and_then(|viewport| viewport.get("pageY"))
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| format!("no cssVisualViewport.pageY in {metrics}"))?;
    Ok(element.rect().await?.y - scrolled)
}

#[tokio::test(flavor = "multi_thread")]
async fn shift_tab_never_leaves_the_focus_under_the_top_bar() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser
            .driver()
            .cdp()
            .send_raw(
                "Emulation.setDeviceMetricsOverride",
                json!({ "width": 1280, "height": 500, "deviceScaleFactor": 1, "mobile": false }),
            )
            .await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        let bar = browser.element(By::Tag("header")).await?.rect().await?.height;
        // The last field of the form, far down the page: focusing it scrolls
        // the page, so the fields before it pass under the bar's place.
        let fields = browser
            .driver()
            .find_all(By::Css(r#"main form input[type="text"]"#))
            .await?;
        let last = fields.last().ok_or("the query form has a text field")?;
        last.click().await?;
        for step in 1..=6 {
            browser
                .driver()
                .active_element()
                .await?
                .send_keys(Key::Shift + Key::Tab)
                .await?;
            let focused = browser.driver().active_element().await?;
            let top = from_top(browser, &focused).await?;
            if top < bar - 0.5 {
                return Err(format!(
                    "Shift+Tab {step} leaves the focus {top} px from the top, under the {bar} px bar"
                )
                .into());
            }
        }
        browser.console_clean("the focus under the top bar").await
    })
    .await
}

// The stack's gateway runs the development profile with its static
// cross-reference, so every signed-in page names it in the banner under the
// top bar, before any script runs and after a navigation of the client
// router, and the pill names the gateway's version.
#[tokio::test(flavor = "multi_thread")]
async fn a_development_gateway_is_named_in_the_banner_on_every_page() -> Result<(), Box<dyn Error>>
{
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        let banner = "//div[@role = 'note' and contains(., 'Development deployment')]";
        browser.element(By::XPath(banner)).await?;
        browser
            .text("div", "the static development cross-reference")
            .await?;
        browser
            .text("span", &format!("· v{}", ferrofed_console::app::VERSION))
            .await?;
        browser.console_clean("the banner on the dashboard").await?;

        browser.follow("Members").await?;
        browser.element(By::XPath(banner)).await?;
        browser
            .console_clean("the banner after a navigation")
            .await?;

        browser.scripts(false).await?;
        browser.goto(&stack.url("/routing")).await?;
        browser.element(By::XPath(banner)).await?;
        browser.scripts(true).await?;
        browser.goto(&stack.url("/members")).await?;
        browser.element(By::XPath(banner)).await?;
        browser.console_clean("the banner before any script").await
    })
    .await
}
