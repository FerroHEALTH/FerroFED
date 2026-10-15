// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page in the browser, against a stub admin listener:
//! its forms disabled until the page has hydrated, the save of a console
//! member answered with the preview of what the saved revision changes, the
//! edit of a console member's FHIR endpoint, and the apply of the newest
//! revision, each without a console error.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::mock::Server;
use ferrofed_viewer::app::PRODUCT;
use thirtyfour::By;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// Revision 1 runs, applied by the file, and revision 2 is saved.
const STANDING: &str = r#"{"ui":"managed","head":2,"current":{"revision":1,"previous":null,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"file"},
    "applied_at":"2026-10-15T08:00:00Z"},"running":1,"in_step":true,"refused":null}"#;

/// Member A the file provides, console member B with a FHIR endpoint, and
/// a writable stored-query registry.
const RESOURCES: &str = r#"{"running":1,"head":2,"members":[
    {"node":"node-a","organisation":"org-a","system_id":"cdr-a.example.org","endpoints":["node-a-pub"],"owner":"file"},
    {"node":"node-b","organisation":"org-b","system_id":"cdr-b.example.org","endpoints":["node-b-pub","node-b-fhir"],
     "owner":"console","state":"pending",
     "member":{"organisation":{"id":"org-b"},"node":{"id":"node-b","system_id":"cdr-b.example.org"},
       "endpoints":[{"id":"node-b-pub","url":"https://cdr-b.example.org/openehr","connection_type":"openehr-rest-query"},
         {"id":"node-b-fhir","url":"https://cdr-b.example.org/fhir","connection_type":"hl7-fhir-rest",
          "categories":["Patient-Summaries"],"profiles":["http://example.org/ps"],"ehr_id_system":"urn:oid:2.999.1"}]}}],
    "stored_query_registry":"writable","stored_queries":[]}"#;

/// The history of revisions 1 and 2.
const HISTORY: &str = r#"{"items":[
    {"id":2,"base":1,"origin":"operator","restores":null,
     "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
     "at":"2026-10-15T09:00:00Z","reason":"raise a budget","toml":""},
    {"id":1,"base":null,"origin":"file","restores":null,"author":{"kind":"file"},
     "at":"2026-10-15T08:00:00Z","reason":"the configuration file was loaded","toml":""}],
    "offset":0,"total":2}"#;

/// What revision 2 changes against revision 1.
const DIFF_2: &str = r#"{"from":1,"to":2,"changes":[
    {"key":"federation.per_node_timeout_ms","change":"changed","from":"2000","to":"2500"}]}"#;

/// What revision 3, the save, changes against revision 1.
const DIFF_3: &str = r#"{"from":1,"to":3,"changes":[
    {"key":"registry.members[0].node.id","change":"added","from":null,"to":"\"node-c\""}]}"#;

/// Revision 3 as the save answers it.
const SAVED: &str = r#"{"id":3,"base":2,"origin":"operator","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:05:00Z","reason":"add C","toml":""}"#;

/// Revision 2 applied.
const APPLIED: &str = r#"{"revision":{"id":2,"base":1,"origin":"operator","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:00:00Z","reason":"raise a budget","toml":""},
    "current":{"revision":2,"previous":1,"outcome":"applied","needs_restart":[],
    "applied_by":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "applied_at":"2026-10-15T09:06:00Z"}}"#;

/// A stub admin listener answering every route the page takes.
pub(crate) async fn listener() -> Server {
    let listener = Server::start().await;
    for (verb, route, status, body) in [
        ("GET", "/admin/config", 200, STANDING),
        ("GET", "/admin/config/resources", 200, RESOURCES),
        ("GET", "/admin/config/revisions", 200, HISTORY),
        ("GET", "/admin/config/revisions/1/diff/2", 200, DIFF_2),
        ("GET", "/admin/config/revisions/1/diff/3", 200, DIFF_3),
        ("PUT", "/admin/config/members/node-c", 201, SAVED),
        ("PUT", "/admin/config/members/node-b", 201, SAVED),
        ("POST", "/admin/config/revisions/2/apply", 200, APPLIED),
    ] {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_raw(body.as_bytes().to_vec(), "application/json"),
            )
            .mount(&listener)
            .await;
    }
    listener
}

/// The submit of the form under the heading `heading`.
fn submit(heading: &str) -> By {
    By::XPath(format!(
        "//section[h2 = '{heading}']//button[@type = 'submit']"
    ))
}

/// Replaces the text of the field `id` with `text`.
async fn fill(browser: &Browser, id: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let field = browser.element(By::Id(id)).await?;
    field.clear().await?;
    field.send_keys(text).await?;
    Ok(())
}

