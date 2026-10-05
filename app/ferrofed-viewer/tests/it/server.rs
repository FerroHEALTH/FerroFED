// SPDX-FileCopyrightText: Cadasto B.V.
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

/// A site root holding a synthetic bundle, and the console that serves it.
fn bundled() -> Result<(tempfile::TempDir, axum::Router, Vec<u8>), Box<dyn Error>> {
    let site = tempfile::tempdir()?;
    let pkg = site.path().join("pkg");
    std::fs::create_dir_all(&pkg)?;
    let wasm: Vec<u8> = b"\0asm\x01\0\0\0"
        .iter()
        .copied()
        .chain((0..65_536_u32).map(|n| u8::try_from(n % 61).unwrap_or(0)))
        .collect();
    std::fs::write(pkg.join("ferrofed-viewer.wasm"), &wasm)?;
    std::fs::write(
        pkg.join("ferrofed-viewer.js"),
        "export function hydrate() {}\n".repeat(64),
    )?;
    let root = site.path().to_string_lossy().replace('\\', "/");
    let (_state, service) = console(&format!("[server]\nsite_root = \"{root}\"\n"))?;
    Ok((site, service, wasm))
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

#[tokio::test]
async fn a_brotli_request_gets_the_bundle_brotli_compressed() -> Result<(), Box<dyn Error>> {
    let (_site, service, wasm) = bundled()?;
    let (response, body) = fetch(&service, "/pkg/ferrofed-viewer.wasm", "gzip, br").await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!("br", header(&response, "content-encoding"));
    assert!(
        header(&response, "vary")
            .to_ascii_lowercase()
            .contains("accept-encoding"),
        "{response:?}"
    );
    assert_eq!("application/wasm", header(&response, "content-type"));
    assert!(
        body.len().saturating_mul(4) < wasm.len(),
        "{} bytes",
        body.len()
    );
    assert_ne!(Some(&b"\0asm"[..]), body.get(..4));
    Ok(())
}

#[tokio::test]
async fn a_gzip_request_gets_the_bundle_gzip_compressed() -> Result<(), Box<dyn Error>> {
    let (_site, service, wasm) = bundled()?;
    for path in ["/pkg/ferrofed-viewer.wasm", "/pkg/ferrofed-viewer.js"] {
        let (response, body) = fetch(&service, path, "gzip").await?;
        assert_eq!("gzip", header(&response, "content-encoding"), "{path}");
        // RFC 1952 §2.3.1: every gzip member opens with ID1 ID2.
        assert_eq!(Some(&[0x1f_u8, 0x8b][..]), body.get(..2), "{path}");
        assert!(body.len() < wasm.len(), "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_request_that_accepts_no_encoding_gets_the_bundle_as_it_is() -> Result<(), Box<dyn Error>>
{
    let (_site, service, wasm) = bundled()?;
    let (response, body) = fetch(&service, "/pkg/ferrofed-viewer.wasm", "identity").await?;
    assert_eq!("", header(&response, "content-encoding"));
    assert_eq!(wasm, body);
    Ok(())
}

// A document carries what the operator entered beside what an attacker would
// want, so it is never compressed (BREACH).
#[tokio::test]
async fn a_document_is_never_compressed() -> Result<(), Box<dyn Error>> {
    let (_site, service, _wasm) = bundled()?;
    for path in ["/", "/health", "/no/such/page"] {
        let (response, _body) = fetch(&service, path, "gzip, br").await?;
        assert_eq!("", header(&response, "content-encoding"), "{path}");
    }
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
