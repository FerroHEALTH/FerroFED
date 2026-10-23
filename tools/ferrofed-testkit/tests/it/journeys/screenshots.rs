// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The book's screenshots of the operator console, taken behind the
//! `FERROFED_SCREENSHOTS` gate, and the guard that every console route has
//! one.
//!
//! With `FERROFED_SCREENSHOTS=1` the capture drives the journeys' stack and
//! browser through every page of [`shots`], in the light and the dark theme,
//! at 1440 CSS pixels wide and the full height of the page, and writes each
//! as `<image>.png` and `<image>-dark.png` under [`IMAGES`]; it takes each
//! again at 390 CSS pixels, a phone, under the ignored [`NARROW_IMAGES`], for
//! the visual review the book does not show. The browser runs
//! through the chromedriver at `FERROFED_WEBDRIVER`, as the journeys do, and
//! the console serves its release site bundle. Without the gate the capture
//! returns at once, so no CI job takes a screenshot.
//!
//! The guard runs in the ordinary suite: every route the console's router
//! declares is shown by a shot, every shot has both images on disk, every
//! image is a shot's, and every light image is shown by a page of the book.
//! No specification governs the screenshots: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use ferrofed_registry::id::NodeId;
use ferrofed_registry::incident::{Detection, Incident};
use ferrofed_registry::operator::ClearedEhrId;
use ferrofed_testkit::mock::Server;
use serde::Deserialize;
use serde_json::json;
use thirtyfour::{By, Cookie};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::journeys::browser::{Browser, until, with_browser};
use crate::journeys::query::{Run, hydrated, submit};
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{PATIENT, Stack, patient_query};

/// The environment variable that opts in to the capture.
const SCREENSHOTS_GATE: &str = "FERROFED_SCREENSHOTS";

/// The book's folder of console screenshots, from the workspace root.
const IMAGES: &str = "website/book/src/operate/img/console";

/// The ignored folder of the phone-width screenshots, from the workspace
/// root.
const NARROW_IMAGES: &str = "target/console-screenshots/narrow";

/// A width a screenshot is taken at.
#[derive(Debug, Clone, Copy)]
enum Width {
    /// 1440 CSS pixels, the book's.
    Wide,
    /// 390 CSS pixels, a phone.
    Narrow,
}

impl Width {
    /// The width in CSS pixels.
    const fn pixels(self) -> u32 {
        match self {
            Self::Wide => 1440,
            Self::Narrow => 390,
        }
    }
}

/// The height of the window a page is laid out in before its full height is
/// measured.
const HEIGHT: u32 = 900;

/// The tallest screenshot, so a page that grows without bound stays a file
/// the book can hold.
const TALLEST: u32 = 8000;

/// The name of the console's theme cookie.
const THEME_COOKIE: &str = "ferrofed_console_theme";

/// One screenshot of a console page.
#[derive(Debug)]
struct Shot {
    /// The image's name, without the theme suffix and the extension.
    image: &'static str,
    /// The route of the console's router the page answers, as `path!` names
    /// it, or `*` for the fallback.
    route: &'static str,
    /// The path the browser loads.
    url: &'static str,
    /// The `XPath` of an element the page shows once it has loaded.
    ready: &'static str,
}

/// The sign-in card a visitor reads.
const SIGN_IN: Shot = Shot {
    image: "sign-in",
    route: "/",
    url: "/",
    ready: "//a[normalize-space(.) = 'Sign in']",
};

