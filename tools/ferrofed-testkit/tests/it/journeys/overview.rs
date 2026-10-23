// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The overview in the browser: the dashboard with its counts, the way from
//! it to an organisation and a node and back, a node the gateway does not
//! declare answered as not found, and a node's routes read from every page
//! of the routing table.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{ROUTES, Stack};

/// The title of the page called `section`.
fn titled(section: &str) -> String {
    format!("{section} · {PRODUCT}")
}

/// The number of rows in the body of the table captioned `caption`.
async fn rows(browser: &Browser, caption: &str) -> Result<usize, Box<dyn Error>> {
    Ok(browser
        .driver()
        .find_all(By::XPath(format!(
            "//table[caption = '{caption}']/tbody/tr"
        )))
        .await?
        .len())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dashboard_counts_the_federation_and_leads_to_a_node() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.text("span", "Organisations").await?;
        browser.text("span", "Nodes reachable").await?;
        browser.text("span", "Open incidents").await?;
        browser.text("h2", "Nodes that need attention").await?;
        browser.console_clean("the dashboard").await?;

        browser
            .element(By::XPath("//main//a[@href = '/organisations/org-a']"))
            .await?
            .click()
            .await?;
        browser.titled(&titled("org-a")).await?;
        browser.at(&stack.url("/organisations/org-a")).await?;
        browser
            .element(By::XPath(
                "//main//a[@href = '/organisations/org-a/node-a']",
            ))
            .await?
            .click()
            .await?;
        browser.titled(&titled("node-a")).await?;
        browser.text("h1", "node-a").await?;
        browser.text("li", "node-a-pub").await?;
        browser.console_clean("the node's page").await?;

        // The nested navigation names the organisation and its nodes.
        browser
            .element(By::XPath(
                "//nav[@aria-label = 'Console']//a[@href = '/organisations/org-a/node-a']",
            ))
            .await?;
        browser
            .element(By::XPath(
                "//nav[@aria-label = 'Breadcrumb']//a[@href = '/organisations']",
            ))
            .await?
            .click()
            .await?;
        browser.titled(&titled("Organisations")).await?;
        browser.follow("Dashboard").await?;
        browser.titled(&titled("Dashboard")).await?;
        browser.at(&stack.url("/")).await?;
        browser.console_clean("the way back to the dashboard").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_the_gateway_does_not_declare_is_not_found() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser
            .goto(&stack.url("/organisations/org-a/node-z"))
            .await?;
        browser.text("h1", "Not found").await?;
        // The page answers 404, which the browser logs as an error: that
        // answer is the page under test, so it is the one entry allowed.
        let errors = browser.console_errors().await?;
        let [error] = errors.as_slice() else {
            panic!("the page of an unknown node logged {errors:?}, not its one 404");
        };
        assert!(
            error.contains("/organisations/org-a/node-z") && error.contains("404"),
            "the page of an unknown node logged {error}, not its 404"
        );
        Ok(())
    })
    .await
}

// The routing table answers in pages of 100; a node's Routes tab reads every
// page, so each of its routes shows, its own system_id first.
#[tokio::test(flavor = "multi_thread")]
async fn a_nodes_routes_are_read_from_every_page_of_the_routing_table() -> Result<(), Box<dyn Error>>
{
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser
            .goto(&stack.url("/organisations/org-a/node-a/routes"))
            .await?;
        browser.titled(&titled("node-a")).await?;
        browser
            .element(By::XPath(
                "//table[caption = 'Routes']//code[. = 'legacy-099.example.org']",
            ))
            .await?;
        let node_a = usize::try_from(ROUTES)? - 1;
        assert_eq!(node_a, rows(browser, "Routes").await?, "node A's routes");
        browser.console_clean("the node's routes").await
    })
    .await
}
