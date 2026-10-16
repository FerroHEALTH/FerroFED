// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shell around every page: the theme the server draws the page in and
//! the server function that keeps it, the top bar's operator and gateway
//! status, the navigation's marked page, and the About page's gateway.

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_NAME, console, get, get_as, header, send, signed_in};
use crate::views::options_example;

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

/// A `GET` of `path` carrying the session `session` and the theme cookie
/// `theme`.
fn get_in(
    path: &str,
    session: &ferrofed_viewer::session::SessionId,
    theme: &str,
) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::get(path)
        .header(
            "cookie",
            format!(
                "{}={}; {}={theme}",
                ferrofed_viewer::session::COOKIE,
                session.as_str(),
                ferrofed_viewer::session::THEME_COOKIE
            ),
        )
        .body(Body::empty())?)
}

#[tokio::test]
async fn the_page_arrives_in_the_theme_its_cookie_names() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/", &session)?).await?;
    assert!(
        body.contains(r#"<html lang="en" data-theme="light">"#),
        "{body}"
    );
    for (cookie, drawn) in [
        ("dark", "dark"),
        ("light", "light"),
        ("system", "system"),
        ("sepia", "light"),
    ] {
        let (_response, body) = send(&service, get_in("/members", &session, cookie)?).await?;
        assert!(
            body.contains(&format!(r#"<html lang="en" data-theme="{drawn}">"#)),
            "{cookie}: {body}"
        );
    }
    // The switch marks the theme in force, by a word as well as its look.
    let (_response, body) = send(&service, get_in("/", &session, "dark")?).await?;
    assert!(body.contains(r#"aria-label="Theme""#), "{body}");
    assert!(body.contains(r#"value="dark""#), "{body}");
    Ok(())
}

/// A `POST` of the theme server function with `form`, as the page sends it.
fn keep(
    form: &str,
    session: &ferrofed_viewer::session::SessionId,
) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post("/api/theme")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header(
            "cookie",
            format!("{}={}", ferrofed_viewer::session::COOKIE, session.as_str()),
        )
        .body(Body::from(form.to_owned()))?)
}

#[tokio::test]
async fn the_theme_server_function_keeps_a_theme_in_its_cookie_and_refuses_any_other_key()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (response, body) = send(&service, keep("theme=dark", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("Shown"), "{body}");
    let cookie = header(&response, "set-cookie");
    assert!(
        cookie.starts_with("ferrofed_viewer_theme=dark;"),
        "{cookie}"
    );
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(cookie.contains("SameSite=Lax"), "{cookie}");
    assert_eq!("no-store", header(&response, "cache-control"));

    let (response, body) = send(&service, keep("theme=sepia", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("Refused"), "{body}");
    assert!(body.contains("Invalid"), "{body}");
    assert_eq!("", header(&response, "set-cookie"));
    Ok(())
}

// Before the page has hydrated, a press of the switch is a plain form post,
// which the server function answers by keeping the theme and sending the
// browser back to the console.
#[tokio::test]
async fn a_plain_form_post_of_the_switch_keeps_the_theme_and_returns_to_its_own_page()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let post = |form: &str, accept: &str| -> Result<Request<Body>, Box<dyn Error>> {
        Ok(Request::post("/api/theme")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("accept", accept)
            .header("sec-fetch-site", "same-origin")
            .header(
                "cookie",
                format!("{}={}", ferrofed_viewer::session::COOKIE, session.as_str()),
            )
            .body(Body::from(form.to_owned()))?)
    };
    // The page the form names, when it is one of the console's own.
    for (form, location) in [
        ("theme=dark&back=%2Fmembers", "/members"),
        ("theme=dark&back=%2Fconfiguration", "/configuration"),
        ("theme=light", "/"),
        // Never an open redirect: anything but the console's own page is `/`.
        ("theme=dark&back=https%3A%2F%2Fattacker.example.net%2F", "/"),
        ("theme=dark&back=%2F%2Fattacker.example.net%2F", "/"),
        ("theme=dark&back=%2Fno%2Fsuch%2Fpage", "/"),
    ] {
        let (response, body) = send(&service, post(form, "text/html")?).await?;
        assert!(
            response.status().is_redirection(),
            "{form}: {response:?} {body}"
        );
        assert_eq!(location, header(&response, "location"), "{form}");
        assert!(
            header(&response, "set-cookie").starts_with("ferrofed_viewer_theme="),
            "{form}: {response:?}"
        );
    }
    // The hydrated page calls the function for its answer, never a redirect.
    let (response, body) = send(
        &service,
        post("theme=dark&back=%2Fmembers", "application/json")?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert_eq!("", header(&response, "location"));
    Ok(())
}

#[tokio::test]
async fn the_top_bar_names_the_operator_and_the_gateway_that_answered() -> Result<(), Box<dyn Error>>
{
    let up = gateway(200, options_example()?).await;
    let (state, service) = console(&configured(&up.uri()))?;
    let session = state.sessions().establish(signed_in())?;
    let (response, body) = send(&service, get_as("/", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains(OPERATOR_NAME), "{body}");
    assert!(body.contains(r#"popovertarget="operator-menu""#), "{body}");
    assert!(body.contains("Gateway UP"), "{body}");
    assert!(body.contains("· spec 1.0 · rso-zl"), "{body}");
    assert!(body.contains("pill tone-ok"), "{body}");

    let refusing = gateway(
        403,
        String::from(r#"{"message":"no operator scope","code":"insufficient-scope"}"#),
    )
    .await;
    let (state, service) = console(&configured(&refusing.uri()))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/", &session)?).await?;
    assert!(body.contains("Gateway refused 403"), "{body}");
    assert!(body.contains("· insufficient-scope"), "{body}");
    assert!(body.contains("pill tone-danger"), "{body}");

    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/", &session)?).await?;
    assert!(body.contains("Gateway DOWN"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_navigation_marks_the_page_and_groups_its_entries() -> Result<(), Box<dyn Error>> {
    let up = gateway(200, options_example()?).await;
    let (state, service) = console(&configured(&up.uri()))?;
    let session = state.sessions().establish(signed_in())?;
    for (page, link) in [
        ("/", r#"href="/""#),
        ("/federation", r#"href="/federation""#),
    ] {
        let (_response, body) = send(&service, get_as(page, &session)?).await?;
        let nav = body
            .split_once(r#"aria-label="Views""#)
            .and_then(|(_, rest)| rest.split_once("</nav>"))
            .map(|(inside, _)| inside)
            .ok_or("the navigation")?;
        for group in ["Overview", "Federation", "Queries", "Gateway"] {
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
        // The theme switch posts back to this page before the page hydrates.
        assert!(
            body.contains(&format!(
                r#"<input type="hidden" name="back" value="{page}"#
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
    assert!(body.contains("Sign in to see the gateway"), "{body}");

    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/about", &session)?).await?;
    assert!(body.contains(&format!("<code>{base}</code>")), "{body}");
    Ok(())
}

// The server renders the About page's markup, which the page shows as HTML,
// so every value in it is escaped: markup in the text and a quote breaking
// out of an attribute arrive as text.
#[test]
fn the_about_markup_escapes_what_it_names() {
    // Rendering needs a reactive owner, which a request provides.
    leptos::prelude::Owner::new().with(|| {
        let hostile = String::from(r#"x"><script>alert(1)</script><b onmouseover="y"#);
        let html = ferrofed_viewer::about::about(hostile);
        assert!(!html.contains("<script>"), "{html}");
        assert!(!html.contains("<b onmouseover"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        for page in [
            ferrofed_viewer::home::sign_in(),
            ferrofed_viewer::home::dashboard(),
        ] {
            assert!(!page.contains("<script"), "{page}");
        }
    });
}

#[tokio::test]
async fn the_server_rendered_pages_arrive_whole_and_link_back_with_a_full_load()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let (_response, body) = send(&service, get("/")?).await?;
    assert!(body.contains(">Sign in to the console</h1>"), "{body}");
    let session = state.sessions().establish(signed_in())?;
    let (_response, body) = send(&service, get_as("/members", &session)?).await?;
    // The router never takes a link to a page the server renders whole.
    assert!(body.contains(r#"<a href="/" rel="external""#), "{body}");
    assert!(
        body.contains(r#"<a href="/about" rel="external">"#),
        "{body}"
    );
    let (_response, body) = send(&service, get_as("/", &session)?).await?;
    assert!(
        body.contains(">Each member endpoint and its health.</span>"),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_page_the_console_does_not_serve_offers_the_way_back() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(&configured("http://127.0.0.1:9/"))?;
    let session = state.sessions().establish(signed_in())?;
    for request in [get("/no/such/page")?, get_as("/no/such/page", &session)?] {
        let (response, body) = send(&service, request).await?;
        assert_eq!(StatusCode::NOT_FOUND, response.status());
        assert!(
            body.contains("<title>Not found · FerroFED Console</title>"),
            "{body}"
        );
        assert!(body.contains(">Not found</h1>"), "{body}");
        assert!(body.contains("404: the console has no page"), "{body}");
        assert!(body.contains("Back to the console"), "{body}");
    }
    Ok(())
}