/// The pages a signed-in operator loads and reads as they are.
const PAGES: [Shot; 16] = [
    Shot {
        image: "dashboard",
        route: "/",
        url: "/",
        ready: "//h2[contains(., 'Member health')]",
    },
    Shot {
        image: "members",
        route: "/members",
        url: "/members",
        ready: "//caption[contains(., 'Member endpoints')]",
    },
    Shot {
        image: "member",
        route: "/members/:node",
        url: "/members/node-a",
        ready: "//caption[contains(., 'Endpoints of this member')]",
    },
    Shot {
        image: "routing",
        route: "/routing",
        url: "/routing",
        ready: "//caption[contains(., 'routing table')]",
    },
    Shot {
        image: "integrity",
        route: "/integrity",
        url: "/integrity",
        ready: "//h1",
    },
    Shot {
        image: "query",
        route: "/query",
        url: "/query",
        ready: "//form//button[@type = 'submit' and not(@disabled) and normalize-space(.) = 'Run query']",
    },
    Shot {
        image: "stored-queries",
        route: "/stored-queries",
        url: "/stored-queries",
        ready: "//h1",
    },
    Shot {
        image: "stored-query",
        route: "/stored-queries/:name",
        url: "/stored-queries/org.example::journey_001",
        ready: "//div[contains(@class, 'fui-code-panel')]",
    },
    Shot {
        image: "self-description",
        route: "/federation",
        url: "/federation",
        ready: "//dd",
    },
    Shot {
        image: "configuration-members",
        route: "/configuration",
        url: "/configuration",
        ready: "//*[@id = 'tab-members']",
    },
    Shot {
        image: "configuration-credentials",
        route: "/configuration/:tab",
        url: "/configuration/credentials",
        ready: "//*[@id = 'tab-credentials']",
    },
    Shot {
        image: "configuration-stored-queries",
        route: "/configuration/:tab",
        url: "/configuration/stored-queries",
        ready: "//*[@id = 'tab-stored-queries']",
    },
    Shot {
        image: "configuration-history",
        route: "/configuration/:tab",
        url: "/configuration/history",
        ready: "//*[@id = 'tab-history']",
    },
    Shot {
        image: "configuration-import-export",
        route: "/configuration/:tab",
        url: "/configuration/import-export",
        ready: "//*[@id = 'tab-import-export']",
    },
    Shot {
        image: "configuration-member-form",
        route: "/configuration/:tab/:item",
        url: "/configuration/members/new",
        ready: "//button[@type = 'submit' and contains(., 'Save as revision')]",
    },
    Shot {
        image: "about",
        route: "/about",
        url: "/about",
        ready: "//h1",
    },
];

/// The page no route answers.
const NOT_FOUND: Shot = Shot {
    image: "not-found",
    route: "*",
    url: "/no/such/page",
    ready: "//h1",
};

/// A complete answer: node A alone answers.
const QUERY_COMPLETE: Shot = Shot {
    image: "query-complete",
    route: "/query",
    url: "/query",
    ready: "//div[contains(., 'Every node in scope answered.')]",
};

/// An incomplete answer: both nodes, best effort, and node B fails.
const QUERY_PARTIAL: Shot = Shot {
    image: "query-partial",
    route: "/query",
    url: "/query",
    ready: "//div[contains(., 'Not every node in scope answered')]",
};

/// A query the gateway refuses.
const QUERY_REFUSED: Shot = Shot {
    image: "query-refused",
    route: "/query",
    url: "/query",
    ready: "//div[contains(., 'The gateway refused this view: 400')]",
};

/// Every shot the capture takes.
fn shots() -> impl Iterator<Item = &'static Shot> {
    [&SIGN_IN].into_iter().chain(PAGES.iter()).chain([
        &QUERY_COMPLETE,
        &QUERY_PARTIAL,
        &QUERY_REFUSED,
        &NOT_FOUND,
    ])
}

/// The workspace root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The file of `image` in the theme `theme`.
fn file(image: &str, theme: Theme) -> PathBuf {
    root()
        .join(IMAGES)
        .join(format!("{image}{}.png", theme.suffix()))
}

/// A theme of the console.
#[derive(Debug, Clone, Copy)]
enum Theme {
    /// The light theme.
    Light,
    /// The dark theme.
    Dark,
}

impl Theme {
    /// The word the theme cookie holds.
    fn word(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// What the image's name carries for the theme.
    fn suffix(self) -> &'static str {
        match self {
            Self::Light => "",
            Self::Dark => "-dark",
        }
    }
}

/// Revision 1 runs, applied by the file, and revision 2 is saved.
const STANDING: &str = r#"{"ui":"managed","head":2,"current":{"revision":1,"previous":null,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"file"},
    "applied_at":"2026-10-15T08:00:00Z"},"running":1,"in_step":true,"refused":null}"#;

