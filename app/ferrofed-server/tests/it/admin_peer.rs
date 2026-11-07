// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener's authentication. A write action is admitted by client
//! authentication only for a token that carries the operator scope its
//! issuer's entry names, from any peer; `profile = "development"` admits a
//! loopback peer that presents no credential as well. `GET /metrics` is open
//! until `[metrics] scrape_clients` names clients, and then answers a
//! scrape whose client-credentials token (RFC 6749 §4.4) names one of them
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
use ferrofed_server::security::Event;
use ferrofed_server::state::AppState;
use http::{HeaderMap, Request, StatusCode, header};
use tokio::net::TcpListener;

use crate::support::caller::{SCRAPE_CLIENT, basic, bearer, machine_bearer, operator_bearer};
use crate::support::{self, error_body, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// A remote peer under the documentation range of RFC 5737.
const REMOTE: ([u8; 4], u16) = ([192, 0, 2, 10], 40_000);

/// The operator's distribution of a stored-query version.
const WRITE: &str = "/admin/stored-queries/example::query/1.0.0/distribute";

/// A client the test issuer signs for and `scrape_clients` does not name.
const OTHER_CLIENT: &str = "synthetic-other-client";

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

/// The admin listener's application under `profile`, [`SCRAPE_CLIENT`]
/// naming the scrape when `scraping`, as a request from `peer` reaches it.
fn listener(profile: Profile, scraping: bool, peer: Option<SocketAddr>) -> Router {
    let clients = if scraping {
        vec![SCRAPE_CLIENT.to_owned()]
    } else {
        Vec::new()
    };
    let access = Access::new(&support::caller::auth(), profile, clients);
    let app = admin::router(Arc::new(AppState::default()), access, None, None);
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
const BEARER_CHALLENGE: &str = "Bearer realm=\"ferrofed\"";

#[tokio::test]
async fn in_production_a_write_without_a_token_is_refused_from_every_peer() -> TestResult {
    for peer in peers() {
        let before = Event::AdminWriteRefused.counted();
        let app = listener(Profile::Production, false, peer);
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
            let app = listener(profile, false, Some(SocketAddr::from(([127, 0, 0, 1], 1))));
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
            let app = listener(profile, false, peer);
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
            let app = listener(profile, false, peer);
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
        let app = listener(Profile::Development, false, peer);
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
async fn without_scrape_clients_the_scrape_is_open_to_every_peer() -> TestResult {
    for peer in peers() {
        let app = listener(Profile::Production, false, peer);
        let (status, _, _) = answer(app, scrape(None)?).await?;
        assert_eq!(StatusCode::OK, status, "{peer:?}");
    }
    Ok(())
}

#[tokio::test]
async fn with_scrape_clients_a_scrape_without_a_credential_is_refused() -> TestResult {
    for peer in peers() {
        let before = Event::ScrapeRefused.counted();
        let app = listener(Profile::Production, true, peer);
        let (status, headers, text) = answer(app, scrape(None)?).await?;
        assert!(
            challenged(status, &headers, &text, BEARER_CHALLENGE),
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

// RFC 6750 §3: a token that does not verify, or a credential of another scheme,
// is refused with the bearer challenge, and the credential is never echoed.
#[tokio::test]
async fn with_scrape_clients_another_credential_is_refused() -> TestResult {
    let forged = machine_bearer(SCRAPE_CLIENT)?.replacen('.', ".x", 2);
    let others = [
        basic(SCRAPE_CLIENT, "synthetic-scrape-password"),
        String::from("Bearer synthetic-not-a-token"),
        forged,
        String::from("synthetic-scrape-password"),
    ];
    for credential in &others {
        let app = listener(Profile::Production, true, None);
        let (status, headers, text) = answer(app, scrape(Some(credential))?).await?;
        assert!(
            challenged(status, &headers, &text, BEARER_CHALLENGE)
                || (status == StatusCode::UNAUTHORIZED && credential.starts_with("Basic")),
            "{credential}: {status} {text}"
        );
        assert!(
            !text.contains("synthetic-scrape-password") && !text.contains("synthetic-not-a-token"),
            "the credential is never echoed"
        );
    }
    let app = listener(Profile::Production, true, None);
    let token = machine_bearer(SCRAPE_CLIENT)?;
    let twice = Request::get("/metrics")
        .header(header::AUTHORIZATION, &token)
        .header(header::AUTHORIZATION, &token)
        .body(Body::empty())?;
    let (status, _, text) = answer(app, twice).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "one credential: {text}");
    Ok(())
}

// RFC 6749 §4.4: a machine lane admits a client's own token alone, so another
// client's, and a person's however privileged, is forbidden.
#[tokio::test]
async fn with_scrape_clients_another_client_or_a_person_is_refused() -> TestResult {
    for credential in [machine_bearer(OTHER_CLIENT)?, operator_bearer()?, bearer()?] {
        let app = listener(Profile::Production, true, None);
        let (status, _, text) = answer(app, scrape(Some(&credential))?).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    }
    Ok(())
}

#[tokio::test]
async fn with_scrape_clients_the_scrape_that_authenticates_as_one_is_served() -> TestResult {
    let credential = machine_bearer(SCRAPE_CLIENT)?;
    let lower = credential.replacen("Bearer", "bearer", 1);
    for credential in [&credential, &lower] {
        for peer in peers() {
            let app = listener(Profile::Production, true, peer);
            let (status, _, text) = answer(app, scrape(Some(credential))?).await?;
            assert_eq!(StatusCode::OK, status, "{peer:?}: {text}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn the_scrape_client_admits_no_write() -> TestResult {
    for profile in [Profile::Production, Profile::Development] {
        let app = listener(profile, true, Some(SocketAddr::from(([127, 0, 0, 1], 1))));
        let credential = machine_bearer(SCRAPE_CLIENT)?;
        let (status, _, text) = answer(app, write(Some(&credential))?).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{profile:?}: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn the_served_listener_records_a_loopback_peer() -> TestResult {
    let listener_socket = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener_socket.local_addr()?;
    let server = tokio::spawn(admin::serve(
        listener_socket,
        listener(Profile::Development, false, None),
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