/// Loads the page and waits until its forms run.
async fn hydrated(browser: &Browser, stack: &Stack) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url("/configuration")).await?;
    browser
        .titled(&format!("Configuration · {PRODUCT}"))
        .await?;
    browser
        .enabled(submit("Add a console member"))
        .await
        .map_err(|error| format!("the configuration page never hydrated: {error}").into())
}

/// Edits console member B's FHIR endpoint: the edit form shows it with
/// every key, and a save sends it as the operator changed it.
async fn edit_fhir_endpoint(
    browser: &Browser,
    stack: &Stack,
    admin: &Server,
) -> Result<(), Box<dyn Error>> {
    hydrated(browser, stack).await?;
    let shown = browser
        .element(By::Id("edit-node-b-fhir_endpoints"))
        .await?
        .value()
        .await?;
    assert_eq!(
        Some(
            "node-b-fhir https://cdr-b.example.org/fhir system=urn:oid:2.999.1 categories=Patient-Summaries profiles=http://example.org/ps"
        ),
        shown.as_deref(),
        "the edit form shows the held FHIR endpoint"
    );
    fill(
        browser,
        "edit-node-b-fhir_endpoints",
        "node-b-fhir https://fhir.example.org/r4 system=urn:oid:2.999.1 categories=Patient-Summaries profiles=http://example.org/ps",
    )
    .await?;
    fill(browser, "edit-node-b-reason", "move B's FHIR endpoint").await?;
    browser
        .element(By::Id("edit-node-b-confirm"))
        .await?
        .click()
        .await?;
    browser
        .element(submit("Edit console member node-b"))
        .await?
        .click()
        .await?;
    browser
        .text("strong", "Replica gateway-0 saved revision 3.")
        .await?;
    let saved = admin
        .received_requests()
        .await
        .ok_or("the stub records requests")?
        .into_iter()
        .filter(|request| request.url.path() == "/admin/config/members/node-b")
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect::<Vec<_>>();
    let [saved] = saved.as_slice() else {
        return Err(format!("one save of member B, not {}", saved.len()).into());
    };
    assert!(
        saved.contains(r#""url":"https://fhir.example.org/r4","connection_type":"hl7-fhir-rest""#),
        "{saved}"
    );
    browser.console_clean("an edited FHIR endpoint").await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_console_member_is_saved_previewed_and_applied() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;

        // Before the bundle runs, no form can be sent: the server sends the
        // forms inside a disabled fieldset, and a browser that runs no
        // script never finds a submit enabled.
        let session = browser
            .cookie(stack.cookie())
            .await?
            .ok_or("the session cookie")?;
        let page = reqwest::Client::new()
            .get(stack.url("/configuration"))
            .header("cookie", format!("{}={}", session.name, session.value))
            .send()
            .await?
            .text()
            .await?;
        assert!(
            page.contains("<fieldset disabled"),
            "the page the server sends: {page}"
        );
        browser.scripts(false).await?;
        browser.goto(&stack.url("/configuration")).await?;
        browser
            .titled(&format!("Configuration · {PRODUCT}"))
            .await?;
        for button in browser
            .driver()
            .find_all(By::XPath("//fieldset//button"))
            .await?
        {
            assert!(!button.is_enabled().await?, "a submit before hydration");
        }
        browser.scripts(true).await?;
        hydrated(browser, &stack).await?;
        browser.text("td", "file (read-only)").await?;
        browser
            .text("caption", "What revision 2 changes, by key")
            .await?;
        browser
            .console_clean("the hydration of the configuration page")
            .await?;

        // A save is answered with the revision saved and what it changes,
        // and is not applied.
        fill(browser, "add-member-organisation", "org-c").await?;
        fill(browser, "add-member-node", "node-c").await?;
        fill(browser, "add-member-system_id", "cdr-c.example.org").await?;
        fill(
            browser,
            "add-member-endpoints",
            "node-c-pub https://cdr-c.example.org/openehr",
        )
        .await?;
        fill(browser, "add-member-reason", "add C").await?;
        browser
            .element(By::Id("add-member-confirm"))
            .await?
            .click()
            .await?;
        browser
            .element(submit("Add a console member"))
            .await?
            .click()
            .await?;
        browser
            .text("strong", "Replica gateway-0 saved revision 3.")
            .await?;
        browser
            .text("caption", "What revision 3 changes, by key")
            .await?;
        browser.console_clean("a saved revision").await?;

        edit_fhir_endpoint(browser, &stack, &admin).await?;

        // The apply of the newest revision the page offers.
        hydrated(browser, &stack).await?;
        browser
            .element(By::Id("apply-confirm"))
            .await?
            .click()
            .await?;
        browser
            .element(submit("Apply revision 2"))
            .await?
            .click()
            .await?;
        browser
            .text("strong", "Replica gateway-0 applied revision 2.")
            .await?;
        browser.console_clean("an applied revision").await
    })
    .await
}
