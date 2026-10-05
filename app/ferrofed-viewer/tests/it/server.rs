// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP surface, driven in-process: the health route, the
//! landing page, a path it does not serve, and the browser security headers
//! on every answer.

use std::error::Error;

use http::StatusCode;

use crate::support::{console, get, header, send};

#[tokio::test]
async fn the_health_route_answers_up() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, body) = send(&service, get("/health")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(r#"{"status":"up"}"#, body);
    Ok(())
}

#[tokio::test]
async fn the_landing_page_names_the_console_and_offers_sign_in() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, body) = send(&service, get("/")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        header(&response, "content-type").starts_with("text/html"),
        "{response:?}"
    );
    assert!(
        body.contains("<title>FerroFED operator console</title>"),
        "{body}"
    );
    assert!(body.contains("<h1>FerroFED operator console"), "{body}");
    assert!(
        body.contains(r#"<a href="/login" rel="external">"#),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_path_the_console_does_not_serve_answers_not_found() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, body) = send(&service, get("/no/such/page")?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    assert!(body.contains("Not found"), "{body}");
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

#[tokio::test]
async fn the_page_scripts_carry_the_nonce_the_policy_authorizes() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, body) = send(&service, get("/")?).await?;
    let policy = header(&response, "content-security-policy");
    let nonce = policy
        .split_once("'nonce-")
        .and_then(|(_, rest)| rest.split_once('\''))
        .map(|(nonce, _)| nonce)
        .ok_or("the policy names a nonce")?;
    let stamped = format!(r#"<script type="module" nonce="{nonce}">"#);
    assert!(body.contains(&stamped), "{nonce}: {body}");
    let (second, _body) = send(&service, get("/")?).await?;
    assert_ne!(policy, header(&second, "content-security-policy"));
    Ok(())
}

#[tokio::test]
async fn the_page_loads_the_bundle_cargo_leptos_writes() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (_response, body) = send(&service, get("/")?).await?;
    for asset in [
        "/pkg/ferrofed-viewer.wasm",
        "/pkg/ferrofed-viewer.js",
        "/pkg/ferrofed-viewer.css",
    ] {
        assert!(
            body.contains(&format!(r#"href="{asset}""#)),
            "{asset}: {body}"
        );
    }
    Ok(())
}
