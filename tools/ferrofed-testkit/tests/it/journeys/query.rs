// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console in the browser: its submit disabled until the page has
//! hydrated, a query run that shows every node's status and whether the
//! answer is complete (§9.5, §11.1, §11.4, N37), a refused query shown with
//! the gateway's status and code, and a patient named in a parameter that
//! never reaches the URL (N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_viewer::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{PATIENT, Stack, UID_AT_A, patient_query};

/// The query form's submit button.
const SUBMIT: &str = "//form//button[@type = 'submit' and . = 'Run the query']";

/// The caption of the table of every endpoint's status.
const ENDPOINTS: &str = "Every endpoint the gateway reports";

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
    /// The parameters, one `name=value` per line.
    parameters: &'a str,
    /// The endpoints to target, comma-separated.
    endpoints: &'a str,
    /// Whether the best-effort box is ticked.
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

/// Fills the form with `run` and submits it.
async fn submit(browser: &Browser, run: &Run<'_>) -> Result<(), Box<dyn Error>> {
    fill(browser, "query-aql", run.aql).await?;
    fill(browser, "query-parameters", run.parameters).await?;
    fill(browser, "query-endpoints", run.endpoints).await?;
    let partial = browser.element(By::Id("query-partial")).await?;
    if partial.is_selected().await? != run.partial {
        partial.click().await?;
    }
    browser.element(By::XPath(SUBMIT)).await?.click().await?;
    Ok(())
}

/// Waits for the status the endpoint table gives `id`, and returns it.
async fn status_of(browser: &Browser, id: &str) -> Result<String, Box<dyn Error>> {
    let cell = browser
        .element(By::XPath(format!(
            "//table[caption = '{ENDPOINTS}']/tbody/tr[th = '{id}']/td[1]"
        )))
        .await
        .map_err(|error| format!("no status for {id}: {error}"))?;
    Ok(cell.text().await?)
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

#[tokio::test(flavor = "multi_thread")]
async fn a_query_run_shows_every_node_and_whether_the_answer_is_complete()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let aql = patient_query();
    let parameters = format!("patient={PATIENT}");
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
            page.contains(r#"<button type="submit" disabled>Run the query</button>"#),
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
            parameters: &parameters,
            endpoints: "node-a-pub",
            partial: false,
        };
        submit(browser, &run).await?;
        browser.text("strong", "Complete.").await?;
        assert_eq!("active", status_of(browser, "node-a-pub").await?);
        browser.text("caption", "1 row").await?;
        browser.text("td", UID_AT_A).await?;
        url_is_clean(browser, &stack).await?;
        browser.console_clean("a complete answer").await?;

        // Both nodes, best effort: node B fails and the answer says it is
        // incomplete before its rows.
        run.endpoints = "";
        run.partial = true;
        submit(browser, &run).await?;
        browser.text("strong", "Incomplete answer.").await?;
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
        browser.text("strong", "Incomplete answer.").await?;
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
            parameters: "",
            endpoints: "",
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
        url_is_clean(browser, &stack).await?;
        browser.console_clean("a refused query").await
    })
    .await
}
