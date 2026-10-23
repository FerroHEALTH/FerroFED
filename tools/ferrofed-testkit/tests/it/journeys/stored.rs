// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored queries in the browser: the list grouped by namespace with the
//! gateway's own namespace marked read-only and no AQL in it; the detail of
//! one query, switched between its versions on its Versions tab; and the
//! query console opened on a stored query by its name and version, with no
//! AQL and no parameter in the URL.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, hydrated, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{SECOND_TEXT, SECOND_VERSION, STORED, Stack, VERSIONED, held};

/// The gateway's own namespace.
const RESERVED: &str = "eu.ferrofed.eehrxf";

/// The title of the page called `section`.
fn titled(section: &str) -> String {
    format!("{section} · {PRODUCT}")
}

/// The head of the namespace card `namespace`.
fn card_head(namespace: &str) -> String {
    format!(
        "//section[@aria-label = '{namespace}']//div[contains(@class, 'fui-panel-section-head')]"
    )
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
async fn the_stored_queries_are_listed_by_namespace() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.follow("Stored queries").await?;
        browser.titled(&titled("Stored queries")).await?;
        let reserved = browser
            .element(By::XPath(card_head(RESERVED)))
            .await?
            .text()
            .await?;
        assert!(reserved.contains("Read-only"), "{reserved}");
        let other = browser
            .element(By::XPath(card_head("org.example")))
            .await?
            .text()
            .await?;
        assert!(!other.contains("Read-only"), "{other}");
        let queries = browser
            .driver()
            .find_all(By::XPath("//table[caption = 'org.example']/tbody/tr"))
            .await?;
        assert_eq!(
            STORED,
            u64::try_from(queries.len())?,
            "every query of org.example"
        );
        let versions = browser
            .driver()
            .find_all(By::XPath(
                "//main//table/tbody/tr/td//span[contains(@class, 'fui-tag-mono')]",
            ))
            .await?;
        assert_eq!(
            held()?,
            u64::try_from(versions.len())?,
            "every version the gateway holds"
        );
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
async fn the_detail_shows_each_version_its_versions_tab_names() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/stored-queries")).await?;
        browser
            .element(By::XPath(format!(
                "//table[caption = 'org.example']//a[normalize-space(.) = '{VERSIONED}']"
            )))
            .await?
            .click()
            .await?;
        browser.titled(&titled(VERSIONED)).await?;
        browser
            .text("p", &format!("Version {SECOND_VERSION} shown."))
            .await?;
        browser.console_clean("the detail of a stored query").await?;

        browser
            .element(By::Css(r#"a[data-tab="versions"]"#))
            .await?
            .click()
            .await?;
        browser
            .at(&stack.url(&format!("/stored-queries/{VERSIONED}/versions")))
            .await?;
        browser
            .element(By::XPath(
                "//main//table/tbody/tr/th//a[normalize-space(.) = '1.0.0']",
            ))
            .await?
            .click()
            .await?;
        browser.text("p", "Version 1.0.0 shown.").await?;
        browser
            .at(&stack.url(&format!("/stored-queries/{VERSIONED}?version=1.0.0")))
            .await?;
        browser
            .element(By::XPath(
                "//div[contains(@class, 'fui-code-panel')]//span[@class = 'fui-tok-parameter' and . = '$patient']",
            ))
            .await?;

        browser
            .goto(&stack.url(&format!(
                "/stored-queries/{VERSIONED}?version={SECOND_VERSION}"
            )))
            .await?;
        let code = browser
            .element(By::XPath("//div[contains(@class, 'fui-code-panel')]"))
            .await?
            .text()
            .await?;
        assert!(code.contains("c/name/value"), "the AQL of the version shown: {code}");
        assert!(!code.contains("$patient"), "{code}");
        assert!(SECOND_TEXT.contains("c/name/value"));
        browser.console_clean("the versions").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn run_in_the_query_console_carries_the_name_and_never_the_text() -> Result<(), Box<dyn Error>>
{
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
        browser.follow("Run in the query console").await?;
        browser
            .at(&stack.url(&format!(
                "/query/stored?stored={VERSIONED}&version={SECOND_VERSION}"
            )))
            .await?;
        hydrated(browser).await?;
        browser
            .element(By::Css(r#"a[data-tab="stored"][aria-current="page"]"#))
            .await?;
        assert_eq!(VERSIONED, value_of(browser, "query-name").await?);
        assert_eq!(SECOND_VERSION, value_of(browser, "query-version").await?);
        assert!(
            browser.driver().find_all(By::Id("aql")).await?.is_empty(),
            "the stored-query tab carries no AQL field"
        );
        let url = browser.driver().current_url().await?;
        assert!(!url.as_str().contains("SELECT"), "{url}");
        browser
            .console_clean("the query console opened on a stored query")
            .await
    })
    .await
}
