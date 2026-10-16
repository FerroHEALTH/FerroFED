// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console in the browser: its submit disabled until the page has
//! hydrated, a query run that shows every node's status and whether the
//! answer is complete (§9.5, §11.1, §11.4, N37), a refused query shown with
//! the gateway's status and code, a patient named in a parameter that never
//! reaches the URL (N33), and the editor's tabs, target chips and the focus
//! move to the answer (WCAG 2.2 SC 2.1.1, 2.4.3, 4.1.2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_viewer::app::PRODUCT;
use thirtyfour::{By, Key, WebElement};

use crate::journeys::browser::{Browser, until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{PATIENT, Stack, UID_AT_A, patient_query};

/// The query form's submit button.
const SUBMIT: &str = "//form//button[@type = 'submit' and normalize-space(.) = 'Run query']";

/// The summary of the federation options' disclosure.
const OPTIONS: &str = "//details[contains(@class, 'federation-options')]/summary";

/// Waits until the query form's submit is enabled, which the bundle does
/// once it has hydrated the page.
pub(crate) async fn hydrated(browser: &Browser) -> Result<(), Box<dyn Error>> {
    browser
        .enabled(By::XPath(SUBMIT))
        .await
        .map_err(|error| format!("the query form never hydrated: {error}").into())
}

/// What one run of the query form sends.
struct Run<'a> {
    /// The AQL text.
    aql: &'a str,
    /// The first parameter row's name and value.
    parameter: (&'a str, &'a str),
    /// The endpoint chips to tick; every other is left clear.
    endpoints: &'a [&'a str],
    /// Whether the best-effort switch is on.
    partial: bool,
}

/// Replaces the text of the field `id` with `text`.
async fn fill(browser: &Browser, id: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let field = browser.element(By::Id(id)).await?;
    field.clear().await?;
    if !text.is_empty() {
        field.send_keys(text).await?;
    }
    Ok(())
}

/// Opens the federation options, unless they are open.
async fn options(browser: &Browser) -> Result<(), Box<dyn Error>> {
    let details = browser
        .element(By::Css("details.federation-options"))
        .await?;
    if details.attr("open").await?.is_none() {
        browser.element(By::XPath(OPTIONS)).await?.click().await?;
    }
    Ok(())
}

/// The checkbox of the endpoint chip of `id`.
async fn chip(browser: &Browser, id: &str) -> Result<WebElement, Box<dyn Error>> {
    browser
        .element(By::XPath(format!(
            "//input[@type = 'checkbox' and @name = 'form[endpoints]' and @value = '{id}']"
        )))
        .await
}

/// Fills the form with `run` and submits it.
async fn submit(browser: &Browser, run: &Run<'_>) -> Result<(), Box<dyn Error>> {
    fill(browser, "query-aql", run.aql).await?;
    fill(browser, "query-parameter-name-0", run.parameter.0).await?;
    fill(browser, "query-parameter-value-0", run.parameter.1).await?;
    options(browser).await?;
    for id in ["node-a-pub", "node-b-pub"] {
        let box_ = chip(browser, id).await?;
        if box_.is_selected().await? != run.endpoints.contains(&id) {
            box_.click().await?;
        }
    }
    let partial = browser.element(By::Id("query-partial")).await?;
    if partial.is_selected().await? != run.partial {
        partial.click().await?;
    }
    browser.element(By::XPath(SUBMIT)).await?.click().await?;
    Ok(())
}

/// Waits for the status the chip of the node `id` shows, and returns it.
async fn status_of(browser: &Browser, id: &str) -> Result<String, Box<dyn Error>> {
    let pill = browser
        .element(By::XPath(format!(
            "//ul[@aria-label = 'Nodes']/li[span[1] = '{id}']/span[contains(@class, 'pill')]"
        )))
        .await
        .map_err(|error| format!("no status for {id}: {error}"))?;
    Ok(pill.text().await?)
}