/// Member A the file provides, console member B with a FHIR endpoint, a
/// console credential, and two stored queries.
const RESOURCES: &str = r#"{"running":1,"head":2,"members":[
    {"node":"node-a","organisation":"org-a","system_id":"cdr-a.example.org","endpoints":["node-a-pub"],"owner":"file"},
    {"node":"node-b","organisation":"org-b","system_id":"cdr-b.example.org","endpoints":["node-b-pub","node-b-fhir"],
     "owner":"console","state":"pending",
     "member":{"organisation":{"id":"org-b"},"node":{"id":"node-b","system_id":"cdr-b.example.org"},
       "endpoints":[{"id":"node-b-pub","url":"https://cdr-b.example.org/openehr","connection_type":"openehr-rest-query"},
         {"id":"node-b-fhir","url":"https://cdr-b.example.org/fhir","connection_type":"hl7-fhir-rest",
          "categories":["Patient-Summaries"],"profiles":["http://example.org/ps"],"ehr_id_system":"urn:oid:2.999.1"}]}}],
    "stored_query_registry":"writable","stored_queries":[
      {"name":"org.example::vital_signs","version":"1.0.0","owner":"store"},
      {"name":"org.example::vital_signs","version":"1.1.0","owner":"store"},
      {"name":"org.example::problem_list","version":"2.0.0","owner":"store"}],
    "sections":[{"resource":{"credentials":"node-a-pub"},"owner":"console","state":"running",
      "settings":"{ user = \"gateway\" }","secrets":[{"key":"password","reference":"/var/lib/ferrofed/secrets/a.secret"}]}],
    "bindings":["localization","consent"]}"#;

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

/// The synthetic `ehr_id` the integrity history names, a bare UUID.
const COLLIDING: &str = "3333cccc-3333-4333-8333-333333333333";

/// Gives the gateway, which runs in this process, an integrity history the
/// dashboard and the Integrity view draw with its timestamps: an open
/// `creating_system_id` conflict, and an `ehr_id` collision the operator's
/// clear resolved (#1154).
fn integrity_history() -> Result<(), Box<dyn Error>> {
    Incident::LearnedCreatingSystemConflict {
        creating_system_id: "legacy-shared.example.org".parse()?,
        first: "node-a-pub".parse()?,
        second: "node-b-pub".parse()?,
    }
    .emit();
    let ehr_id = COLLIDING.parse()?;
    Incident::EhrIdCollision {
        ehr_id,
        detection: Detection::AskAll,
        claimants: vec!["node-a-pub".parse()?, "node-b-pub".parse()?],
    }
    .emit();
    let held = [NodeId::new("node-a")?, NodeId::new("node-b")?];
    ClearedEhrId::of(&COLLIDING.parse()?, &held, (0, &BTreeSet::new())).record();
    Ok(())
}

/// A stub admin listener answering every route the pages read.
async fn listener() -> Server {
    let listener = Server::start().await;
    for (route, body) in [
        ("/admin/config", STANDING),
        ("/admin/config/resources", RESOURCES),
        ("/admin/config/revisions", HISTORY),
        ("/admin/config/revisions/1/diff/2", DIFF_2),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(body.as_bytes().to_vec(), "application/json"),
            )
            .mount(&listener)
            .await;
    }
    listener
}

/// The part of the Chrome `DevTools` Protocol's `Page.getLayoutMetrics`
/// answer the capture reads.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayoutMetrics {
    /// The size of the page's content, in CSS pixels.
    css_content_size: ContentSize,
}

/// A size in CSS pixels.
#[derive(Debug, Deserialize)]
struct ContentSize {
    /// The height.
    height: f64,
}

/// Lays the page out in a window `height` CSS pixels tall at `width`.
async fn window(browser: &Browser, width: Width, height: u32) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .cdp()
        .send_raw(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width.pixels(), "height": height, "deviceScaleFactor": 1, "mobile": false }),
        )
        .await?;
    Ok(())
}

/// Where the image of `image` in `theme` at `width` is written: the book's
/// folder for the wide one, the ignored folder for the narrow one.
fn output(image: &str, theme: Theme, width: Width) -> PathBuf {
    match width {
        Width::Wide => file(image, theme),
        Width::Narrow => root()
            .join(NARROW_IMAGES)
            .join(format!("{image}{}.png", theme.suffix())),
    }
}

/// Has the browser report `theme` as the system's colour scheme, so the
/// form controls the browser draws match the page, and ask for reduced
/// motion, so a control that just turned enabled is drawn at rest.
async fn scheme(browser: &Browser, theme: Theme) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .cdp()
        .send_raw(
            "Emulation.setEmulatedMedia",
            json!({ "features": [
                { "name": "prefers-color-scheme", "value": theme.word() },
                { "name": "prefers-reduced-motion", "value": "reduce" }
            ] }),
        )
        .await?;
    Ok(())
}

