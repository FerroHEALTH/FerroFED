// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page in the browser, against a stub admin listener:
//! its tabs as links, a member added with an endpoint row added to its form
//! by a post of the form, a refused field marked where it stands with what
//! the operator typed kept, the save answered with the preview of what the
//! saved revision changes, the edit of a console member's FHIR endpoint,
//! and the apply of the newest revision through its confirmation dialog,
//! each without a console error, and a member added with no script.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_console::app::PRODUCT;
use ferrofed_testkit::mock::Server;
use thirtyfour::{By, Key};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::journeys::browser::{Browser, hydrated as islands, until, with_browser};
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

/// The submit button of the page whose text holds `label`.
pub(crate) fn submit(label: &str) -> By {
    By::XPath(format!(
        "//main//button[@type = 'submit' and contains(., '{label}')]"
    ))
}

/// Replaces the text of the field `id` with `text`, as typing does.
pub(crate) async fn fill(browser: &Browser, id: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let field = browser.element(By::Id(id)).await?;
    browser.press(&field).await?;
    browser
        .driver()
        .execute("arguments[0].select();", vec![field.to_json()?])
        .await?;
    field.send_keys(Key::Backspace).await?;
    field.send_keys(text).await?;
    Ok(())
}

/// Loads the configuration page at `path` and waits until it is drawn and,
/// with scripts on, hydrated.
pub(crate) async fn hydrated(
    browser: &Browser,
    stack: &Stack,
    path: &str,
) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url(path)).await?;
    browser
        .titled(&format!("Configuration · {PRODUCT}"))
        .await?;
    islands(browser).await
}

/// Opens the confirmation dialog `id` from its button, and runs it with its
/// submit.
pub(crate) async fn confirmed(browser: &Browser, id: &str) -> Result<(), Box<dyn Error>> {
    let opens = browser
        .element(By::Css(format!(
            r#"button[commandfor="{id}"][aria-haspopup="dialog"]"#
        )))
        .await?;
    browser.press(&opens).await?;
    browser
        .element(By::Css(format!("dialog#{id}[open]")))
        .await?;
    browser
        .element(By::Css(format!(
            r#"dialog#{id}[open] button[type="submit"]"#
        )))
        .await?
        .click()
        .await?;
    Ok(())
}

/// The bodies `admin` received at `route`.
pub(crate) async fn bodies(admin: &Server, route: &str) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(admin
        .received_requests()
        .await
        .ok_or("the stub records requests")?
        .into_iter()
        .filter(|request| request.url.path() == route)
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect())
}

