// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shell around every page: the preferences the server draws the page
//! in and the post that keeps each, the top bar's operator and gateway
//! status with no theme toggle of its own, the navigation's marked page,
//! the development banner, and the About page's gateway.

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::options_example;
use crate::support::{OPERATOR_NAME, console, get, get_as, header, send, signed_in};

/// A stub gateway answering `OPTIONS {base}/` with `status` and `body`.
async fn gateway(status: u16, body: String) -> MockServer {
    let gateway = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(status)
                .insert_header("content-type", "application/json")
                .set_body_string(body),
        )
        .mount(&gateway)
        .await;
    gateway
}

/// The configuration of a console in front of the gateway at `base`.
fn configured(base: &str) -> String {
    format!("[gateway]\nbase_url = \"{base}\"\n\n[session]\nsecure_cookie = false\n")
}

/// A `GET` of `path` carrying the session `session` and the extra cookies
/// `cookies`.
fn get_with(
    path: &str,
    session: &ferrofed_console::session::SessionId,
    cookies: &str,
) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::get(path)
        .header(
            "cookie",
            format!(
                "{}={}; {cookies}",
                ferrofed_console::session::COOKIE,
                session.as_str()
            ),
        )
        .body(Body::empty())?)
}

/// A plain form post of `form` to `path`, as the console's own page sends
/// it, carrying the session `session`.
fn post(
    path: &str,
    form: &str,
    session: &ferrofed_console::session::SessionId,
) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("sec-fetch-site", "same-origin")
        .header(
            "cookie",
            format!("{}={}", ferrofed_console::session::COOKIE, session.as_str()),
        )
        .body(Body::from(form.to_owned()))?)
}