/// Waits until the page has hydrated: no form section, confirmation button
/// or submit is still disabled for want of the bundle.
async fn settled(browser: &Browser) -> Result<(), Box<dyn Error>> {
    until("the page has hydrated", async || {
        Ok(browser
            .driver()
            .find_all(By::Css(
                "fieldset[disabled], button[popovertarget][disabled], form button[type=submit][disabled]",
            ))
            .await?
            .is_empty())
    })
    .await
}

/// The height of the page's content, in whole CSS pixels from [`HEIGHT`] to
/// [`TALLEST`].
async fn content_height(browser: &Browser) -> Result<u32, Box<dyn Error>> {
    let metrics = browser
        .driver()
        .cdp()
        .send_raw("Page.getLayoutMetrics", json!({}))
        .await?;
    let metrics: LayoutMetrics = serde_json::from_value(metrics)?;
    let content = metrics.css_content_size.height;
    Ok((HEIGHT..=TALLEST)
        .find(|tall| f64::from(*tall) >= content)
        .unwrap_or(TALLEST))
}

/// The part of a Chrome `DevTools` Protocol answer that names a DOM node.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Found {
    /// The node, `0` when none matched.
    node_id: u64,
}

/// The document a `DOM.getDocument` answer names.
#[derive(Debug, Deserialize)]
struct Document {
    /// The document's root node.
    root: Found,
}

/// Scrolls a signed-in page back to its top, where its skip link sits: a
/// query run moves the focus to its answer and scrolls the page there.
async fn to_top(browser: &Browser) -> Result<(), Box<dyn Error>> {
    let cdp = browser.driver().cdp();
    let document: Document =
        serde_json::from_value(cdp.send_raw("DOM.getDocument", json!({})).await?)?;
    let skip: Found = serde_json::from_value(
        cdp.send_raw(
            "DOM.querySelector",
            json!({ "nodeId": document.root.node_id, "selector": "a[href=\"#main\"]" }),
        )
        .await?,
    )?;
    if skip.node_id != 0 {
        cdp.send_raw(
            "DOM.scrollIntoViewIfNeeded",
            json!({ "nodeId": skip.node_id }),
        )
        .await?;
    }
    Ok(())
}

