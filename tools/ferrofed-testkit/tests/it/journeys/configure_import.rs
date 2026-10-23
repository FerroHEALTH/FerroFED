// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The import of a configuration as TOML in the browser, against a stub
//! admin listener: each revision of the history offers its download, the
//! running one from the Import and export tab, and TOML pasted into the
//! import form is saved as a dry-run revision whose changes the page shows,
//! without a console error.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::mock::Server;
use thirtyfour::By;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::journeys::browser::with_browser;
use crate::journeys::configure::{fill, hydrated, submit};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// Revision 2 runs, and none waits.
const STANDING: &str = r#"{"ui":"managed","head":2,"current":{"revision":2,"previous":1,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"file"},
    "applied_at":"2026-10-15T09:00:00Z"},"running":2,"in_step":true,"refused":null}"#;

/// No member, stored query or section.
const RESOURCES: &str = r#"{"running":2,"head":2,"members":[],"stored_query_registry":"absent",
    "stored_queries":[]}"#;

/// Revision 2, by the file.
const HISTORY: &str = r#"{"items":[{"id":2,"base":1,"origin":"file","restores":null,
    "author":{"kind":"file"},"at":"2026-10-15T09:00:00Z",
    "reason":"the configuration file was loaded","toml":""}],"offset":0,"total":1}"#;

/// Revision 3, the import, and what it changes against revision 2.
const IMPORTED: &str = r#"{"revision":{"id":3,"base":2,"origin":"import","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:01:00Z","reason":"from git","toml":""},
    "diff":{"from":2,"to":3,"changes":[
      {"key":"federation.best_effort","change":"changed","from":"false","to":"true"}]}}"#;

/// A stub admin listener answering every route the page takes.
async fn listener() -> Server {
    let listener = Server::start().await;
    for (verb, route, status, body) in [
        ("GET", "/admin/config", 200, STANDING),
        ("GET", "/admin/config/resources", 200, RESOURCES),
        ("GET", "/admin/config/revisions", 200, HISTORY),
        ("POST", "/admin/config/import", 201, IMPORTED),
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

#[tokio::test(flavor = "multi_thread")]
async fn toml_is_imported_as_a_dry_run_revision_and_its_changes_shown() -> Result<(), Box<dyn Error>>
{
    if !journeys_enabled() {
        return Ok(());
    }
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        hydrated(browser, &stack, "/configuration/history").await?;
        let link = browser
            .element(By::PartialLinkText("Export revision 2 as TOML"))
            .await?;
        assert_eq!(
            Some(String::from("/configuration/revisions/2/export")),
            link.attr("href").await?
        );
        hydrated(browser, &stack, "/configuration/import-export").await?;
        let download = browser
            .element(By::PartialLinkText("Download as TOML"))
            .await?;
        assert_eq!(
            Some(String::from("/configuration/revisions/2/export")),
            download.attr("href").await?
        );
        browser
            .console_clean("the hydration of the import form")
            .await?;

        fill(browser, "cfg-toml", "[federation]\nbest_effort = true\n").await?;
        fill(browser, "cfg-reason", "from git").await?;
        let import = browser
            .element(submit("Import as a dry-run revision"))
            .await?;
        browser.press(&import).await?;
        browser
            .text(
                "strong",
                "Replica gateway-0 imported revision 3 as a dry run.",
            )
            .await?;
        browser.text("code", "federation.best_effort").await?;
        browser.console_clean("an imported revision").await
    })
    .await
}
