// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console in the browser: a query run that posts the form and
//! answers on the same page with every node's status and whether the answer
//! is complete (§9.5, §11.1, §11.4, N37), with no script as well as with
//! it; a refused query shown with the gateway's status and code; a
//! parameter row added and removed by the form; a patient named in a
//! parameter that never reaches the URL (N33); and the tabs between AQL and
//! a stored query.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_console::app::PRODUCT;
use thirtyfour::{By, WebElement};

use crate::journeys::browser::{Browser, hydrated, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{PATIENT, Stack, UID_AT_A, patient_query};

/// The query form's submit button.
const SUBMIT: &str =
    "//form[@id = 'query-form']//button[@type = 'submit' and normalize-space(.) = 'Run']";

/// What one run of the query form sends.
pub(crate) struct Run<'a> {
    /// The AQL text.
    pub(crate) aql: &'a str,
    /// The first parameter row's name and value.
    pub(crate) parameter: (&'a str, &'a str),
    /// The endpoints to tick; every other is left clear.
    pub(crate) endpoints: &'a [&'a str],
    /// Whether the best-effort switch is on.
    pub(crate) partial: bool,
}

/// Replaces the text of the field `id` with `text`: the field emptied, then
/// the new text typed.
async fn fill(browser: &Browser, id: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let field = browser.element(By::Id(id)).await?;
    browser.press(&field).await?;
    // NOTE: no specification governs this: our own design; the text is cleared
    // with an input event, so an island that keeps the field's text sees it.
    browser
        .driver()
        .execute(
            "arguments[0].value = ''; arguments[0].dispatchEvent(new Event('input', { bubbles: true }));",
            vec![field.to_json()?],
        )
        .await?;
    if !text.is_empty() {
        field.send_keys(text).await?;
    }
    Ok(())
}

/// Opens the federation options, unless they are open.
async fn options(browser: &Browser) -> Result<(), Box<dyn Error>> {
    let details = browser
        .element(By::XPath(
            "//form[@id = 'query-form']//details[summary[starts-with(., 'Federation options')]]",
        ))
        .await?;
    if details.attr("open").await?.is_none() {
        browser
            .press(&details.find(By::Tag("summary")).await?)
            .await?;
    }
    Ok(())
}

/// The checkbox of the endpoint `id`.
async fn endpoint(browser: &Browser, id: &str) -> Result<WebElement, Box<dyn Error>> {
    browser
        .element(By::Id(format!("query-endpoint-{id}")))
        .await
}

/// Pastes `text` into the field `id`: one input event with the whole text,
/// as a paste is (FerroHEALTH/FerroUI#333 tracks the editor losing keys typed
/// faster than it redraws).
async fn paste(browser: &Browser, id: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let field = browser.element(By::Id(id)).await?;
    browser.press(&field).await?;
    browser
        .driver()
        .execute(
            "arguments[0].value = arguments[1]; arguments[0].dispatchEvent(new Event('input', { bubbles: true }));",
            vec![field.to_json()?, serde_json::json!(text)],
        )
        .await?;
    Ok(())
}

/// Fills the form with `run` and submits it.
pub(crate) async fn submit(browser: &Browser, run: &Run<'_>) -> Result<(), Box<dyn Error>> {
    paste(browser, "aql", run.aql).await?;
    fill(browser, "parameter-name-1", run.parameter.0).await?;
    fill(browser, "parameter-value-1", run.parameter.1).await?;
    options(browser).await?;
    for id in ["node-a-pub", "node-b-pub"] {
        let ticked = endpoint(browser, id).await?;
        if ticked.is_selected().await? != run.endpoints.contains(&id) {
            browser.press(&ticked).await?;
        }
    }
    let partial = browser.element(By::Id("query-partial")).await?;
    if partial.is_selected().await? != run.partial {
        browser.press(&partial).await?;
    }
    let run = browser.element(By::XPath(SUBMIT)).await?;
    browser.press(&run).await?;
    Ok(())
}

/// Waits for the status the fan-out names for the endpoint `id`, and
/// returns it.
async fn status_of(browser: &Browser, id: &str) -> Result<String, Box<dyn Error>> {
    let term = browser
        .element(By::XPath(format!(
            "//ul[contains(@class, 'fui-fanout-list')]/li[.//code[normalize-space(.) = '{id}']]//span[contains(@class, 'fui-fanout-term')]"
        )))
        .await
        .map_err(|error| format!("no status for {id}: {error}"))?;
    Ok(term.text().await?.trim().to_owned())
}

