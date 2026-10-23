// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The overview in the browser: the dashboard with its counts and each
//! endpoint's health, the way from it to a member's page and back, reached
//! by the hydrated console's own navigation, and the routing table's filter
//! over the rows its page loaded.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::query::hydrated;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// The title of the view called `section`.
fn titled(section: &str) -> String {
    format!("{section} · {PRODUCT}")
}

/// The number of rows in the body of the table captioned `caption` that
/// carry a row header.
async fn rows(browser: &Browser, caption: &str) -> Result<usize, Box<dyn Error>> {
    Ok(browser
        .driver()
        .find_all(By::XPath(format!(
            "//table[caption = '{caption}']/tbody/tr[th]"
        )))
        .await?
        .len())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dashboard_counts_the_federation_and_leads_to_a_members_page()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.text("span", "Members active").await?;
        browser.text("span", "Nodes up").await?;
        browser.text("span", "Open incidents").await?;
        browser.text("span", "Stored queries").await?;
        browser.text("h2", "Member health").await?;
        browser.text("h2", "Recent incidents").await?;
        browser.console_clean("the dashboard").await?;

        browser.follow("node-a-pub").await?;
        browser.titled(&titled("Member node-a")).await?;
        browser.text("h1", "node-a").await?;
        browser
            .element(By::XPath(
                "//table[caption = 'Endpoints of this member']/tbody/tr/th[normalize-space(.) = 'node-a-pub']",
            ))
            .await?;
        assert_eq!(1, rows(browser, "Endpoints of this member").await?);
        browser.at(&stack.url("/members/node-a")).await?;
        browser.console_clean("the member's page").await?;

        browser
            .element(By::XPath(
                "//main//a[normalize-space(.) = 'Members']",
            ))
            .await?
            .click()
            .await?;
        browser.titled(&titled("Members")).await?;
        browser
            .element(By::XPath(
                "//table[caption = 'Member endpoints']/tbody/tr/th/a[normalize-space(.) = 'node-b-pub']",
            ))
            .await?
            .click()
            .await?;
        browser.titled(&titled("Member node-b")).await?;
        browser.at(&stack.url("/members/node-b")).await?;

        // The dashboard is reached by the client router as well: its markup
        // comes through its server function.
        browser.follow("Dashboard").await?;
        browser.titled(&titled("Dashboard")).await?;
        browser.text("h2", "Member health").await?;
        browser.at(&stack.url("/")).await?;
        browser.console_clean("the way back to the dashboard").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_members_page_the_gateway_does_not_declare_is_not_found() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/members/node-z")).await?;
        browser.text("h1", "No such member").await?;
        // The page answers 404, which the browser logs as an error: that
        // answer is the page under test, so it is the one entry allowed.
        let errors = browser.console_errors().await?;
        let [error] = errors.as_slice() else {
            panic!("the page of an unknown member logged {errors:?}, not its one 404");
        };
        assert!(
            error.contains("/members/node-z") && error.contains("404"),
            "the page of an unknown member logged {error}, not its 404"
        );
        Ok(())
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_routing_filter_keeps_the_rows_that_match() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        // The filter runs once the page has hydrated, so the journey reaches
        // the routing page through the hydrated console's navigation.
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        browser.follow("Routing").await?;
        browser.titled(&titled("Routing")).await?;
        let caption = "The creating_system_id routing table";
        browser.text("p", "100 of 100 rows on this page.").await?;

        let field = browser.element(By::Id("routing-filter")).await?;
        field.send_keys("LEGACY-007").await?;
        browser.text("p", "1 of 100 rows on this page.").await?;
        until("the table keeps the one row", async || {
            Ok(rows(browser, caption).await? == 1)
        })
        .await?;
        browser
            .element(By::XPath(format!(
                "//table[caption = '{caption}']/tbody/tr/th[normalize-space(.) = 'legacy-007.example.org']"
            )))
            .await?;

        field.clear().await?;
        field.send_keys("no-such-route").await?;
        browser.text("p", "0 of 100 rows on this page.").await?;
        browser
            .text("td", "No row on this page matches the filter.")
            .await?;
        // The filter is the page's own: the URL never carries it.
        browser.at(&stack.url("/routing")).await?;
        browser.console_clean("the routing filter").await
    })
    .await
}
