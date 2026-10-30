// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP surface, driven in-process: the health route, the
//! landing page, a path it does not serve, and the browser security headers
//! on every answer.

use std::error::Error;

use http::StatusCode;

use crate::support::{WITH_OIDC, console, get, get_as, header, send, signed_in};

#[tokio::test]
async fn the_health_route_answers_up() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, body) = send(&service, get("/health")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(r#"{"state":"up"}"#, body);
    Ok(())
}

#[tokio::test]
async fn the_liveness_and_readiness_routes_answer_with_no_session() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    for path in ["/health/liveness", "/health/readiness"] {
        let (response, _body) = send(&service, get(path)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn the_landing_page_names_the_console_and_offers_sign_in() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let (response, body) = send(&service, get("/")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        header(&response, "content-type").starts_with("text/html"),
        "{response:?}"
    );
    assert!(
        body.contains("<title>Sign in · FerroFED Console</title>"),
        "{body}"
    );
    assert!(body.contains(r#"href="/login""#), "{body}");
    assert!(!body.contains("Sign-in is not set up"), "{body}");
    Ok(())
}

// A console with no OpenID Provider says so on its sign-in and answers 503,
// since no operator can sign in to it.
#[tokio::test]
async fn a_console_with_no_provider_says_sign_in_is_not_set_up() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, body) = send(&service, get("/")?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
    assert!(
        body.contains("Sign-in is not set up on this console."),
        "{body}"
    );
    assert!(body.contains("[oidc]"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_navigation_and_the_sign_out_show_only_for_a_signed_in_operator()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(WITH_OIDC)?;
    for path in ["/", "/about"] {
        let (_response, body) = send(&service, get(path)?).await?;
        assert!(!body.contains("<nav"), "{path}: {body}");
        assert!(!body.contains("Sign out"), "{path}: {body}");
        assert!(!body.contains(r#"href="/organisations""#), "{path}: {body}");
    }

    let session = state.sessions().establish(signed_in())?;
    for path in ["/", "/no/such/page", "/about"] {
        let (_response, body) = send(&service, get_as(path, &session)?).await?;
        assert!(
            body.contains(r#"<nav aria-label="Console" class="fui-sidebar-nav">"#),
            "{path}: {body}"
        );
        assert!(body.contains("Sign out"), "{path}: {body}");
        assert!(body.contains(r#"href="/organisations""#), "{path}: {body}");
    }
    let (response, body) = send(&service, get_as("/", &session)?).await?;
    assert!(!body.contains(r#"href="/login""#), "{body}");
    assert_eq!("no-store", header(&response, "cache-control"));

    state.sessions().end(&session)?;
    let (_response, body) = send(&service, get_as("/", &session)?).await?;
    assert!(!body.contains("<nav"), "an ended session: {body}");
    Ok(())
}

/// What the `<footer>` of `body`, the page at `path`, holds.
fn footer<'a>(path: &str, body: &'a str) -> Result<&'a str, String> {
    body.split_once("<footer")
        .and_then(|(_, rest)| rest.split_once("</footer>"))
        .map(|(inside, _)| inside)
        .ok_or_else(|| format!("{path} has no footer: {body}"))
}

// Regulation (EU) 2025/327 Art 30(1)(g): the manufacturer's name, postal
// address and single point of contact, in the system. Every page names the
// manufacturer and links the About page, which names it in full.
#[tokio::test]
async fn every_page_links_the_about_page_which_names_the_manufacturer_in_full()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(WITH_OIDC)?;
    let session = state.sessions().establish(signed_in())?;
    for (path, signed) in [
        ("/", false),
        ("/about", false),
        ("/", true),
        ("/no/such/page", true),
        ("/about", true),
        ("/organisations", true),
    ] {
        let request = if signed {
            get_as(path, &session)?
        } else {
            get(path)?
        };
        let (_response, body) = send(&service, request).await?;
        let footer = footer(path, &body)?;
        for text in [
            "FerroFED Console",
            env!("CARGO_PKG_VERSION"),
            "Cadasto B.V.",
            r#"<a href="/about">About</a>"#,
        ] {
            assert!(footer.contains(text), "{path} {signed}: {text}: {footer}");
        }
    }
    for request in [get("/about")?, get_as("/about", &session)?] {
        let (response, body) = send(&service, request).await?;
        assert_eq!(StatusCode::OK, response.status(), "{body}");
        assert!(
            body.contains("<title>About · FerroFED Console</title>"),
            "{body}"
        );
        for text in [
            "FerroFED Console",
            env!("CARGO_PKG_VERSION"),
            "Cadasto B.V.",
            "Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands",
            r#"<a href="mailto:info@cadasto.com">info@cadasto.com</a>"#,
            r#"<a href="https://www.cadasto.com/contact/">https://www.cadasto.com/contact/</a>"#,
        ] {
            assert!(body.contains(text), "{text}: {body}");
        }
    }
    Ok(())
}

#[test]
fn the_long_version_names_the_manufacturer() {
    use clap::CommandFactory as _;
    let version = ferrofed_console::cli::Cli::command().render_long_version();
    for text in [
        "Cadasto B.V.",
        "Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands",
        "info@cadasto.com",
    ] {
        assert!(version.contains(text), "{text}: {version}");
    }
}

#[tokio::test]
async fn a_path_the_console_does_not_serve_answers_not_found() -> Result<(), Box<dyn Error>> {
    let (state, service) = console("[session]\nsecure_cookie = false\n")?;
    let session = state.sessions().establish(signed_in())?;
    let (response, body) = send(&service, get_as("/no/such/page", &session)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    assert!(body.contains("/no/such/page"), "{body}");
    let (response, _body) = send(&service, get("/no/such/page")?).await?;
    assert_eq!(
        StatusCode::SEE_OTHER,
        response.status(),
        "a visitor signs in first"
    );
    Ok(())
}

#[tokio::test]
async fn every_answer_carries_the_browser_security_headers() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    for path in ["/", "/health", "/login", "/no/such/page"] {
        let (response, _body) = send(&service, get(path)?).await?;
        assert_eq!(
            "nosniff",
            header(&response, "x-content-type-options"),
            "{path}"
        );
        assert_eq!("DENY", header(&response, "x-frame-options"), "{path}");
        assert_eq!(
            "no-referrer",
            header(&response, "referrer-policy"),
            "{path}"
        );
        assert_eq!("no-store", header(&response, "cache-control"), "{path}");
        let policy = header(&response, "content-security-policy");
        assert!(
            policy.contains("frame-ancestors 'none'"),
            "{path}: {policy}"
        );
        assert!(policy.contains("object-src 'none'"), "{path}: {policy}");
    }
    Ok(())
}

// Every page is drawn by the server and every script is a module the console
// serves itself, so the policy admits scripts and stylesheets of the console's
// own origin alone, with no nonce, no inline script and no WebAssembly.
#[tokio::test]
async fn the_policy_admits_the_consoles_own_scripts_and_styles_alone() -> Result<(), Box<dyn Error>>
{
    let (_state, service) = console("")?;
    let (response, body) = send(&service, get("/")?).await?;
    let policy = header(&response, "content-security-policy");
    assert!(policy.contains("script-src 'self';"), "{policy}");
    assert!(policy.contains("style-src 'self';"), "{policy}");
    for refused in ["'nonce-", "'wasm-unsafe-eval'", "'unsafe-inline'", "'unsafe-eval'"] {
        assert!(!policy.contains(refused), "{refused}: {policy}");
    }
    let loader = ferrohealth_ui::serve::Assets::of(ferrohealth_ui::style::Brand::Fed).loader();
    assert!(
        body.contains(&format!(r#"<script type="module" src="{loader}"></script>"#)),
        "{loader}: {body}"
    );
    assert_eq!(
        1,
        body.matches("<script").count(),
        "the loader is the one script: {body}"
    );
    assert!(!body.contains(" style="), "no inline style: {body}");
    let (second, _body) = send(&service, get("/")?).await?;
    assert_eq!(policy, header(&second, "content-security-policy"));
    Ok(())
}

/// A `GET` of `path` accepting `encoding`, answered whole as bytes.
async fn fetch(
    service: &axum::Router,
    path: &str,
    encoding: &str,
) -> Result<(http::Response<()>, Vec<u8>), Box<dyn Error>> {
    let request = http::Request::get(path)
        .header("accept-encoding", encoding)
        .body(axum::body::Body::empty())?;
    let response = tower::ServiceExt::oneshot(service.clone(), request).await?;
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;
    Ok((http::Response::from_parts(parts, ()), bytes.to_vec()))
}

// FerroUI's script modules are served as the crate ships them, under the
// path of its version with a year's immutable cache, to a visitor too, so
// the sign-in works before a session exists.
#[tokio::test]
async fn ferroui_s_modules_are_served_as_the_crate_ships_them() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let base = ferrohealth_ui::serve::Assets::of(ferrohealth_ui::style::Brand::Fed).base;
    for module in ferrohealth_ui::script::MODULES {
        let path = format!("{base}/scripts/{}", module.file);
        let (response, body) = fetch(&service, &path, "gzip, br").await?;
        assert_eq!(StatusCode::OK, response.status(), "{path}");
        assert!(
            header(&response, "content-type").starts_with("text/javascript"),
            "{path}"
        );
        assert!(
            header(&response, "cache-control").contains("immutable"),
            "{path}"
        );
        assert_eq!(module.text.as_bytes(), body.as_slice(), "{path}");
    }
    let (response, _body) = fetch(&service, "/ferrohealth-ui/0.0.0/scripts/fui.js", "").await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

// A document carries what the operator entered beside what an attacker would
// want, so it is never compressed (BREACH); nothing the console serves is.
#[tokio::test]
async fn a_document_is_never_compressed() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let loader = ferrohealth_ui::serve::Assets::of(ferrohealth_ui::style::Brand::Fed).loader();
    for path in ["/", "/health", "/no/such/page", loader.as_str()] {
        let (response, _body) = fetch(&service, path, "gzip, br").await?;
        assert_eq!("", header(&response, "content-encoding"), "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn the_page_loads_ferroui_s_assets_from_their_versioned_path() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (_response, body) = send(&service, get("/")?).await?;
    let assets = ferrohealth_ui::serve::Assets::of(ferrohealth_ui::style::Brand::Fed);
    for asset in [assets.stylesheet(), assets.theme(), String::from("/favicon.ico")] {
        assert!(
            body.contains(&format!(r#"href="{asset}""#)),
            "{asset}: {body}"
        );
    }
    assert!(!body.contains("/pkg/"), "no site bundle: {body}");
    Ok(())
}

// The console draws with FerroUI's stylesheet and its FerroFED theme exactly
// as the crate ships them, so FerroUI's own contrast and theme checks hold
// for every page the console serves; the console keeps no stylesheet of its
// own.
#[tokio::test]
async fn the_stylesheets_are_ferroui_s_own() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let assets = ferrohealth_ui::serve::Assets::of(ferrohealth_ui::style::Brand::Fed);
    for (path, shipped) in [
        (assets.stylesheet(), ferrohealth_ui::style::STYLESHEET),
        (assets.theme(), ferrohealth_ui::style::Brand::Fed.theme()),
    ] {
        let (response, body) = send(&service, get(&path)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{path}");
        assert!(
            header(&response, "content-type").starts_with("text/css"),
            "{path}"
        );
        assert_eq!(shipped, body, "{path}");
    }
    assert!(
        !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/style")).exists(),
        "the console keeps no stylesheet of its own"
    );
    Ok(())
}

// Every page reads and every action runs as the operator of the request, so
// a visitor's post or read is sent to sign-in and asks neither the gateway nor
// a replica anything, whatever it posts.
#[tokio::test]
async fn a_post_or_a_read_asks_nothing_for_a_visitor() -> Result<(), Box<dyn Error>> {
    let gateway = ferrofed_testkit::mock::Server::start().await;
    let (_state, service) = console(&format!(
        "[gateway]\nbase_url = \"{uri}\"\n\n[session]\nsecure_cookie = false\n\n\
         [[admin.replicas]]\nname = \"gateway-0\"\nadmin_url = \"{uri}\"\n",
        uri = gateway.uri()
    ))?;
    for (route, body) in [
        ("/query", "kind=aql&aql=SELECT+1"),
        (
            "/configuration",
            "form%5Baction%5D=apply&form%5Brevision%5D=2&form%5Bconfirm%5D=yes",
        ),
        (
            "/integrity",
            "ehr_id=8c3e8f3a-1b2c-4d5e-8f90-123456789abc&replica=*&confirm=yes",
        ),
        (
            "/stored-queries/org.example::weights",
            "version=1.0.0&endpoint=*&confirm=yes",
        ),
    ] {
        let request = http::Request::post(route)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("sec-fetch-site", "same-origin")
            .body(axum::body::Body::from(body))?;
        let (response, _answer) = send(&service, request).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{route}");
        assert_eq!("/login", header(&response, "location"), "{route}");
    }
    for route in ["/organisations", "/configuration", "/search/answer?q=node"] {
        let (response, _answer) = send(&service, get(route)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{route}");
    }
    let (_response, landing) = send(&service, get("/")?).await?;
    assert!(landing.contains("Sign-in is not set up"), "{landing}");
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(asked.is_empty(), "a visitor's call asked {}", asked.len());
    Ok(())
}
