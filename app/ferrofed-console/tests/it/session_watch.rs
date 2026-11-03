// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session watch: `GET` and `POST /session` answer what the operator's
//! session has left, and `401` once it is gone, and every page of a
//! signed-in operator carries FerroUI's watch over it.

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};
use wiremock::MockServer;

use crate::support::{COOKIE, console, establish, get, get_as, send, signed_in};

/// What the watch answers a live session has left, in seconds.
#[derive(serde::Deserialize)]
struct Left {
    /// The idle lifetime left.
    idle: u64,
    /// The absolute lifetime left.
    absolute: u64,
}

/// A console over the stub gateway `gateway`, with no provider.
fn configuration(gateway: &MockServer) -> String {
    format!(
        "[gateway]\nbase_url = \"{}\"\n\n[session]\nsecure_cookie = false\n",
        gateway.uri()
    )
}

#[tokio::test]
async fn the_watch_answers_what_the_session_has_left_and_401_once_it_is_gone()
-> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    let (state, service) = console(&configuration(&gateway)).await?;
    let session = establish(&state, signed_in().await?)?;
    for request in [
        get_as(ferrohealth_session::paths::SESSION, &session)?,
        Request::post(ferrohealth_session::paths::SESSION)
            .header("cookie", format!("{COOKIE}={}", session.as_str()))
            .header("sec-fetch-site", "same-origin")
            .body(Body::empty())?,
    ] {
        let (response, body) = send(&service, request).await?;
        assert_eq!(StatusCode::OK, response.status(), "{body}");
        let left: Left = serde_json::from_str(&body)?;
        assert!(left.idle > 0 && left.absolute > 0, "{body}");
    }
    crate::support::end(&state, &session)?;
    let (gone, _body) = send(
        &service,
        get_as(ferrohealth_session::paths::SESSION, &session)?,
    )
    .await?;
    assert_eq!(StatusCode::UNAUTHORIZED, gone.status());
    let (absent, _body) = send(&service, get(ferrohealth_session::paths::SESSION)?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, absent.status());
    Ok(())
}

#[tokio::test]
async fn a_signed_in_page_carries_the_session_watch_and_the_sign_in_does_not()
-> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    let (state, service) = console(&configuration(&gateway)).await?;
    let session = establish(&state, signed_in().await?)?;
    let (_response, page) = send(&service, get_as("/about", &session)?).await?;
    assert!(
        page.contains(ferrohealth_ui::view::session::WARNING_ID),
        "the signed-in page carries the watch"
    );
    let (_response, card) = send(&service, get("/")?).await?;
    assert!(
        !card.contains(ferrohealth_ui::view::session::WARNING_ID),
        "the sign-in card carries none"
    );
    Ok(())
}