/// Waits for `shot` to show in `theme`, and writes it at the page's full
/// height.
async fn capture(
    browser: &Browser,
    shot: &Shot,
    (theme, width): (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    browser.element(By::XPath(shot.ready)).await?;
    browser
        .element(By::Css(format!(r#"html[data-theme="{}"]"#, theme.word())))
        .await?;
    settled(browser).await?;
    let height = content_height(browser).await?;
    window(browser, width, height).await?;
    to_top(browser).await?;
    let png = browser.driver().screenshot_as_png().await?;
    window(browser, width, HEIGHT).await?;
    std::fs::write(output(shot.image, theme, width), png)?;
    if shot.route != NOT_FOUND.route {
        return browser.console_clean(shot.image).await;
    }
    // The fallback answers 404, which the browser logs as an error: that
    // answer is the page shown, so it is the one entry allowed.
    let errors = browser.console_errors().await?;
    match errors.as_slice() {
        [error] if error.contains(shot.url) && error.contains("404") => Ok(()),
        _ => Err(format!("the page of an unknown path logged {errors:?}, not its one 404").into()),
    }
}

/// Loads `shot` and captures it in `theme`.
async fn visit(
    browser: &Browser,
    stack: &Stack,
    shot: &Shot,
    at: (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url(shot.url)).await?;
    capture(browser, shot, at).await
}

/// Runs the query console's three answers and captures each in `at`.
async fn answers(
    browser: &Browser,
    stack: &Stack,
    at: (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    let aql = patient_query();
    let mut run = Run {
        aql: &aql,
        parameter: ("patient", PATIENT),
        endpoints: &["node-a-pub"],
        partial: false,
    };
    for (shot, endpoints, partial) in [
        (&QUERY_COMPLETE, &["node-a-pub"][..], false),
        (&QUERY_PARTIAL, &[][..], true),
    ] {
        browser.goto(&stack.url(shot.url)).await?;
        hydrated(browser).await?;
        run.endpoints = endpoints;
        run.partial = partial;
        submit(browser, &run).await?;
        capture(browser, shot, at).await?;
    }
    browser.goto(&stack.url(QUERY_REFUSED.url)).await?;
    hydrated(browser).await?;
    let refused = Run {
        aql: "SELECT FROM",
        parameter: ("", ""),
        endpoints: &[],
        partial: false,
    };
    submit(browser, &refused).await?;
    capture(browser, &QUERY_REFUSED, at).await
}

/// Puts the theme cookie for `theme` in the browser, which must be at the
/// console.
async fn choose(browser: &Browser, theme: Theme) -> Result<(), Box<dyn Error>> {
    let mut cookie = Cookie::new(THEME_COOKIE, theme.word());
    cookie.set_path("/");
    browser.driver().add_cookie(cookie).await?;
    scheme(browser, theme).await
}

/// Captures every shot in `at`, signing in and out around the pages.
async fn every_page(
    browser: &Browser,
    stack: &Stack,
    at: (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    browser.driver().delete_all_cookies().await?;
    browser.goto(&stack.url("/")).await?;
    choose(browser, at.0).await?;
    visit(browser, stack, &SIGN_IN, at).await?;
    sign_in(browser, stack).await?;
    for shot in &PAGES {
        visit(browser, stack, shot, at).await?;
    }
    answers(browser, stack, at).await?;
    visit(browser, stack, &NOT_FOUND, at).await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_book_shows_every_console_page_in_light_and_dark() -> Result<(), Box<dyn Error>> {
    if !std::env::var(SCREENSHOTS_GATE).is_ok_and(|value| value == "1") {
        return Ok(());
    }
    std::fs::create_dir_all(root().join(IMAGES))?;
    std::fs::create_dir_all(root().join(NARROW_IMAGES))?;
    let admin = listener().await;
    let stack = Stack::start_with_admin(&admin.uri()).await?;
    integrity_history()?;
    with_browser(async |browser| {
        for width in [Width::Wide, Width::Narrow] {
            window(browser, width, HEIGHT).await?;
            for theme in [Theme::Light, Theme::Dark] {
                every_page(browser, &stack, (theme, width)).await?;
            }
        }
        Ok(())
    })
    .await
}

/// The routes the console's router declares, read from its source.
fn routes() -> Result<BTreeSet<String>, Box<dyn Error>> {
    let source = std::fs::read_to_string(root().join("app/ferrofed-console/src/app.rs"))?;
    let routes: BTreeSet<String> = source
        .split("path!(\"")
        .skip(1)
        .filter_map(|rest| rest.split_once('"').map(|(route, _)| route.to_owned()))
        .collect();
    if routes.is_empty() {
        return Err("the console's router declares no path!(…) route".into());
    }
    Ok(routes)
}

/// The text of every page of the book.
fn book() -> Result<String, Box<dyn Error>> {
    let mut text = String::new();
    let mut folders = vec![root().join("website/book/src")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let entry = entry?.path();
            if entry.is_dir() {
                folders.push(entry);
            } else if entry.extension().is_some_and(|extension| extension == "md") {
                text.push_str(&std::fs::read_to_string(&entry)?);
            }
        }
    }
    Ok(text)
}

#[test]
fn every_console_route_has_a_screenshot_in_light_and_dark() -> Result<(), Box<dyn Error>> {
    let shown: BTreeSet<&str> = shots().map(|shot| shot.route).collect();
    for route in routes()? {
        assert!(
            shown.contains(route.as_str()),
            "the console route {route} has no screenshot: add a shot for it to journeys::screenshots and run the capture"
        );
    }
    let mut images = BTreeSet::new();
    for shot in shots() {
        for theme in [Theme::Light, Theme::Dark] {
            let image = file(shot.image, theme);
            assert!(
                image.is_file(),
                "{} is missing: run the capture with {SCREENSHOTS_GATE}=1",
                image.display()
            );
            images.insert(image);
        }
    }
    for entry in std::fs::read_dir(root().join(IMAGES))? {
        let entry = root().join(IMAGES).join(entry?.file_name());
        assert!(
            images.contains(&entry),
            "{} is no shot's image: remove it",
            entry.display()
        );
    }
    let book = book()?;
    for shot in shots() {
        let light = format!("img/console/{}.png", shot.image);
        assert!(book.contains(&light), "no page of the book shows {light}");
    }
    Ok(())
}