/// Adds member C, its second Query API endpoint a row the form adds, and
/// first has a refused `node_id` marked at its field.
async fn add_member(browser: &Browser, admin: &Server) -> Result<(), Box<dyn Error>> {
    fill(browser, "cfg-organisation", "org-c").await?;
    fill(browser, "cfg-node", "node c").await?;
    fill(browser, "cfg-system_id", "cdr-c.example.org").await?;
    fill(browser, "cfg-q-0-id", "node-c-pub").await?;
    fill(browser, "cfg-q-0-url", "https://cdr-c.example.org/openehr").await?;
    fill(browser, "cfg-reason", "add C").await?;
    // Add endpoint posts the form, and the page answers with one row more
    // and every field as it was typed.
    let add = browser.element(By::Id("cfg-add-query")).await?;
    browser.press(&add).await?;
    fill(browser, "cfg-q-q1-id", "node-c-old").await?;
    fill(browser, "cfg-q-q1-url", "https://old.example.org/openehr").await?;
    let kept = browser
        .element(By::Id("cfg-q-0-url"))
        .await?
        .value()
        .await?;
    assert_eq!(
        Some("https://cdr-c.example.org/openehr"),
        kept.as_deref(),
        "the typed row is kept"
    );
    let save = browser.element(submit("Save as revision 3")).await?;
    browser.press(&save).await?;
    // The post loads the page again, so the line is found afresh on every
    // poll, never through an element of the page before it.
    until("the refused node_id is marked", async || {
        let lines = browser.driver().find_all(By::Id("cfg-node-error")).await?;
        let Some(line) = lines.first() else {
            return Ok(false);
        };
        Ok(line
            .text()
            .await
            .unwrap_or_default()
            .contains("The node_id is not"))
    })
    .await?;
    assert_eq!(
        Some(String::from("true")),
        browser
            .element(By::Id("cfg-node"))
            .await?
            .attr("aria-invalid")
            .await?,
        "the refused field is marked invalid"
    );
    let reason = browser.element(By::Id("cfg-reason")).await?.value().await?;
    assert_eq!(Some("add C"), reason.as_deref(), "the reason is kept");
    fill(browser, "cfg-node", "node-c").await?;
    let save = browser.element(submit("Save as revision 3")).await?;
    browser.press(&save).await?;
    browser
        .text("strong", "Replica gateway-0 saved revision 3.")
        .await?;
    browser
        .text("caption", "What revision 3 changes, by key")
        .await?;
    let saved = bodies(admin, "/admin/config/members/node-c").await?;
    let [saved] = saved.as_slice() else {
        return Err(format!("one save of member C, not {}", saved.len()).into());
    };
    assert!(saved.contains(r#""id":"node-c-pub""#), "{saved}");
    assert!(saved.contains(r#""id":"node-c-old""#), "{saved}");
    Ok(())
}

/// Edits console member B's FHIR endpoint: the edit form shows its fields,
/// and a save sends it as the operator changed it.
async fn edit_fhir_endpoint(
    browser: &Browser,
    stack: &Stack,
    admin: &Server,
) -> Result<(), Box<dyn Error>> {
    hydrated(browser, stack, "/configuration/members/node-b").await?;
    let shown = browser
        .element(By::Id("cfg-f-0-url"))
        .await?
        .value()
        .await?;
    assert_eq!(
        Some("https://cdr-b.example.org/fhir"),
        shown.as_deref(),
        "the edit form shows the held FHIR endpoint"
    );
    fill(browser, "cfg-f-0-url", "https://fhir.example.org/r4").await?;
    fill(browser, "cfg-reason", "move B's FHIR endpoint").await?;
    let save = browser.element(submit("Save as revision 3")).await?;
    browser.press(&save).await?;
    browser
        .text("strong", "Replica gateway-0 saved revision 3.")
        .await?;
    let saved = bodies(admin, "/admin/config/members/node-b").await?;
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
async fn a_console_member_is_added_edited_and_applied() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;

        // The tabs are links, the one shown marked.
        hydrated(browser, &stack, "/configuration/members").await?;
        browser.text("span", "File, read-only").await?;
        browser
            .text("caption", "What revision 2 changes, by key")
            .await?;
        browser
            .element(By::Css(r#"a[data-tab="tab-members"][aria-current="page"]"#))
            .await?;
        browser
            .element(By::Css(r#"a[data-tab="tab-credentials"]"#))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/configuration/credentials")).await?;
        browser
            .console_clean("the tabs of the configuration page")
            .await?;

        hydrated(browser, &stack, "/configuration/members/new").await?;
        add_member(browser, &admin).await?;
        browser.console_clean("an added member").await?;
        edit_fhir_endpoint(browser, &stack, &admin).await?;

        // The apply of the newest revision, through its confirmation dialog.
        hydrated(browser, &stack, "/configuration/members").await?;
        confirmed(browser, "apply-confirm").await?;
        browser
            .text("strong", "Replica gateway-0 applied revision 2.")
            .await?;
        assert_eq!(
            vec![String::new()],
            bodies(&admin, "/admin/config/revisions/2/apply").await?
        );
        browser.console_clean("an applied revision").await
    })
    .await
}

// The configuration works with no WebAssembly: a row is added and a member
// saved by plain posts of the form.
#[tokio::test(flavor = "multi_thread")]
async fn a_member_is_added_with_no_script() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.scripts(false).await?;
        browser
            .goto(&stack.url("/configuration/members/new"))
            .await?;
        browser
            .titled(&format!("Configuration · {PRODUCT}"))
            .await?;
        add_member(browser, &admin).await?;
        browser.scripts(true).await?;
        Ok(())
    })
    .await
}
