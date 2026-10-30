// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pages in the browser: the organisations and their nodes, the
//! services, the integrity incidents, the stored queries, the
//! self-description and the topology, each reached by the navigation and
//! loaded whole from the server, and each drawn with no script too.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_console::app::PRODUCT;
use serde_json::json;
use thirtyfour::By;

use crate::journeys::browser::{Browser, scripted, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{Stack, VERSIONED};

/// The title of the page called `section`.
fn titled(section: &str) -> String {
    format!("{section} · {PRODUCT}")
}

/// Follows the navigation's entry `entry`.
async fn navigate(browser: &Browser, entry: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::XPath(format!(
            "//nav[@aria-label = 'Console']//a[normalize-space(.) = '{entry}']"
        )))
        .await?
        .click()
        .await?;
    Ok(())
}

/// Checks what each page shows once the gateway answered.
async fn each_page(browser: &Browser) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::XPath("//main//a[@href = '/organisations/org-a']"))
        .await?;
    browser
        .element(By::XPath("//main//a[@href = '/organisations/org-b']"))
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn each_page_is_reached_by_the_navigation_and_drawn_whole() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        scripted(browser).await?;

        navigate(browser, "Organisations").await?;
        browser.titled(&titled("Organisations")).await?;
        each_page(browser).await?;
        browser
            .element(By::Css(r#"a[data-tab="nodes"]"#))
            .await?
            .click()
            .await?;
        browser.titled(&titled("All nodes")).await?;
        browser.text("td", "cdr-a.example.org").await?;
        browser.console_clean("the organisations").await?;

        navigate(browser, "Services").await?;
        browser.titled(&titled("Services")).await?;
        browser.text("th", "Identity cross-reference").await?;
        browser.console_clean("the services").await?;

        navigate(browser, "Integrity").await?;
        browser.titled(&titled("Integrity")).await?;
        browser.text("p", "No incident is open.").await?;
        browser.console_clean("the integrity page").await?;

        navigate(browser, "Stored queries").await?;
        browser.titled(&titled("Stored queries")).await?;
        browser.text("caption", "eu.ferrofed.eehrxf").await?;
        browser
            .element(By::XPath(format!(
                "//table[caption = 'org.example']//a[normalize-space(.) = '{VERSIONED}']"
            )))
            .await?;
        browser.console_clean("the stored queries").await?;

        navigate(browser, "Self-description").await?;
        browser.titled(&titled("Self-description")).await?;
        browser.text("code", "example-federation").await?;
        browser
            .element(By::Css(r#"a[data-tab="limits"]"#))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/self-description/limits")).await?;
        browser.console_clean("the self-description").await?;

        navigate(browser, "Topology").await?;
        browser.titled(&titled("Topology")).await?;
        browser.text("span", "org-a").await?;
        scripted(browser).await?;
        browser
            .element(By::Css(r#"button[commandfor="topology-rule"]"#))
            .await?
            .click()
            .await?;
        browser
            .text("dialog[@open]//li", "dashed, marked !")
            .await?;
        browser.console_clean("the topology").await
    })
    .await
}

// Every page reads with no WebAssembly: the server draws it whole, and the
// navigation is plain links.
#[tokio::test(flavor = "multi_thread")]
async fn each_page_reads_with_no_script() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.scripts(false).await?;
        for (path, title, shown) in [
            ("/", "Dashboard", "Nodes reachable"),
            ("/organisations", "Organisations", "org-a"),
            ("/organisations/org-a/node-a", "node-a", "node-a-pub"),
            ("/services", "Services", "Identity cross-reference"),
            ("/integrity", "Integrity", "No incident is open."),
            ("/stored-queries", "Stored queries", VERSIONED),
            (
                "/self-description",
                "Self-description",
                "example-federation",
            ),
            ("/query", "Query console", "Run"),
            ("/preferences", "Your preferences", "Appearance"),
            ("/about", "About", "Cadasto B.V."),
        ] {
            browser.goto(&stack.url(path)).await?;
            browser.titled(&titled(title)).await?;
            browser
                .element(By::XPath(format!("//main//*[contains(., '{shown}')]")))
                .await
                .map_err(|error| format!("{path} shows {shown:?} with no script: {error}"))?;
        }
        navigate(browser, "Organisations").await?;
        browser.titled(&titled("Organisations")).await?;
        browser.scripts(true).await?;
        browser.console_clean("the pages with no script").await
    })
    .await
}

/// The address pattern of FerroUI's script modules, the loader among them.
const MODULES: &str = "*/ferrohealth-ui/*/scripts/*";

/// Refuses the browser every script module of FerroUI from now on (the
/// Chrome `DevTools` Protocol, `Network.setBlockedURLs`).
async fn withhold_the_modules(browser: &Browser) -> Result<(), Box<dyn Error>> {
    let cdp = browser.driver().cdp();
    cdp.send_raw("Network.enable", json!({})).await?;
    cdp.send_raw("Network.setBlockedURLs", json!({ "urls": [MODULES] }))
        .await?;
    Ok(())
}

/// Fails on any browser error other than a refused module, naming `step`.
async fn only_the_modules_refused(browser: &Browser, step: &str) -> Result<(), Box<dyn Error>> {
    let others: Vec<String> = browser
        .console_errors()
        .await?
        .into_iter()
        .filter(|entry| !entry.contains("/scripts/"))
        .collect();
    if others.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the browser logged {} error(s) other than the withheld modules during {step}:\n{}",
        others.len(),
        others.join("\n")
    )
    .into())
}

// Every page the book shows reads whole with FerroUI's modules refused, as
// on a page whose modules have not loaded or will not: the server drew its
// content, its links lead where they lead and its forms post (FerroUI
// `docs/migration-0.6.md`, Tests). A refused module is the one error a page
// may log here.
#[tokio::test(flavor = "multi_thread")]
async fn every_page_reads_whole_with_the_modules_refused() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = crate::journeys::screenshots::listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        withhold_the_modules(browser).await?;
        for shot in &crate::journeys::screenshots::PAGES {
            browser.goto(&stack.url(shot.url)).await?;
            browser
                .element(By::XPath(shot.ready.to_owned()))
                .await
                .map_err(|error| format!("{} reads with the modules refused: {error}", shot.url))?;
            let marked = browser
                .driver()
                .find_all(By::Css(format!("html[{}]", ferrohealth_ui::script::MARK)))
                .await?;
            assert!(marked.is_empty(), "{}: a module ran", shot.url);
            browser.element(By::Css("main h1")).await?;
            only_the_modules_refused(browser, shot.url).await?;
        }
        browser.goto(&stack.url("/")).await?;
        navigate(browser, "Organisations").await?;
        browser.titled(&titled("Organisations")).await?;
        browser
            .element(By::XPath("//main//a[@href = '/organisations/org-a']"))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/organisations/org-a")).await?;
        browser.goto("about:blank").await?;
        only_the_modules_refused(browser, "the links with the modules refused").await
    })
    .await
}
