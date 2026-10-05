// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator sign-in: the authorization request `GET /login` redirects to
//! (RFC 6749 §4.1.1, RFC 7636 §4.3), the session it leaves on the server,
//! and the callback holding the returned `state` to that session (RFC 6749
//! §10.12).

use std::collections::BTreeMap;
use std::error::Error;

use axum::body::Body;
use ferrofed_viewer::session::Occupancy;
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
    assert_eq!(
        Occupancy {
            sign_ins: 0,
            sessions: 0
        },
        state.sessions().occupancy()?
    );
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
    assert_eq!("http://127.0.0.1:3000/auth/callback", pair("redirect_uri"));
    assert_eq!("openid profile", pair("scope"));
    assert_eq!("S256", pair("code_challenge_method"));
    assert_eq!(43, pair("nonce").len());
    assert_ne!(pair("state"), pair("nonce"));
    assert_eq!(43, pair("code_challenge").len());
    assert_eq!(43, pair("state").len());
    Ok(())
}

#[tokio::test]
async fn sign_in_holds_its_state_on_the_server_and_creates_no_session() -> Result<(), Box<dyn Error>>
{
    let (state, service) = console(WITH_OIDC)?;
    let (response, _body) = send(&service, get("/login")?).await?;
    let set_cookie = header(&response, "set-cookie");
    assert!(
        set_cookie.starts_with("ferrofed_viewer_sign_in="),
        "{set_cookie}"
    );
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    assert!(set_cookie.contains("Max-Age=300"), "{set_cookie}");
    assert_eq!("no-store", header(&response, "cache-control"));
    let location = header(&response, "location");
    let challenge = Url::parse(location)?
        .query_pairs()
        .find(|(name, _)| name == "code_challenge")
        .map(|(_, value)| value.into_owned())
        .ok_or("a challenge")?;
    assert!(!set_cookie.contains(&challenge), "{set_cookie}");
    assert_eq!(
        Occupancy {
            sign_ins: 1,
            sessions: 0
        },
        state.sessions().occupancy()?
    );
    Ok(())
}

#[tokio::test]
async fn the_callback_with_the_session_state_reaches_the_code_exchange()
-> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let redirect = sign_in(&service).await?;
    let query = format!("code=an-authorization-code&state={}", redirect.state());
    let (response, _body) = send(&service, callback(&query, Some(&redirect.cookie))?).await?;
    // The token endpoint of this configuration listens nowhere, so a sign-in
    // that passed its checks reaches the exchange and fails there.
    assert_eq!(StatusCode::BAD_GATEWAY, response.status());
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
    assert_eq!(StatusCode::BAD_GATEWAY, first.status());
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

#[tokio::test]
async fn the_callback_tells_the_browser_to_drop_the_spent_sign_in() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console(WITH_OIDC)?;
    let redirect = sign_in(&service).await?;
    let query = format!("code=c&state={}", redirect.state());
    let (response, _body) = send(&service, callback(&query, Some(&redirect.cookie))?).await?;
    let removal = header(&response, "set-cookie");
    assert!(
        removal.starts_with("ferrofed_viewer_sign_in=;"),
        "{removal}"
    );
    assert!(removal.contains("Max-Age=0"), "{removal}");
    Ok(())
}

/// A console whose pool of pending sign-ins holds four.
const SMALL_SIGN_IN_POOL: &str = r#"
[session]
secure_cookie = false
max_sign_ins = 4
sign_in_timeout_s = 1

[oidc]
issuer = "https://idp.example.org/realms/ferrofed"
authorization_endpoint = "https://idp.example.org/realms/ferrofed/auth"
token_endpoint = "http://127.0.0.1:9/token"
jwks_uri = "http://127.0.0.1:9/jwks.json"
client_id = "ferrofed-viewer"
redirect_uri = "http://127.0.0.1:3000/auth/callback"
"#;

#[tokio::test]
async fn a_flood_of_sign_ins_stays_bounded_and_leaves_signed_in_sessions_alone()
-> Result<(), Box<dyn Error>> {
    let (state, service) = console(SMALL_SIGN_IN_POOL)?;
    let operator = state.sessions().establish(crate::support::signed_in())?;
    let first = sign_in(&service).await?;
    let mut last = sign_in(&service).await?;
    for _ in 0..200 {
        last = sign_in(&service).await?;
        let occupancy = state.sessions().occupancy()?;
        assert!(occupancy.sign_ins <= 4, "{occupancy:?}");
        assert_eq!(1, occupancy.sessions, "{occupancy:?}");
    }
    assert!(state.sessions().access_token(&operator)?.is_some());
    // The oldest sign-in made room for the flood; the newest still completes.
    let dropped_query = format!("code=c&state={}", first.state());
    let (dropped, _body) = send(&service, callback(&dropped_query, Some(&first.cookie))?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, dropped.status());
    let kept_query = format!("code=c&state={}", last.state());
    let (kept, _body) = send(&service, callback(&kept_query, Some(&last.cookie))?).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, kept.status());
    Ok(())
}

#[tokio::test]
async fn a_sign_in_past_its_timeout_is_refused_and_a_new_one_recovers() -> Result<(), Box<dyn Error>>
{
    let (state, service) = console(SMALL_SIGN_IN_POOL)?;
    for _ in 0..10 {
        sign_in(&service).await?;
    }
    let expired = sign_in(&service).await?;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let query = format!("code=c&state={}", expired.state());
    let (refused, _body) = send(&service, callback(&query, Some(&expired.cookie))?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, refused.status());
    let fresh = sign_in(&service).await?;
    assert!(state.sessions().occupancy()?.sign_ins <= 2);
    let query = format!("code=c&state={}", fresh.state());
    let (completed, _body) = send(&service, callback(&query, Some(&fresh.cookie))?).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, completed.status());
    Ok(())
}
