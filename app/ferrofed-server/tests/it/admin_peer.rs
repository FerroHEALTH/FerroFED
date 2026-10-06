// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener's authentication. A write action is admitted by client
//! authentication only for a token that carries the operator scope its
//! issuer's entry names, from any peer; `profile = "development"` admits a
//! loopback peer that presents no credential as well. `GET /metrics` is open
//! until `[metrics] scrape_token` is set, and then answers a scrape carrying
//! that token alone. Every refusal is counted. No specification governs the
//! admin listener: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use ferrofed_identity::dev::Profile;
use ferrofed_registry::secret::Secret;
use ferrofed_server::admin::{self, Access};
use ferrofed_server::metrics::security::Event;
use ferrofed_server::state::AppState;
use http::{HeaderMap, Request, StatusCode, header};
use tokio::net::TcpListener;

use crate::support::{self, bearer, error_body, operator_bearer, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// A remote peer under the documentation range of RFC 5737.
const REMOTE: ([u8; 4], u16) = ([192, 0, 2, 10], 40_000);

/// The operator's distribution of a stored-query version.
const WRITE: &str = "/admin/stored-queries/example::query/1.0.0/distribute";

/// A synthetic scrape token.
const SCRAPE_TOKEN: &str = "synthetic-scrape-token-5f1c";

/// The peers a request may come from: loopback on IPv4 and IPv6, a remote
/// peer, and none recorded.
fn peers() -> [Option<SocketAddr>; 4] {
    [
        Some(SocketAddr::from(([127, 0, 0, 1], 40_000))),
        Some(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 40_000))),
        Some(SocketAddr::from(REMOTE)),
        None,
    ]
}

/// The admin listener's application under `profile`, with `scrape` as its
/// scrape token, as a request from `peer` reaches it.
fn listener(profile: Profile, scrape: Option<&str>, peer: Option<SocketAddr>) -> Router {
    let access = Access::new(&support::auth(), profile, scrape.map(Secret::new));
    let app = admin::router(Arc::new(AppState::default()), access);
    match peer {
        Some(peer) => app.layer(MockConnectInfo(peer)),
        None => app,
    }
}

/// A request to `path` with `method`, carrying `authorization` when given.
fn request(
    method: &http::Method,
    path: &str,
    authorization: Option<&str>,
) -> Result<Request<Body>, http::Error> {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(value) = authorization {
        request = request.header(header::AUTHORIZATION, value);
    }
    request.body(Body::empty())
}

/// A `POST` of [`WRITE`], carrying `authorization` when given.
fn write(authorization: Option<&str>) -> Result<Request<Body>, http::Error> {
    request(&http::Method::POST, WRITE, authorization)
}

/// A `GET /metrics`, carrying `authorization` when given.
fn scrape(authorization: Option<&str>) -> Result<Request<Body>, http::Error> {
    request(&http::Method::GET, "/metrics", authorization)
}

/// Sends `request` to `app` and reads the status, the headers and the body.
async fn answer(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, HeaderMap, String), Box<dyn Error>> {
    let response = send_as_is(app, request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024).await?;
    Ok((status, headers, String::from_utf8(bytes.to_vec())?))
}

/// Whether `status` and `text` are the distribution's own answer: the
/// default state holds no stored query, so an admitted request is `404`.
fn admitted(status: StatusCode, text: &str) -> bool {
    status == StatusCode::NOT_FOUND
        && error_body(text).is_ok_and(|body| body.code == "stored-query-unknown")
}

/// Whether `status`, `headers` and `text` are a `401` with its challenge.
fn unauthenticated(status: StatusCode, headers: &HeaderMap, text: &str) -> bool {
    status == StatusCode::UNAUTHORIZED
        && error_body(text).is_ok_and(|body| body.code == "unauthenticated")
        && headers
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("Bearer realm=\"ferrofed\""))
}