/// Checks the browser is still on the query console, whose URL names no
/// field of the form, the patient least of all.
async fn url_is_clean(browser: &Browser, stack: &Stack) -> Result<(), Box<dyn Error>> {
    let url = browser.driver().current_url().await?;
    assert_eq!(
        stack.url("/query"),
        url.as_str(),
        "a run leaves the URL as it was"
    );
    assert!(!url.as_str().contains(PATIENT), "{url}");
    Ok(())
}

/// Waits until the element in focus has the id `id`.
async fn focused(browser: &Browser, id: &str) -> Result<(), Box<dyn Error>> {
    until(&format!("the focus is on #{id}"), async || {
        let active = browser.driver().active_element().await?;
        Ok(active.attr("id").await?.as_deref() == Some(id))
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_query_run_shows_every_node_and_whether_the_answer_is_complete()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let aql = patient_query();
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;

        // Before the bundle runs, the form cannot be sent: the page the
        // server sends holds the submit disabled, and a browser that runs no
        // script never finds it enabled. The server may stream the form
        // after the shell, which only a script puts in place, so the browser
        // may find no submit at all.
        let session = browser
            .cookie(stack.cookie())
            .await?
            .ok_or("the session cookie")?;
        let page = reqwest::Client::new()
            .get(stack.url("/query"))
            .header("cookie", format!("{}={}", session.name, session.value))
            .send()
            .await?
            .text()
            .await?;
        assert!(
            page.contains(r#"<button type="submit" disabled class="#),
            "the page the server sends: {page}"
        );
        browser.scripts(false).await?;
        browser.goto(&stack.url("/query")).await?;
        browser.titled(&format!("Query · {PRODUCT}")).await?;
        for button in browser.driver().find_all(By::XPath(SUBMIT)).await? {
            assert!(!button.is_enabled().await?, "the submit before hydration");
        }
        browser.scripts(true).await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        browser
            .console_clean("the hydration of the query console")
            .await?;

        // Node A alone: a complete answer.
        let mut run = Run {
            aql: &aql,
            parameter: ("patient", PATIENT),
            endpoints: &["node-a-pub"],
            partial: false,
        };
        submit(browser, &run).await?;
        browser
            .text("span", "Every node in scope answered.")
            .await?;
        assert_eq!("active", status_of(browser, "node-a-pub").await?);
        browser.text("caption", "1 row").await?;
        browser.text("td", UID_AT_A).await?;
        url_is_clean(browser, &stack).await?;
        browser.console_clean("a complete answer").await?;

        // Both nodes, best effort: node B fails and the answer says it is
        // incomplete before its rows.
        run.endpoints = &[];
        run.partial = true;
        submit(browser, &run).await?;
        browser
            .text("span", "Not every node in scope answered")
            .await?;
        assert_eq!("active", status_of(browser, "node-a-pub").await?);
        assert_eq!("node-error", status_of(browser, "node-b-pub").await?);
        browser.text("caption", "1 row").await?;
        url_is_clean(browser, &stack).await?;
        browser.console_clean("an incomplete answer").await?;

        // Both nodes, all or nothing: the gateway fails the query with 424
        // and still names what each node did.
        run.partial = false;
        submit(browser, &run).await?;
        browser
            .text("strong", "The gateway failed the query: 424.")
            .await?;
        browser
            .text("span", "Not every node in scope answered")
            .await?;
        assert_eq!("active", status_of(browser, "node-a-pub").await?);
        assert_eq!("node-error", status_of(browser, "node-b-pub").await?);
        browser.text("p", "No rows.").await?;
        url_is_clean(browser, &stack).await?;
        browser.console_clean("a failed answer").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_query_shows_the_gateways_status_and_code() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        let run = Run {
            aql: "SELECT FROM",
            parameter: ("", ""),
            endpoints: &[],
            partial: false,
        };
        submit(browser, &run).await?;
        let alert = browser
            .text("p", "The gateway refused this view: 400")
            .await?;
        let text = alert.text().await?;
        assert_eq!(Some("alert"), alert.attr("role").await?.as_deref());
        assert!(
            !text.contains("(no code)"),
            "a refusal names its code: {text}"
        );
        // The refusal is the error state of the results card, and the focus
        // follows the run to it.
        browser
            .element(By::XPath(
                "//div[contains(@class, 'query-refusal')]/span[contains(@class, 'pill') and . = 'refused']",
            ))
            .await?;
        focused(browser, "query-results-title").await?;
        url_is_clean(browser, &stack).await?;
        browser.console_clean("a refused query").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_editor_tabs_the_target_chips_and_the_focus_move_work_from_the_keyboard()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;

        // The query kind tabs: the arrow keys move between them, the tab in
        // focus is the one shown, and only it is in the tab order.
        let aql_tab = browser.element(By::Id("query-tab-aql")).await?;
        assert_eq!(
            Some("true"),
            aql_tab.attr("aria-selected").await?.as_deref()
        );
        assert_eq!(
            Some("-1"),
            browser
                .element(By::Id("query-tab-stored"))
                .await?
                .attr("tabindex")
                .await?
                .as_deref()
        );
        aql_tab.click().await?;
        aql_tab.send_keys(Key::Right).await?;
        focused(browser, "query-tab-stored").await?;
        let stored_tab = browser.element(By::Id("query-tab-stored")).await?;
        assert_eq!(
            Some("true"),
            stored_tab.attr("aria-selected").await?.as_deref()
        );
        assert_eq!(Some("0"), stored_tab.attr("tabindex").await?.as_deref());
        assert!(
            browser
                .element(By::Id("query-name"))
                .await?
                .is_displayed()
                .await?
        );
        assert!(
            !browser
                .element(By::Id("query-aql"))
                .await?
                .is_displayed()
                .await?
        );
        stored_tab.send_keys(Key::Home).await?;
        focused(browser, "query-tab-aql").await?;
        assert!(
            browser
                .element(By::Id("query-aql"))
                .await?
                .is_displayed()
                .await?
        );
        browser.console_clean("the query kind tabs").await?;

        // The targets are real checkboxes, each with its label, ticked from
        // the keyboard; best effort is a switch.
        options(browser).await?;
        let node_a = chip(browser, "node-a-pub").await?;
        let label = browser
            .element(By::XPath(
                "//label[@for = //input[@value = 'node-a-pub']/@id]",
            ))
            .await?;
        assert!(label.text().await?.contains("node-a-pub"));
        assert!(!node_a.is_selected().await?);
        node_a.send_keys(Key::Space).await?;
        assert!(node_a.is_selected().await?);
        assert!(!chip(browser, "node-b-pub").await?.is_selected().await?);
        let switch = browser.element(By::Id("query-partial")).await?;
        assert_eq!(Some("switch"), switch.attr("role").await?.as_deref());
        assert_eq!(Some("checkbox"), switch.attr("type").await?.as_deref());

        // A run moves the focus to the results card, which names the one
        // node ticked as asked and the other as excluded.
        fill(browser, "query-aql", &patient_query()).await?;
        fill(browser, "query-parameter-name-0", "patient").await?;
        fill(browser, "query-parameter-value-0", PATIENT).await?;
        browser.element(By::XPath(SUBMIT)).await?.click().await?;
        browser
            .text("span", "Every node in scope answered.")
            .await?;
        focused(browser, "query-results-title").await?;
        assert_eq!("active", status_of(browser, "node-a-pub").await?);
        assert_eq!("excluded", status_of(browser, "node-b-pub").await?);

        // The answer's tabs: the raw result set, and back to the table.
        let table_tab = browser.element(By::Id("query-tab-table")).await?;
        table_tab.click().await?;
        table_tab.send_keys(Key::Right).await?;
        focused(browser, "query-tab-raw").await?;
        let raw = browser.element(By::Css("pre.raw-json")).await?;
        assert!(raw.is_displayed().await?);
        assert!(raw.text().await?.contains("\"federation\""));
        browser
            .element(By::Id("query-tab-raw"))
            .await?
            .send_keys(Key::Left)
            .await?;
        focused(browser, "query-tab-table").await?;
        assert!(!raw.is_displayed().await?);
        url_is_clean(browser, &stack).await?;
        browser
            .console_clean("the tabs, the chips and the focus")
            .await
    })
    .await
}
