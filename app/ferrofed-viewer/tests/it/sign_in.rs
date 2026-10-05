// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator sign-in: the authorization request `GET /login` redirects to
//! (RFC 6749 §4.1.1, RFC 7636 §4.3), the session it leaves on the server,
//! and the callback holding the returned `state` to that session (RFC 6749
//! §10.12).

use std::collections::BTreeMap;
use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};
use url::Url;

use crate::support::{WITH_OIDC, console, get, header, send};

/// The redirect and the session cookie of one `GET /login`.
struct Redirect {
    location: Url,
    cookie: String,
}

impl Redirect {
    /// The query pairs of the authorization request.
    fn query(&self) -> BTreeMap<String, String> {
        self.location.query_pairs().into_owned().collect()
    }

    /// The `state` the authorization request carries.
    fn state(&self) -> String {
        self.query().get("state").cloned().unwrap_or_default()
    }
}

/// Sends `GET /login` through `service`.
async fn sign_in(service: &axum::Router) -> Result<Redirect, Box<dyn Error>> {
    let (response, _body) = send(service, get("/login")?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let location = Url::parse(header(&response, "location"))?;
    let cookie = header(&response, "set-cookie")
        .split(';')
        .next()
        .ok_or("a session cookie")?
        .to_owned();
    Ok(Redirect { location, cookie })
}

/// A `GET` of the callback with `query`, carrying `cookie` when given.
fn callback(query: &str, cookie: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::get(format!("/auth/callback?{query}"));
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    Ok(request.body(Body::empty())?)
}

#[tokio::test]
async fn sign_in_answers_unavailable_when_no_provider_is_configured() -> Result<(), Box<dyn Error>>
{
    let (state, service) = console("")?;
    let (response, _body) = send(&service, get("/login")?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
    assert_eq!(0, state.sessions().count()?);
    Ok(())
}

#[tokio::test]
async fn sign_in_redirects_with_the_code_grant_and_an_s256_challenge() -> Result<(), Box<dyn Error>>
{
    let (_state, service) = console(WITH_OIDC)?;
    let redirect = sign_in(&service).await?;
    assert_eq!(
        "https://idp.example.org/realms/ferrofed/auth",
        format!(
            "{}://{}{}",
            redirect.location.scheme(),
            redirect.location.host_str().unwrap_or_default(),
            redirect.location.path()
        )
    );
    let query = redirect.query();
    let pair = |name: &str| query.get(name).map(String::as_str).unwrap_or_default();
    // RFC 6749 §3.1: the query the endpoint already carries is kept.
    assert_eq!("example", pair("kc_idp_hint"));
    assert_eq!("code", pair("response_type"));
    assert_eq!("ferrofed-viewer", pair("client_id"));
    assert_eq!(
        "https://console.example.org/auth/callback",
        pair("redirect_uri")
    );
    assert_eq!("openid profile", pair("scope"));
    assert_eq!("S256", pair("code_challenge_method"));
    assert_eq!(43, pair("code_challenge").len());
    assert_eq!(43, pair("state").len());
    Ok(())
}

#[tokio::test]
async fn sign_in_holds_the_session_on_the_server_behind_an_opaque_cookie()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(WITH_OIDC)?;
    let (response, _body) = send(&service, get("/login")?).await?;
    let set_cookie = header(&response, "set-cookie");
    assert!(
        set_cookie.starts_with("ferrofed_viewer_session="),
        "{set_cookie}"
    );
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    assert_eq!("no-store", header(&response, "cache-control"));
    let location = header(&response, "location");
    let challenge = Url::parse(location)?
        .query_pairs()
        .find(|(name, _)| name == "code_challenge")
        .map(|(_, value)| value.into_owned())
        .ok_or("a challenge")?;
    assert!(!set_cookie.contains(&challenge), "{set_cookie}");
    assert_eq!(1, state.sessions().count()?);
    Ok(())
}

#[tokio::test]
async fn the_callback_with_the_session_state_reaches_the_code_exchange()
-> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let redirect = sign_in(&service).await?;
    let query = format!("code=an-authorization-code&state={}", redirect.state());
    let (response, _body) = send(&service, callback(&query, Some(&redirect.cookie))?).await?;
    // TODO(#276): a `303` to the console once the code exchange is built.
    assert_eq!(StatusCode::NOT_IMPLEMENTED, response.status());
    Ok(())
}

#[tokio::test]
async fn the_callback_refuses_a_state_that_is_not_the_sessions() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let redirect = sign_in(&service).await?;
    let (response, body) = send(
        &service,
        callback("code=c&state=forged-state", Some(&redirect.cookie))?,
    )
    .await?;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert!(body.contains("state"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_callback_refuses_a_request_without_a_session() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let redirect = sign_in(&service).await?;
    let query = format!("code=c&state={}", redirect.state());
    let (response, _body) = send(&service, callback(&query, None)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    Ok(())
}

#[tokio::test]
async fn a_pending_sign_in_answers_one_callback_only() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let redirect = sign_in(&service).await?;
    let query = format!("code=c&state={}", redirect.state());
    let (first, _body) = send(&service, callback(&query, Some(&redirect.cookie))?).await?;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, first.status());
    let (second, _body) = send(&service, callback(&query, Some(&redirect.cookie))?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, second.status());
    Ok(())
}

// RFC 6749 §4.1.2.1: the provider answers a refused request with `error`.
#[tokio::test]
async fn a_refusal_from_the_provider_answers_unauthorized() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let redirect = sign_in(&service).await?;
    let query = format!("error=access_denied&state={}", redirect.state());
    let (response, _body) = send(&service, callback(&query, Some(&redirect.cookie))?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    Ok(())
}