#[tokio::test]
async fn in_production_a_write_without_a_token_is_refused_from_every_peer() -> TestResult {
    for peer in peers() {
        let before = Event::AdminWriteRefused.counted();
        let app = listener(Profile::Production, None, peer);
        let (status, headers, text) = answer(app, write(None)?).await?;
        assert!(
            unauthenticated(status, &headers, &text),
            "{peer:?}: {status} {text}"
        );
        assert_eq!(
            before + 1,
            Event::AdminWriteRefused.counted(),
            "{peer:?}: the refusal is counted"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_token_that_does_not_verify_is_refused_every_write() -> TestResult {
    for profile in [Profile::Production, Profile::Development] {
        for credential in ["Bearer not-a-token", "Basic Z2F0ZXdheTpzZWNyZXQ="] {
            let app = listener(profile, None, Some(SocketAddr::from(([127, 0, 0, 1], 1))));
            let (status, _, text) = answer(app, write(Some(credential))?).await?;
            assert_eq!(
                StatusCode::UNAUTHORIZED,
                status,
                "{profile:?} {credential}: {text}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_token_without_the_operator_scope_is_refused_every_write() -> TestResult {
    for profile in [Profile::Production, Profile::Development] {
        for peer in peers() {
            let app = listener(profile, None, peer);
            let (status, _, text) = answer(app, write(Some(&bearer()?))?).await?;
            assert_eq!(
                StatusCode::FORBIDDEN,
                status,
                "{profile:?} {peer:?}: {text}"
            );
            assert_eq!("scope-insufficient", error_body(&text)?.code);
        }
    }
    Ok(())
}

#[tokio::test]
async fn an_operator_token_is_admitted_from_every_peer() -> TestResult {
    for profile in [Profile::Production, Profile::Development] {
        for peer in peers() {
            let app = listener(profile, None, peer);
            let (status, _, text) = answer(app, write(Some(&operator_bearer()?))?).await?;
            assert!(
                admitted(status, &text),
                "{profile:?} {peer:?}: {status} {text}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn development_admits_a_loopback_peer_without_a_credential_and_no_other() -> TestResult {
    for peer in peers() {
        let app = listener(Profile::Development, None, peer);
        let (status, headers, text) = answer(app, write(None)?).await?;
        if peer.is_some_and(|peer| peer.ip().is_loopback()) {
            assert!(admitted(status, &text), "{peer:?}: {status} {text}");
        } else {
            assert!(
                unauthenticated(status, &headers, &text),
                "{peer:?}: {status} {text}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn without_a_scrape_token_the_scrape_is_open_to_every_peer() -> TestResult {
    for peer in peers() {
        let app = listener(Profile::Production, None, peer);
        let (status, _, _) = answer(app, scrape(None)?).await?;
        assert_eq!(StatusCode::OK, status, "{peer:?}");
    }
    Ok(())
}

#[tokio::test]
async fn with_a_scrape_token_a_scrape_without_a_credential_is_refused() -> TestResult {
    for peer in peers() {
        let before = Event::ScrapeRefused.counted();
        let app = listener(Profile::Production, Some(SCRAPE_TOKEN), peer);
        let (status, headers, text) = answer(app, scrape(None)?).await?;
        assert!(
            unauthenticated(status, &headers, &text),
            "{peer:?}: {status} {text}"
        );
        assert_eq!(
            before + 1,
            Event::ScrapeRefused.counted(),
            "{peer:?}: the refusal is counted"
        );
    }
    Ok(())
}

#[tokio::test]
async fn with_a_scrape_token_another_credential_is_refused() -> TestResult {
    let others = [
        String::from("Bearer synthetic-scrape-token"),
        format!("Bearer {SCRAPE_TOKEN}x"),
        format!("Basic {SCRAPE_TOKEN}"),
        String::from(SCRAPE_TOKEN),
        operator_bearer()?,
    ];
    for credential in &others {
        let app = listener(Profile::Production, Some(SCRAPE_TOKEN), None);
        let (status, headers, text) = answer(app, scrape(Some(credential))?).await?;
        assert!(
            unauthenticated(status, &headers, &text),
            "{credential}: {status} {text}"
        );
        let challenge = headers
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(challenge.contains("invalid_token"), "{challenge}");
        assert!(!text.contains(SCRAPE_TOKEN), "the token is never echoed");
    }
    let app = listener(Profile::Production, Some(SCRAPE_TOKEN), None);
    let twice = Request::get("/metrics")
        .header(header::AUTHORIZATION, format!("Bearer {SCRAPE_TOKEN}"))
        .header(header::AUTHORIZATION, format!("Bearer {SCRAPE_TOKEN}"))
        .body(Body::empty())?;
    let (status, _, text) = answer(app, twice).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "one bearer token: {text}");
    Ok(())
}

#[tokio::test]
async fn with_a_scrape_token_the_scrape_that_carries_it_is_served() -> TestResult {
    for scheme in ["Bearer", "bearer"] {
        for peer in peers() {
            let app = listener(Profile::Production, Some(SCRAPE_TOKEN), peer);
            let credential = format!("{scheme} {SCRAPE_TOKEN}");
            let (status, _, text) = answer(app, scrape(Some(&credential))?).await?;
            assert_eq!(StatusCode::OK, status, "{scheme} {peer:?}: {text}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn the_scrape_token_admits_no_write() -> TestResult {
    for profile in [Profile::Production, Profile::Development] {
        let app = listener(
            profile,
            Some(SCRAPE_TOKEN),
            Some(SocketAddr::from(([127, 0, 0, 1], 1))),
        );
        let credential = format!("Bearer {SCRAPE_TOKEN}");
        let (status, _, text) = answer(app, write(Some(&credential))?).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, status, "{profile:?}: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn the_served_listener_records_a_loopback_peer() -> TestResult {
    let listener_socket = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener_socket.local_addr()?;
    let server = tokio::spawn(admin::serve(
        listener_socket,
        listener(Profile::Development, None, None),
    ));
    let response = reqwest::Client::new()
        .post(format!("http://{address}{WRITE}"))
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    server.abort();
    assert!(
        admitted(status, &text),
        "a loopback connection reaches the action: {status} {text}"
    );
    Ok(())
}
