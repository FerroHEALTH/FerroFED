// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored queries in the browser: the list grouped by namespace with the
//! gateway's own namespace marked read-only, paged forward and back; the
//! detail of one query, switched between its versions by the URL; and the
//! query console opened on a stored query by its name and version, with no
//! AQL and no parameter in the URL.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_viewer::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::query::hydrated;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{SECOND_TEXT, SECOND_VERSION, Stack, VERSIONED, held};

/// The most versions a page of the list names.
const PAGE: u64 = 100;

/// Every chip of the list, one per version the page names.
const CHIPS: &str = "//section[@aria-label = 'Namespaces']//li/a";

/// The gateway's own namespace.
const RESERVED: &str = "eu.ferrofed.eehrxf";

/// The title of the page called `section`.
fn titled(section: &str) -> String {
    format!("{section} · {PRODUCT}")
}

/// The header of the namespace card titled `namespace`.
fn card_header(namespace: &str) -> String {
    format!("//section[@aria-label = 'Namespaces']//div[h2[normalize-space(.) = '{namespace}']]")
}

/// Waits for the pager of the page of the list that starts at `offset`, of
/// `total` versions in all, and checks the page names that many chips.
pub(crate) async fn list_page(
    browser: &Browser,
    offset: u64,
    total: u64,
) -> Result<(), Box<dyn Error>> {
    let end = offset.saturating_add(PAGE).min(total);
    browser
        .text(
            "p",
            &format!("Rows {} to {end} of {total}.", offset.saturating_add(1)),
        )
        .await?;
    let chips = browser.driver().find_all(By::XPath(CHIPS)).await?;
    assert_eq!(
        end.saturating_sub(offset),
        u64::try_from(chips.len())?,
        "the chips of the page from {offset}"
    );
    Ok(())
}

/// The value of the field `id`.
async fn value_of(browser: &Browser, id: &str) -> Result<String, Box<dyn Error>> {
    Ok(browser
        .element(By::Id(id))
        .await?
        .prop("value")
        .await?
        .unwrap_or_default())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stored_queries_are_listed_by_namespace_and_paged() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        browser.follow("Stored queries").await?;
        browser.titled(&titled("Stored queries")).await?;
        browser
            .element(By::XPath(format!(
                "{}/span[contains(., 'read-only')]",
                card_header(RESERVED)
            )))
            .await?;
        let other = browser
            .element(By::XPath(card_header("org.example")))
            .await?
            .text()
            .await?;
        assert!(!other.contains("read-only"), "{other}");
        let total = held()?;
        list_page(browser, 0, total).await?;
        browser.follow("Next").await?;
        list_page(browser, PAGE, total).await?;
        browser
            .at(&stack.url(&format!("/stored-queries?offset={PAGE}")))
            .await?;
        browser.follow("Previous").await?;
        list_page(browser, 0, total).await?;
        let text = browser.driver().source().await?;
        assert!(
            !text.contains("CONTAINS COMPOSITION"),
            "the list shows no AQL"
        );
        browser.console_clean("the stored-query list").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_detail_shows_each_version_the_switch_names() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/stored-queries")).await?;
        browser.titled(&titled("Stored queries")).await?;
        browser
            .element(By::XPath(format!(
                "{CHIPS}[normalize-space(.) = 'journey_001@1.0.0']"
            )))
            .await?
            .click()
            .await?;
        browser.titled(&titled(VERSIONED)).await?;
        browser.text("p", "Version 1.0.0, stored").await?;
        browser
            .element(By::XPath(
                "//div[contains(@class, 'fui-code-panel')]//span[@class = 'fui-tok-parameter' and . = '$patient']",
            ))
            .await?;
        browser
            .element(By::XPath(
                "//nav[@aria-label = 'Versions']//a[@aria-current = 'page'][span[normalize-space(.) = '1.0.0']]",
            ))
            .await?;
        browser.console_clean("the detail of a stored query").await?;

        browser
            .element(By::XPath(format!(
                "//nav[@aria-label = 'Versions']//a[span[normalize-space(.) = '{SECOND_VERSION}']]"
            )))
            .await?
            .click()
            .await?;
        browser
            .text("p", &format!("Version {SECOND_VERSION}, stored"))
            .await?;
        browser
            .at(&stack.url(&format!(
                "/stored-queries/{VERSIONED}?version={SECOND_VERSION}"
            )))
            .await?;
        let code = browser
            .element(By::XPath("//div[contains(@class, 'fui-code-panel')]"))
            .await?
            .text()
            .await?;
        assert_eq!(SECOND_TEXT, code.trim(), "the AQL of the version shown");
        browser
            .element(By::XPath(format!(
                "//nav[@aria-label = 'Versions']//a[@aria-current = 'page'][span[normalize-space(.) = '{SECOND_VERSION}']]"
            )))
            .await?;
        browser.console_clean("the version switch").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn open_in_query_console_carries_the_name_and_never_the_text() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser
            .goto(&stack.url(&format!(
                "/stored-queries/{VERSIONED}?version={SECOND_VERSION}"
            )))
            .await?;
        browser.titled(&titled(VERSIONED)).await?;
        browser.follow("Open in query console").await?;
        browser
            .at(&stack.url(&format!(
                "/query?stored={VERSIONED}&version={SECOND_VERSION}"
            )))
            .await?;
        hydrated(browser).await?;
        let stored_tab = browser.element(By::Id("query-tab-stored")).await?;
        assert_eq!(
            Some("true"),
            stored_tab.attr("aria-selected").await?.as_deref(),
            "the console opens on the stored query"
        );
        assert!(
            browser
                .element(By::Id("query-name"))
                .await?
                .is_displayed()
                .await?,
            "the stored query's fields are shown"
        );
        assert!(
            !browser
                .element(By::Id("query-aql"))
                .await?
                .is_displayed()
                .await?,
            "the AQL editor is hidden"
        );
        assert_eq!(VERSIONED, value_of(browser, "query-name").await?);
        assert_eq!(SECOND_VERSION, value_of(browser, "query-version").await?);
        assert_eq!("", value_of(browser, "query-aql").await?);
        let url = browser.driver().current_url().await?;
        assert!(!url.as_str().contains("SELECT"), "{url}");
        browser
            .console_clean("the query console opened on a stored query")
            .await
    })
    .await
}
