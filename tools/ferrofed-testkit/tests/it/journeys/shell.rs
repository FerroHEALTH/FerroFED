// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shell in the browser: the theme switch, which draws the page at once
//! and keeps the choice for the next page the server draws, before any
//! script runs; the navigation drawer below `md`, which the keyboard opens,
//! moves the focus into, closes with Escape and returns the focus from; and
//! the operator's menu.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use serde_json::json;
use thirtyfour::{By, Key};

use crate::journeys::browser::{Browser, until, with_browser};
use crate::journeys::journeys_enabled;
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
async fn the_theme_switch_draws_the_page_and_the_server_keeps_the_choice()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        drawn_in(browser, "system").await?;
        let dark = "//header//div[@aria-label = 'Theme']//button[normalize-space(.) = 'Dark']";
        browser.element(By::XPath(dark)).await?.click().await?;
        drawn_in(browser, "dark").await?;
        until("the switch marks the dark theme pressed", async || {
            let pressed = browser
                .element(By::XPath(dark))
                .await?
                .attr("aria-pressed")
                .await?;
            Ok(pressed.as_deref() == Some("true"))
        })
        .await?;
        browser.console_clean("the theme switch").await?;

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
        let system = "//header//div[@aria-label = 'Theme']//button[normalize-space(.) = 'System']";
        browser.element(By::XPath(system)).await?.click().await?;
        drawn_in(browser, "system").await?;
        browser
            .console_clean("the way back to the system theme")
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
            Ok(focused(browser).await? == "Menu")
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
        browser
            .element(By::XPath(
                "//div[@id = 'operator-menu']//button[normalize-space(.) = 'Sign out']",
            ))
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
        browser.console_clean("the operator's menu").await
    })
    .await
}