#[tokio::test]
async fn the_page_arrives_in_the_preferences_its_cookies_name() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/about", &session)?).await?;
    assert!(body.contains(r#"data-theme="system""#), "{body}");
    for (cookie, drawn) in [
        ("dark", "dark"),
        ("light", "light"),
        ("system", "system"),
        ("sepia", "system"),
    ] {
        let (_response, body) = send(
            &service,
            get_with(
                "/about",
                &session,
                &format!("ferrofed_console_theme={cookie}"),
            )?,
        )
        .await?;
        assert!(
            body.contains(&format!(r#"data-theme="{drawn}""#)),
            "{cookie}: {body}"
        );
    }
    let (_response, body) = send(
        &service,
        get_with(
            "/about",
            &session,
            "ferrofed_console_density=compact; ferrofed_console_motion=reduce",
        )?,
    )
    .await?;
    assert!(body.contains(r#"data-density="compact""#), "{body}");
    assert!(body.contains(r#"data-motion="reduce""#), "{body}");
    Ok(())
}

// The theme lives in the user menu and Your preferences; the top bar
// carries no toggle of its own (FerroUI 0.3.2, the family rule).
#[tokio::test]
async fn the_top_bar_carries_no_theme_toggle_and_the_user_menu_offers_the_theme()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/about", &session)?).await?;
    let top = body
        .split_once(r#"<header class="fui-topbar""#)
        .and_then(|(_, rest)| rest.split_once(r#"id="user-menu""#))
        .map(|(top, _)| top)
        .ok_or("the top bar")?;
    assert!(!top.contains("theme"), "{top}");
    assert!(
        body.contains(r#"<form method="post" action="/preference" class="fui-menu-theme">"#),
        "{body}"
    );
    assert!(body.contains(r#"name="theme" value="dark""#), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_preference_post_keeps_its_value_in_a_cookie_of_its_own_and_refuses_any_other()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    for (form, cookie) in [
        ("theme=dark&back=%2Fabout", "ferrofed_console_theme=dark;"),
        (
            "density=compact&back=%2Fabout",
            "ferrofed_console_density=compact;",
        ),
        (
            "motion=reduce&back=%2Fabout",
            "ferrofed_console_motion=reduce;",
        ),
    ] {
        let (response, _body) = send(&service, post("/preference", form, &session)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{form}");
        let set = header(&response, "set-cookie");
        assert!(set.starts_with(cookie), "{form}: {set}");
        assert!(set.contains("HttpOnly"), "{set}");
        assert!(set.contains("SameSite=Lax"), "{set}");
    }
    for refused in [
        "theme=sepia&back=%2F",
        "colour=red&back=%2F",
        "theme=dark&density=compact&back=%2F",
    ] {
        let (response, _body) = send(&service, post("/preference", refused, &session)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, response.status(), "{refused}");
        assert_eq!("", header(&response, "set-cookie"), "{refused}");
    }
    Ok(())
}

// A preference post sends the browser back to the console's own page it came
// from, and never anywhere else.
#[tokio::test]
async fn a_preference_post_returns_to_its_own_page_and_never_off_the_console()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    for (form, location) in [
        ("theme=dark&back=%2Forganisations", "/organisations"),
        ("theme=dark&back=%2Fconfiguration", "/configuration"),
        ("theme=light", "/"),
        ("theme=dark&back=https%3A%2F%2Fattacker.example.net%2F", "/"),
        ("theme=dark&back=%2F%2Fattacker.example.net%2F", "/"),
        ("theme=dark&back=%2F..%2Fx", "/"),
        ("theme=dark&back=%2Fpreferences", "/preferences?saved=theme"),
    ] {
        let (response, body) = send(&service, post("/preference", form, &session)?).await?;
        assert!(
            response.status().is_redirection(),
            "{form}: {response:?} {body}"
        );
        assert_eq!(location, header(&response, "location"), "{form}");
    }
    let (response, _body) = send(
        &service,
        post("/preferences/reset", "back=%2Fabout", &session)?,
    )
    .await?;
    assert_eq!("/about", header(&response, "location"));
    let cleared: Vec<&str> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect();
    assert!(
        cleared
            .iter()
            .any(|cookie| cookie.starts_with("ferrofed_console_theme=;")),
        "{cleared:?}"
    );
    assert!(
        cleared.iter().all(|cookie| cookie.contains("Max-Age=0")),
        "{cleared:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_top_bar_names_the_operator_and_the_gateway_that_answered() -> Result<(), Box<dyn Error>>
{
    let up = gateway(200, options_example()?).await;
    let (state, service) = console(&configured(&up.uri()))?;
    let session = state.sessions().establish(signed_in())?;
    let (response, body) = send(&service, get_as("/about", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains(OPERATOR_NAME), "{body}");
    assert!(body.contains(r#"popovertarget="user-menu""#), "{body}");
    assert!(
        body.contains(r#"class="fui-pill fui-tone-ok fui-topbar-status""#),
        "{body}"
    );

    let refusing = gateway(
        403,
        String::from(r#"{"message":"no operator scope","code":"insufficient-scope"}"#),
    )
    .await;
    let (state, service) = console(&configured(&refusing.uri()))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/about", &session)?).await?;
    assert!(!body.contains("fui-tone-ok fui-topbar-status"), "{body}");

    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/about", &session)?).await?;
    assert!(body.contains("fui-tone-bad fui-topbar-status"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_navigation_marks_the_page_and_groups_its_entries() -> Result<(), Box<dyn Error>> {
    let up = gateway(200, options_example()?).await;
    let (state, service) = console(&configured(&up.uri()))?;
    let session = state.sessions().establish(signed_in())?;
    for (page, link) in [
        ("/", r#"href="/""#),
        ("/self-description", r#"href="/self-description""#),
        ("/self-description/limits", r#"href="/self-description""#),
        ("/stored-queries", r#"href="/stored-queries""#),
    ] {
        let (_response, body) = send(&service, get_as(page, &session)?).await?;
        let nav = body
            .split_once(r#"<nav aria-label="Console" class="fui-sidebar-nav">"#)
            .and_then(|(_, rest)| rest.split_once("</nav>"))
            .map(|(inside, _)| inside)
            .ok_or("the navigation")?;
        for group in ["Overview", "Federation", "Queries", "Gateway", "Console"] {
            assert!(nav.contains(&format!(">{group}</p>")), "{group}: {nav}");
        }
        let marked: Vec<&str> = nav
            .split("<a ")
            .filter(|anchor| anchor.contains(r#"aria-current="page""#))
            .collect();
        let [only] = marked.as_slice() else {
            return Err(format!("{page}: one entry marked, not {marked:?}").into());
        };
        assert!(only.contains(link), "{page}: {only}");
        assert!(
            body.contains(&format!(
                r#"<input type="hidden" name="back" value="{page}">"#
            )),
            "{page}: {body}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_about_page_shows_the_gateway_to_a_signed_in_operator_alone()
-> Result<(), Box<dyn Error>> {
    let base = "https://gateway.example.org/ferrofed/";
    let (state, service) = console(&configured(base))?;
    let (_response, body) = send(&service, get("/about")?).await?;
    assert!(!body.contains(base), "{body}");
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/about", &session)?).await?;
    assert!(body.contains(base), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_page_the_console_does_not_serve_offers_the_way_back() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (response, body) = send(&service, get_as("/no/such/page", &session)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    assert!(body.contains("<title>"), "{body}");
    assert!(body.contains("/no/such/page"), "{body}");
    assert!(body.contains(r#"href="/""#), "a way back: {body}");
    Ok(())
}

/// A stub gateway that answers its self-description and
/// `GET {base}/operator/deployment` with `status` and `deployment`.
async fn deployed(status: u16, deployment: &str) -> Result<MockServer, Box<dyn Error>> {
    let up = gateway(200, options_example()?).await;
    Mock::given(method("GET"))
        .and(path("/operator/deployment"))
        .respond_with(
            ResponseTemplate::new(status)
                .insert_header("content-type", "application/json")
                .set_body_string(deployment.to_owned()),
        )
        .mount(&up)
        .await;
    Ok(up)
}

/// The top of the page at `/about` over `gateway`: the top bar and what
/// sits under it, before the navigation.
async fn top_of(gateway: &MockServer) -> Result<String, Box<dyn Error>> {
    let (state, service) = console(&configured(&gateway.uri()))?;
    let session = state.sessions().establish(signed_in())?;
    let (response, body) = send(&service, get_as("/about", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    body.split_once(r#"<nav aria-label="Console""#)
        .map(|(top, _)| top.to_owned())
        .ok_or_else(|| "the navigation".into())
}

// A development gateway is named in a banner under the top bar, and the pill
// names the product version and the replica.
#[tokio::test]
async fn a_development_gateway_is_named_in_a_banner_and_its_version_in_the_pill()
-> Result<(), Box<dyn Error>> {
    let development = deployed(
        200,
        r#"{"mode":"development","product":"FerroFED","version":"0.0.11","replica":"gateway-0"}"#,
    )
    .await?;
    let top = top_of(&development).await?;
    assert!(top.contains("Development deployment"), "{top}");
    assert!(
        top.contains("FerroFED 0.0.11 · answered by replica gateway-0"),
        "{top}"
    );
    let bar = top.find("fui-topbar-status").ok_or("the pill")?;
    let banner = top.find("Development deployment").ok_or("the banner")?;
    assert!(bar < banner, "the banner sits under the top bar: {top}");

    let plain = deployed(
        200,
        r#"{"mode":"development","product":"FerroFED","version":"0.0.11","replica":"unnamed"}"#,
    )
    .await?;
    let top = top_of(&plain).await?;
    assert!(top.contains("Development deployment"), "{top}");
    assert!(
        !top.contains("answered by replica"),
        "an unnamed replica is not named: {top}"
    );
    Ok(())
}

// A production gateway has no banner, and an operator the deployment is
// refused to reads the pill as before, with no banner and no error.
#[tokio::test]
async fn a_production_gateway_or_a_refused_deployment_shows_no_banner() -> Result<(), Box<dyn Error>>
{
    let production = deployed(
        200,
        r#"{"mode":"production","product":"FerroFED","version":"0.0.11","replica":"gateway-0"}"#,
    )
    .await?;
    let top = top_of(&production).await?;
    assert!(!top.contains("Development deployment"), "{top}");
    assert!(top.contains("answered by replica gateway-0"), "{top}");

    let refused = deployed(
        403,
        r#"{"message":"no operator scope","code":"scope-insufficient"}"#,
    )
    .await?;
    let top = top_of(&refused).await?;
    assert!(!top.contains("Development deployment"), "{top}");
    assert!(!top.contains(r#"role="alert""#), "{top}");
    Ok(())
}

#[tokio::test]
async fn the_find_answers_the_panel_as_json_and_says_when_a_read_failed()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (response, body) = send(&service, get_as("/search/answer?q=node-b", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    let answer: ferrohealth_ui::view::search::SearchAnswer = serde_json::from_str(&body)?;
    assert_eq!("node-b", answer.query, "{body}");
    assert!(
        answer.unfinished.is_some_and(|why| !why.is_empty()),
        "a gateway that does not answer is said, never an empty answer: {body}"
    );
    let (response, _body) = send(&service, get("/search/answer?q=node-b")?).await?;
    assert_ne!(
        StatusCode::OK,
        response.status(),
        "no answer without a session"
    );
    Ok(())
}

#[tokio::test]
async fn the_top_bar_find_asks_the_answer_route_and_submits_to_the_find_page()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/search?q=node-b", &session)?).await?;
    assert!(body.contains(r#"action="/search""#), "{body}");
    assert!(
        body.contains("&quot;suggest&quot;:&quot;/search/answer&quot;"),
        "{body}"
    );
    let main = body.split("<main").nth(1).unwrap_or_default();
    assert!(
        !main.contains("fui-search-results"),
        "never an overlay in the page: {body}"
    );
    Ok(())
}
