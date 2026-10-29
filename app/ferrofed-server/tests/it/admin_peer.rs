// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener's authentication. A write action is admitted by client
//! authentication only for a token that carries the operator scope its
//! issuer's entry names, from any peer; `profile = "development"` admits a
//! loopback peer that presents no credential as well. `GET /metrics` is open
//! until `[metrics] scrape_users` names Basic users, and then answers a
//! scrape that authenticates as one of them with HTTP Basic (RFC 7617)
//! alone. Every refusal is counted. No specification governs the admin
//! listener: our own design.
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
use ferrofed_identity::profile::Profile;
use ferrofed_server::admin::{self, Access};
use ferrofed_server::metrics::security::Event;
use ferrofed_server::state::AppState;
use http::{HeaderMap, Request, StatusCode, header};
use tokio::net::TcpListener;

use crate::support::{
    self, SCRAPE_PASSWORD, SCRAPE_USER, basic, bearer, error_body, operator_bearer, send_as_is,
};

type TestResult = Result<(), Box<dyn Error>>;

/// A remote peer under the documentation range of RFC 5737.
const REMOTE: ([u8; 4], u16) = ([192, 0, 2, 10], 40_000);

/// The operator's distribution of a stored-query version.
const WRITE: &str = "/admin/stored-queries/example::query/1.0.0/distribute";

/// A Basic user `[[auth.basic.users]]` writes and `scrape_users` does not
/// name.
const OTHER_USER: &str = "synthetic-other-user";

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

/// The admin listener's application under `profile`, with [`SCRAPE_USER`]
/// and [`OTHER_USER`] as its Basic users and the first naming the scrape
/// when `scraping`, as a request from `peer` reaches it.
fn listener(
    profile: Profile,
    scraping: bool,
    peer: Option<SocketAddr>,
) -> Result<Router, Box<dyn Error>> {
    let users = if scraping {
        vec![SCRAPE_USER.to_owned()]
    } else {
        Vec::new()
    };
    let auth = support::auth_with_basic(&[SCRAPE_USER, OTHER_USER])?;
    let access = Access::new(&auth, profile, users);
    let app = admin::router(Arc::new(AppState::default()), access);
    Ok(match peer {
        Some(peer) => app.layer(MockConnectInfo(peer)),
        None => app,
    })
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
    challenged(status, headers, text, "Bearer realm=\"ferrofed\"")
}

/// Whether `status`, `headers` and `text` are a `401` whose challenge
/// starts with `challenge`.
fn challenged(status: StatusCode, headers: &HeaderMap, text: &str, challenge: &str) -> bool {
    status == StatusCode::UNAUTHORIZED
        && error_body(text).is_ok_and(|body| body.code == "unauthenticated")
        && headers
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with(challenge))
}

/// The Basic challenge of RFC 7617 §2 and §2.1 a scrape is refused with.
const BASIC_CHALLENGE: &str = "Basic realm=\"ferrofed\", charset=\"UTF-8\"";

#[tokio::test]
async fn in_production_a_write_without_a_token_is_refused_from_every_peer() -> TestResult {
    for peer in peers() {
        let before = Event::AdminWriteRefused.counted();
        let app = listener(Profile::Production, false, peer)?;
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
            let app = listener(profile, false, Some(SocketAddr::from(([127, 0, 0, 1], 1))))?;
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
            let app = listener(profile, false, peer)?;
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
            let app = listener(profile, false, peer)?;
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
        let app = listener(Profile::Development, false, peer)?;
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
async fn without_scrape_users_the_scrape_is_open_to_every_peer() -> TestResult {
    for peer in peers() {
        let app = listener(Profile::Production, false, peer)?;
        let (status, _, _) = answer(app, scrape(None)?).await?;
        assert_eq!(StatusCode::OK, status, "{peer:?}");
    }
    Ok(())
}

#[tokio::test]
async fn with_scrape_users_a_scrape_without_a_credential_is_refused() -> TestResult {
    for peer in peers() {
        let before = Event::ScrapeRefused.counted();
        let app = listener(Profile::Production, true, peer)?;
        let (status, headers, text) = answer(app, scrape(None)?).await?;
        assert!(
            challenged(status, &headers, &text, BASIC_CHALLENGE),
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

// RFC 7617 §2: a credential that does not verify, or one of another scheme, is
// refused with the Basic challenge, and the password is never echoed.
#[tokio::test]
async fn with_scrape_users_another_credential_is_refused() -> TestResult {
    let others = [
        basic(SCRAPE_USER, "synthetic-wrong-password"),
        basic("synthetic-unknown-user", SCRAPE_PASSWORD),
        format!("Bearer {SCRAPE_PASSWORD}"),
        String::from(SCRAPE_PASSWORD),
        operator_bearer()?,
    ];
    for credential in &others {
        let app = listener(Profile::Production, true, None)?;
        let (status, headers, text) = answer(app, scrape(Some(credential))?).await?;
        assert!(
            challenged(status, &headers, &text, BASIC_CHALLENGE),
            "{credential}: {status} {text}"
        );
        assert!(
            !text.contains(SCRAPE_PASSWORD),
            "the password is never echoed"
        );
    }
    let app = listener(Profile::Production, true, None)?;
    let twice = Request::get("/metrics")
        .header(header::AUTHORIZATION, basic(SCRAPE_USER, SCRAPE_PASSWORD))
        .header(header::AUTHORIZATION, basic(SCRAPE_USER, SCRAPE_PASSWORD))
        .body(Body::empty())?;
    let (status, _, text) = answer(app, twice).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "one credential: {text}");
    Ok(())
}

#[tokio::test]
async fn with_scrape_users_a_basic_user_they_do_not_name_is_refused() -> TestResult {
    let app = listener(Profile::Production, true, None)?;
    let credential = basic(OTHER_USER, SCRAPE_PASSWORD);
    let (status, _, text) = answer(app, scrape(Some(&credential))?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn with_scrape_users_the_scrape_that_authenticates_as_one_is_served() -> TestResult {
    let credential = basic(SCRAPE_USER, SCRAPE_PASSWORD);
    let lower = credential.replacen("Basic", "basic", 1);
    for credential in [&credential, &lower] {
        for peer in peers() {
            let app = listener(Profile::Production, true, peer)?;
            let (status, _, text) = answer(app, scrape(Some(credential))?).await?;
            assert_eq!(StatusCode::OK, status, "{peer:?}: {text}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn the_scrape_user_admits_no_write() -> TestResult {
    for profile in [Profile::Production, Profile::Development] {
        let app = listener(profile, true, Some(SocketAddr::from(([127, 0, 0, 1], 1))))?;
        let credential = basic(SCRAPE_USER, SCRAPE_PASSWORD);
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
        listener(Profile::Development, false, None)?,
        None,
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
