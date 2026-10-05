// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator sign-out: `POST /logout` ends the server-side session, removes
//! the session cookie, and redirects to the provider's end-session endpoint
//! with the ID Token hint (OpenID Connect RP-Initiated Logout 1.0 §2), and
//! refuses a request that does not come from the console's own pages.

use std::collections::BTreeMap;
use std::error::Error;

use axum::body::Body;
use ferrofed_viewer::session::{COOKIE, SessionId};
use http::{Request, StatusCode};
use url::Url;

use crate::support::{OPERATOR_ID_TOKEN, WITH_OIDC, console, header, send, settings, signed_in};

/// [`WITH_OIDC`] with the provider's end-session endpoint and a registered
/// post-logout redirect.
fn with_end_session() -> String {
    format!(
        "{WITH_OIDC}end_session_endpoint = \"https://idp.example.org/realms/ferrofed/logout?ui_locales=en\"\n\
         post_logout_redirect_uri = \"https://console.example.org/\"\n"
    )
}

/// A `POST /logout` carrying `session`'s cookie and the header `name: value`.
fn sign_out(
    session: Option<&SessionId>,
    origin: Option<(&str, &str)>,
) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post("/logout");
    if let Some(session) = session {
        request = request.header("cookie", format!("{COOKIE}={}", session.as_str()));
    }
    if let Some((name, value)) = origin {
        request = request.header(name, value);
    }
    Ok(request.body(Body::empty())?)
}

/// The fetch metadata of a form the console's own page submitted.
const SAME_ORIGIN: (&str, &str) = ("sec-fetch-site", "same-origin");

#[tokio::test]
async fn a_sign_out_ends_the_session_and_removes_the_cookie() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(WITH_OIDC)?;
    let session = state.sessions().establish(signed_in())?;
    let (response, _body) = send(&service, sign_out(Some(&session), Some(SAME_ORIGIN))?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert!(state.sessions().access_token(&session)?.is_none());
    assert_eq!(0, state.sessions().occupancy()?.sessions);
    let removal = header(&response, "set-cookie");
    assert!(
        removal.starts_with("ferrofed_viewer_session=;"),
        "{removal}"
    );
    assert!(removal.contains("Max-Age=0"), "{removal}");
    assert!(removal.contains("Path=/"), "{removal}");
    assert!(removal.contains("HttpOnly"), "{removal}");
    assert_eq!("no-store", header(&response, "cache-control"));
    // Without an end-session endpoint the operator lands on the console.
    assert_eq!("/", header(&response, "location"));
    Ok(())
}

#[tokio::test]
async fn a_sign_out_redirects_to_the_end_session_endpoint_with_the_id_token_hint()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(&with_end_session())?;
    let session = state.sessions().establish(signed_in())?;
    let (response, _body) = send(&service, sign_out(Some(&session), Some(SAME_ORIGIN))?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let location = Url::parse(header(&response, "location"))?;
    assert_eq!(
        "https://idp.example.org/realms/ferrofed/logout",
        format!(
            "{}://{}{}",
            location.scheme(),
            location.host_str().unwrap_or_default(),
            location.path()
        )
    );
    let query: BTreeMap<String, String> = location.query_pairs().into_owned().collect();
    let pair = |name: &str| query.get(name).map(String::as_str);
    // The query the endpoint already carries is kept.
    assert_eq!(Some("en"), pair("ui_locales"));
    assert_eq!(Some(OPERATOR_ID_TOKEN), pair("id_token_hint"));
    assert_eq!(Some("ferrofed-viewer"), pair("client_id"));
    assert_eq!(
        Some("https://console.example.org/"),
        pair("post_logout_redirect_uri")
    );
    assert!(state.sessions().access_token(&session)?.is_none());
    assert!(header(&response, "set-cookie").contains("Max-Age=0"));
    Ok(())
}