/// Checks the browser is on the query console, whose URL names no field of
/// the form, the patient least of all.
async fn url_is_clean(browser: &Browser, stack: &Stack) -> Result<(), Box<dyn Error>> {
    let url = browser.driver().current_url().await?;
    assert_eq!(
        stack.url("/query"),
        url.as_str(),
        "a run posts to the page and keeps its URL"
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
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/query")).await?;
        browser
            .titled(&format!("Query console · {PRODUCT}"))
            .await?;
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
        browser.text("span", "Complete").await?;
        assert_eq!("active", status_of(browser, "node-a-pub").await?);
        assert_eq!("excluded", status_of(browser, "node-b-pub").await?);
        browser.text("td", UID_AT_A).await?;
        url_is_clean(browser, &stack).await?;
        // The form keeps what was run, so the next run starts from it.
        let kept = browser
            .element(By::Id("aql"))
            .await?
            .prop("value")
            .await?
            .unwrap_or_default();
        assert!(kept.contains("$patient"), "{kept}");
        browser.console_clean("a complete answer").await?;

        // Both nodes, best effort: node B fails and the answer says it is
        // incomplete.
        run.endpoints = &[];
        run.partial = true;
        submit(browser, &run).await?;
        browser.text("span", "Incomplete").await?;
        assert_eq!("active", status_of(browser, "node-a-pub").await?);
        assert_eq!("node-error", status_of(browser, "node-b-pub").await?);
        browser.text("td", UID_AT_A).await?;
        url_is_clean(browser, &stack).await?;
        browser.console_clean("an incomplete answer").await?;

        // Both nodes, all or nothing: the gateway fails the query with 424
        // and still names what each node did.
        run.partial = false;
        submit(browser, &run).await?;
        browser.text("span", "status 424").await?;
        assert_eq!("active", status_of(browser, "node-a-pub").await?);
        assert_eq!("node-error", status_of(browser, "node-b-pub").await?);
        browser.text("p", "The answer holds no rows.").await?;
        url_is_clean(browser, &stack).await?;
        browser.console_clean("a failed answer").await
    })
    .await
}

// The query runs with no WebAssembly: the form is a plain post and the
// server answers with the page.
#[tokio::test(flavor = "multi_thread")]
async fn a_query_runs_with_no_script() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let aql = patient_query();
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.scripts(false).await?;
        browser.goto(&stack.url("/query")).await?;
        browser
            .titled(&format!("Query console · {PRODUCT}"))
            .await?;
        let run = Run {
            aql: &aql,
            parameter: ("patient", PATIENT),
            endpoints: &["node-a-pub"],
            partial: false,
        };
        submit(browser, &run).await?;
        browser.text("td", UID_AT_A).await?;
        url_is_clean(browser, &stack).await?;
        browser.scripts(true).await?;
        Ok(())
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
            .element(By::XPath("//main//*[@role = 'alert'][contains(., '400')]"))
            .await?;
        let text = alert.text().await?;
        assert!(text.contains("The gateway refused the request."), "{text}");
        url_is_clean(browser, &stack).await?;
        browser.console_clean("a refused query").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parameter_row_is_added_and_removed_and_the_tabs_switch_the_kind()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/query")).await?;
        hydrated(browser).await?;
        let rows = async || -> Result<usize, Box<dyn Error>> {
            Ok(browser
                .driver()
                .find_all(By::Css(
                    r#"#query-parameters [data-repeat="rows"] [data-row]"#,
                ))
                .await?
                .len())
        };
        assert_eq!(1, rows().await?);
        let add = browser
            .element(By::Css(r#"button[data-repeat="add"]"#))
            .await?;
        browser.press(&add).await?;
        crate::journeys::browser::until("Add puts a second row", async || Ok(rows().await? == 2))
            .await?;
        let remove = browser
            .element(By::Css(
                r#"[data-repeat="rows"] [data-row]:last-child button[data-repeat="remove"]"#,
            ))
            .await?;
        browser.press(&remove).await?;
        crate::journeys::browser::until("Remove takes it out", async || Ok(rows().await? == 1))
            .await?;
        browser.console_clean("the parameter rows").await?;

        let tab = browser.element(By::Css(r#"a[data-tab="stored"]"#)).await?;
        browser.press(&tab).await?;
        browser.at(&stack.url("/query/stored")).await?;
        browser.element(By::Id("query-name")).await?;
        assert!(
            browser.driver().find_all(By::Id("aql")).await?.is_empty(),
            "the stored tab has no AQL field"
        );
        let tab = browser.element(By::Css(r#"a[data-tab="aql"]"#)).await?;
        browser.press(&tab).await?;
        browser.at(&stack.url("/query")).await?;
        browser.element(By::Id("aql")).await?;
        browser.console_clean("the query kind tabs").await
    })
    .await
}
