// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator views in the browser: the members, the integrity incidents
//! and the routing table, the stored queries and the self-description, each
//! reached by the hydrated console's own navigation and loaded afresh from
//! the server, with both pagers followed forward and back.
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
use crate::journeys::stack::{ROUTES, Stack, held};

/// The most rows a page of a view shows.
const PAGE: u64 = 100;

/// The caption of the routing table.
const ROUTING: &str = "The creating_system_id routing table";

/// The caption of the stored-query table.
const HELD: &str = "Held versions";

/// The title of the view called `section`.
fn titled(section: &str) -> String {
    format!("{section} · {PRODUCT}")
}

/// Waits for the row headed `id` in the table captioned `caption`.
async fn row(browser: &Browser, caption: &str, id: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::XPath(format!(
            "//table[caption = '{caption}']/tbody/tr/th[@scope = 'row' and normalize-space(.) = '{id}']"
        )))
        .await
        .map_err(|error| format!("no row {id} in the table {caption:?}: {error}"))?;
    Ok(())
}

/// The number of rows in the body of the table captioned `caption`.
async fn rows(browser: &Browser, caption: &str) -> Result<u64, Box<dyn Error>> {
    let found = browser
        .driver()
        .find_all(By::XPath(format!(
            "//table[caption = '{caption}']/tbody/tr"
        )))
        .await?;
    Ok(u64::try_from(found.len())?)
}

/// Waits for the pager of the page that starts at `offset`, of `total` rows
/// in all, and checks the table captioned `caption` shows that page.
async fn page(
    browser: &Browser,
    caption: &str,
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
    assert_eq!(
        end.saturating_sub(offset),
        rows(browser, caption).await?,
        "the rows of {caption:?} from {offset}"
    );
    Ok(())
}

/// Follows the pager of the view at `path` forward to its last page and back
/// to its first, checking the URL names each page.
async fn pages(
    browser: &Browser,
    stack: &Stack,
    (path, caption, total): (&str, &str, u64),
) -> Result<(), Box<dyn Error>> {
    page(browser, caption, 0, total).await?;
    browser.follow("Next page").await?;
    page(browser, caption, PAGE, total).await?;
    browser
        .at(&stack.url(&format!("{path}?offset={PAGE}")))
        .await?;
    browser.follow("Previous page").await?;
    page(browser, caption, 0, total).await?;
    browser.at(&stack.url(&format!("{path}?offset=0"))).await?;
    browser.console_clean(&format!("the pager of {path}")).await
}

#[tokio::test(flavor = "multi_thread")]
async fn each_view_renders_and_pages_through_the_hydrated_console() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;

        browser.follow("Members").await?;
        browser.titled(&titled("Members")).await?;
        row(browser, "Member endpoints", "node-a-pub").await?;
        row(browser, "Member endpoints", "node-b-pub").await?;
        browser.console_clean("the members view").await?;

        browser.follow("Integrity").await?;
        browser.titled(&titled("Integrity")).await?;
        browser
            .text("caption", "Incidents since the gateway started")
            .await?;
        pages(browser, &stack, ("/integrity", ROUTING, ROUTES)).await?;

        browser.follow("Stored queries").await?;
        browser.titled(&titled("Stored queries")).await?;
        pages(browser, &stack, ("/stored-queries", HELD, held()?)).await?;

        browser.follow("Self-description").await?;
        browser.titled(&titled("Self-description")).await?;
        browser.text("dd", "example-federation").await?;
        browser.console_clean("the self-description view").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn each_view_loads_from_the_server_and_hydrates() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;

        browser.goto(&stack.url("/members")).await?;
        browser.titled(&titled("Members")).await?;
        row(browser, "Member endpoints", "node-a-pub").await?;
        browser.console_clean("the members view").await?;

        browser
            .goto(&stack.url(&format!("/integrity?offset={PAGE}")))
            .await?;
        browser.titled(&titled("Integrity")).await?;
        page(browser, ROUTING, PAGE, ROUTES).await?;
        browser.follow("Previous page").await?;
        page(browser, ROUTING, 0, ROUTES).await?;
        browser.console_clean("the integrity view").await?;

        browser
            .goto(&stack.url(&format!("/stored-queries?offset={PAGE}")))
            .await?;
        browser.titled(&titled("Stored queries")).await?;
        page(browser, HELD, PAGE, held()?).await?;
        browser.follow("Previous page").await?;
        page(browser, HELD, 0, held()?).await?;
        browser.console_clean("the stored-query view").await?;

        browser.goto(&stack.url("/federation")).await?;
        browser.titled(&titled("Self-description")).await?;
        browser.text("dd", "example-federation").await?;
        browser.console_clean("the self-description view").await?;

        // The query console's submit is enabled once the bundle has run, so
        // the navigation there ends the journey on a hydrated page, and the
        // console check after it reads what the bundle logged on the way.
        browser.follow("Query").await?;
        hydrated(browser).await?;
        browser
            .console_clean("the way back to the query console")
            .await
    })
    .await
}