#[tokio::test]
async fn a_sign_out_without_a_session_still_reaches_the_provider_without_a_hint()
-> Result<(), Box<dyn Error>> {
    let (_state, service) = console(&with_end_session())?;
    let (response, _body) = send(&service, sign_out(None, Some(SAME_ORIGIN))?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let location = Url::parse(header(&response, "location"))?;
    let query: BTreeMap<String, String> = location.query_pairs().into_owned().collect();
    assert!(!query.contains_key("id_token_hint"), "{query:?}");
    assert_eq!(
        Some("ferrofed-viewer"),
        query.get("client_id").map(String::as_str)
    );
    Ok(())
}

// The header set of a `fetch` from the console's page in a browser without
// fetch metadata; under `no-referrer` the navigation bar's plain form posts
// with `Origin: null` instead, which the cross-site test refuses.
#[tokio::test]
async fn a_sign_out_from_the_consoles_own_origin_is_taken_without_fetch_metadata()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(WITH_OIDC)?;
    let session = state.sessions().establish(signed_in())?;
    let request = sign_out(
        Some(&session),
        Some(("origin", "https://console.example.org")),
    )?;
    let (response, _body) = send(&service, request).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert!(state.sessions().access_token(&session)?.is_none());
    Ok(())
}

#[tokio::test]
async fn a_cross_site_sign_out_is_refused_and_the_session_lives_on() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(&with_end_session())?;
    let session = state.sessions().establish(signed_in())?;
    for origin in [
        Some(("sec-fetch-site", "cross-site")),
        Some(("sec-fetch-site", "same-site")),
        Some(("sec-fetch-site", "none")),
        Some(("origin", "https://attacker.example.net")),
        Some(("origin", "http://console.example.org")),
        Some(("origin", "null")),
        None,
    ] {
        let (response, _body) = send(&service, sign_out(Some(&session), origin)?).await?;
        assert_eq!(StatusCode::FORBIDDEN, response.status(), "{origin:?}");
        assert!(header(&response, "set-cookie").is_empty(), "{origin:?}");
        assert!(header(&response, "location").is_empty(), "{origin:?}");
        assert!(
            state.sessions().access_token(&session)?.is_some(),
            "{origin:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn sign_out_is_never_a_get() -> Result<(), Box<dyn Error>> {
    let (state, service) = console(WITH_OIDC)?;
    let session = state.sessions().establish(signed_in())?;
    let request = Request::get("/logout")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .header(SAME_ORIGIN.0, SAME_ORIGIN.1)
        .body(Body::empty())?;
    let (response, _body) = send(&service, request).await?;
    assert_eq!(StatusCode::METHOD_NOT_ALLOWED, response.status());
    assert!(state.sessions().access_token(&session)?.is_some());
    Ok(())
}

#[tokio::test]
async fn sign_out_answers_unavailable_when_no_provider_is_configured() -> Result<(), Box<dyn Error>>
{
    let (_state, service) = console("")?;
    let (response, _body) = send(&service, sign_out(None, Some(SAME_ORIGIN))?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
    Ok(())
}

#[test]
fn a_post_logout_redirect_without_an_end_session_endpoint_is_refused() {
    let text = format!("{WITH_OIDC}post_logout_redirect_uri = \"https://console.example.org/\"\n");
    let refused = settings(&text)
        .map(|_| ())
        .map_err(|error| error.to_string());
    assert_eq!(
        Err(String::from(
            "oidc.end_session_endpoint (for post_logout_redirect_uri) is required"
        )),
        refused
    );
}

#[test]
fn an_end_session_endpoint_over_plain_http_off_loopback_is_refused() {
    let text = format!("{WITH_OIDC}end_session_endpoint = \"http://idp.example.org/logout\"\n");
    assert!(settings(&text).is_err());
}

#[tokio::test]
async fn the_console_offers_sign_out_as_a_post_form() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let (response, body) = send(&service, crate::support::get("/")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        body.contains(r#"<form method="post" action="/logout""#),
        "{body}"
    );
    Ok(())
}
