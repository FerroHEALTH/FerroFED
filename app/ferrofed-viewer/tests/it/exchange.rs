// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sign-in's code exchange against a test OpenID Provider: the code and
//! the PKCE verifier sent to the token endpoint (RFC 6749 §4.1.3, RFC 7636
//! §4.5), the ID Token held to the provider's keys, its issuer, its audience
//! and the sign-in's `nonce` (OpenID Connect Core 1.0 §3.1.3.7), and a
//! signed-in session begun only when every check holds.

use std::collections::BTreeMap;
use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::issuer::{Issuer, JWKS_PATH};
use ferrofed_testkit::mock::Server;
use ferrofed_viewer::server::ViewerState;
use ferrofed_viewer::session::Occupancy;
use http::{Request, StatusCode};
use jsonwebtoken::{Algorithm, Header};
use secrecy::ExposeSecret as _;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, console, get, header, send};

/// The console's client id at the test provider.
const CLIENT_ID: &str = "ferrofed-viewer";

/// A test provider: its mock server and the issuer that signs there.
struct Provider {
    server: Server,
    issuer: Issuer,
}

impl Provider {
    /// Starts a provider whose issuer is its own base URL.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let server = Server::start().await;
        let issuer = Issuer::new(server.uri())?;
        issuer.publish(&server).await?;
        Ok(Self { server, issuer })
    }

    /// The console configuration that signs in at this provider, with
    /// `secret` as a confidential client's secret when given.
    fn configuration(&self, secret: Option<&str>) -> String {
        let uri = self.server.uri();
        let secret = secret
            .map(|secret| format!("client_secret = \"{secret}\"\n"))
            .unwrap_or_default();
        format!(
            "[session]\nsecure_cookie = false\n\n[oidc]\nissuer = \"{uri}\"\n\
             authorization_endpoint = \"{uri}/auth\"\ntoken_endpoint = \"{uri}/token\"\n\
             jwks_uri = \"{uri}{JWKS_PATH}\"\nclient_id = \"{CLIENT_ID}\"\n\
             redirect_uri = \"http://127.0.0.1:3000/auth/callback\"\n{secret}"
        )
    }

    /// An ID Token signed by `signer`, with `claims` over the defaults.
    fn id_token(signer: &Issuer, claims: &[(&str, String)]) -> Result<String, Box<dyn Error>> {
        let now = jiff::Timestamp::now().as_second();
        let mut payload: BTreeMap<&str, String> = BTreeMap::from([
            ("iss", format!("\"{}\"", signer.name())),
            ("aud", format!("\"{CLIENT_ID}\"")),
            ("sub", String::from("\"synthetic-operator\"")),
            ("iat", now.to_string()),
            ("exp", (now + 300).to_string()),
        ]);
        for (name, value) in claims {
            payload.insert(name, value.clone());
        }
        let json = payload
            .iter()
            .map(|(name, value)| format!("\"{name}\":{value}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(String::from("k1"));
        Ok(signer.sign(&header, &format!("{{{json}}}"))?)
    }

    /// Answers the token request with `id_token` and a bearer token.
    async fn answers(&self, id_token: &str) {
        let body = format!(
            r#"{{"access_token":"{OPERATOR_TOKEN}","token_type":"Bearer","expires_in":300,"id_token":"{id_token}"}}"#
        );
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(body),
            )
            .mount(&self.server)
            .await;
    }
}

/// One sign-in begun at the console: its cookie, and the `state`, the
/// `nonce` and the PKCE challenge of its redirect.
struct Begun {
    cookie: String,
    state: String,
    nonce: String,
    challenge: String,
}

/// Begins a sign-in at `service`.
async fn begin(service: &axum::Router) -> Result<Begun, Box<dyn Error>> {
    let (response, _body) = send(service, get("/login")?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let location = Url::parse(header(&response, "location"))?;
    let pairs: BTreeMap<String, String> = location.query_pairs().into_owned().collect();
    let pair = |name: &str| pairs.get(name).cloned().unwrap_or_default();
    Ok(Begun {
        cookie: header(&response, "set-cookie")
            .split(';')
            .next()
            .unwrap_or_default()
            .to_owned(),
        state: pair("state"),
        nonce: pair("nonce"),
        challenge: pair("code_challenge"),
    })
}

/// Completes `begun` at `service` with the code `code`.
async fn complete(
    service: &axum::Router,
    begun: &Begun,
) -> Result<http::Response<()>, Box<dyn Error>> {
    let request = Request::get(format!(
        "/auth/callback?code=synthetic-code&state={}",
        begun.state
    ))
    .header("cookie", &begun.cookie)
    .body(Body::empty())?;
    Ok(send(service, request).await?.0)
}

/// How many signed-in sessions `state` holds.
fn sessions(state: &ViewerState) -> Result<usize, Box<dyn Error>> {
    let Occupancy { sessions, .. } = state.sessions().occupancy()?;
    Ok(sessions)
}

#[tokio::test]
async fn a_verified_sign_in_begins_a_session_and_returns_to_the_console()
-> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    let token = Provider::id_token(
        &provider.issuer,
        &[("nonce", format!("\"{}\"", begun.nonce))],
    )?;
    provider.answers(&token).await;
    let response = complete(&service, &begun).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/", header(&response, "location"));
    let cookies: Vec<&str> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect();
    let session = cookies
        .iter()
        .find(|cookie| cookie.starts_with("ferrofed_viewer_session="))
        .ok_or("a session cookie")?;
    assert!(session.contains("HttpOnly"), "{session}");
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with("ferrofed_viewer_sign_in=;")),
        "{cookies:?}"
    );
    let id = session
        .split(';')
        .next()
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| ferrofed_viewer::session::SessionId::from_cookie(value))
        .ok_or("a session id")?;
    let held = state
        .sessions()
        .access_token(&id)?
        .ok_or("a live session")?;
    assert_eq!(OPERATOR_TOKEN, held.expose_secret());
    Ok(())
}

#[tokio::test]
async fn the_token_request_carries_the_code_and_the_pkce_verifier() -> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (_state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    let token = Provider::id_token(
        &provider.issuer,
        &[("nonce", format!("\"{}\"", begun.nonce))],
    )?;
    provider.answers(&token).await;
    complete(&service, &begun).await?;
    let requests = provider
        .server
        .received_requests()
        .await
        .ok_or("the provider records requests")?;
    let token_request = requests
        .iter()
        .find(|request| request.url.path() == "/token")
        .ok_or("a token request")?;
    let form: BTreeMap<String, String> = url::form_urlencoded::parse(&token_request.body)
        .into_owned()
        .collect();
    assert_eq!(
        Some("authorization_code"),
        form.get("grant_type").map(String::as_str)
    );
    assert_eq!(Some("synthetic-code"), form.get("code").map(String::as_str));
    assert_eq!(Some(CLIENT_ID), form.get("client_id").map(String::as_str));
    let verifier = form.get("code_verifier").ok_or("a code verifier")?;
    // RFC 7636 §4.6: the verifier hashes to the challenge the redirect sent.
    assert_eq!(
        begun.challenge,
        ferrofed_viewer::session::challenge(&secrecy::SecretString::from(verifier.clone()))
    );
    assert!(!token_request.headers.contains_key("authorization"));
    Ok(())
}

#[tokio::test]
async fn a_confidential_client_authenticates_with_basic_and_names_no_client_id_in_the_form()
-> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (_state, service) = console(&provider.configuration(Some("synthetic-secret")))?;
    let begun = begin(&service).await?;
    let token = Provider::id_token(
        &provider.issuer,
        &[("nonce", format!("\"{}\"", begun.nonce))],
    )?;
    provider.answers(&token).await;
    complete(&service, &begun).await?;
    let requests = provider
        .server
        .received_requests()
        .await
        .ok_or("the provider records requests")?;
    let token_request = requests
        .iter()
        .find(|request| request.url.path() == "/token")
        .ok_or("a token request")?;
    let authorization = token_request
        .headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or("a Basic credential")?;
    // RFC 6749 §2.3.1: base64 of the id and the secret, each form-urlencoded.
    assert_eq!(
        "Basic ZmVycm9mZWQtdmlld2VyOnN5bnRoZXRpYy1zZWNyZXQ=",
        authorization
    );
    let body = String::from_utf8(token_request.body.clone())?;
    assert!(!body.contains("client_id="), "{body}");
    Ok(())
}

#[tokio::test]
async fn an_id_token_with_another_nonce_is_refused_and_begins_no_session()
-> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    let token = Provider::id_token(
        &provider.issuer,
        &[("nonce", String::from("\"a-replayed-nonce\""))],
    )?;
    provider.answers(&token).await;
    let response = complete(&service, &begun).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    assert_eq!(0, sessions(&state)?);
    Ok(())
}

#[tokio::test]
async fn an_id_token_with_no_nonce_is_refused() -> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    provider
        .answers(&Provider::id_token(&provider.issuer, &[])?)
        .await;
    let response = complete(&service, &begun).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    assert_eq!(0, sessions(&state)?);
    Ok(())
}

#[tokio::test]
async fn an_id_token_of_another_issuer_or_audience_is_refused() -> Result<(), Box<dyn Error>> {
    for claim in [
        ("iss", String::from("\"https://elsewhere.example.org\"")),
        ("aud", String::from("\"another-client\"")),
        ("azp", String::from("\"another-client\"")),
    ] {
        let provider = Provider::start().await?;
        let (state, service) = console(&provider.configuration(None))?;
        let begun = begin(&service).await?;
        let token = Provider::id_token(
            &provider.issuer,
            &[("nonce", format!("\"{}\"", begun.nonce)), claim.clone()],
        )?;
        provider.answers(&token).await;
        let response = complete(&service, &begun).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, response.status(), "{claim:?}");
        assert_eq!(0, sessions(&state)?, "{claim:?}");
    }
    Ok(())
}

#[tokio::test]
async fn an_id_token_signed_by_a_key_the_provider_does_not_publish_is_refused()
-> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    let stranger = Issuer::new(provider.server.uri())?;
    let token = Provider::id_token(&stranger, &[("nonce", format!("\"{}\"", begun.nonce))])?;
    provider.answers(&token).await;
    let response = complete(&service, &begun).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    assert_eq!(0, sessions(&state)?);
    Ok(())
}

#[tokio::test]
async fn a_refusing_token_endpoint_answers_bad_gateway() -> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(400)
                .insert_header("content-type", "application/json")
                .set_body_string(r#"{"error":"invalid_grant"}"#),
        )
        .mount(&provider.server)
        .await;
    let response = complete(&service, &begun).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, response.status());
    assert_eq!(0, sessions(&state)?);
    Ok(())
}

// OpenID Connect Core 1.0 §3.1.3.7 item 9: the current time is before
// `exp`, so a token past it, beyond the clock leeway, is refused.
#[tokio::test]
async fn an_expired_id_token_is_refused() -> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    let now = jiff::Timestamp::now().as_second();
    let token = Provider::id_token(
        &provider.issuer,
        &[
            ("nonce", format!("\"{}\"", begun.nonce)),
            ("iat", (now - 7200).to_string()),
            ("exp", (now - 3600).to_string()),
        ],
    )?;
    provider.answers(&token).await;
    let response = complete(&service, &begun).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    assert_eq!(0, sessions(&state)?);
    Ok(())
}

// OpenID Connect Core 1.0 §3.1.3.7 item 3: the console refuses an `aud`
// that names any audience besides its own `client_id`, even with an `azp`.
#[tokio::test]
async fn an_id_token_for_two_audiences_is_refused() -> Result<(), Box<dyn Error>> {
    for azp in [None, Some(format!("\"{CLIENT_ID}\""))] {
        let provider = Provider::start().await?;
        let (state, service) = console(&provider.configuration(None))?;
        let begun = begin(&service).await?;
        let mut claims = vec![
            ("nonce", format!("\"{}\"", begun.nonce)),
            ("aud", format!("[\"{CLIENT_ID}\",\"another-client\"]")),
        ];
        if let Some(azp) = &azp {
            claims.push(("azp", azp.clone()));
        }
        let token = Provider::id_token(&provider.issuer, &claims)?;
        provider.answers(&token).await;
        let response = complete(&service, &begun).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, response.status(), "{azp:?}");
        assert_eq!(0, sessions(&state)?, "{azp:?}");
    }
    Ok(())
}

#[tokio::test]
async fn an_id_token_with_its_one_audience_in_an_array_is_accepted() -> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    let token = Provider::id_token(
        &provider.issuer,
        &[
            ("nonce", format!("\"{}\"", begun.nonce)),
            ("aud", format!("[\"{CLIENT_ID}\"]")),
        ],
    )?;
    provider.answers(&token).await;
    let response = complete(&service, &begun).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!(1, sessions(&state)?);
    Ok(())
}

/// The `name=value` pair of the session cookie `response` sets.
fn session_cookie(response: &http::Response<()>) -> Result<String, Box<dyn Error>> {
    Ok(response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|cookie| cookie.starts_with("ferrofed_viewer_session="))
        .and_then(|cookie| cookie.split(';').next())
        .ok_or("a session cookie")?
        .to_owned())
}

// No specification governs this: our own design; a new sign-in in a browser
// that holds a session ends that session, so it is never live beside the new one.
#[tokio::test]
async fn a_new_sign_in_ends_the_session_it_replaces() -> Result<(), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) = console(&provider.configuration(None))?;
    let first = begin(&service).await?;
    let token = Provider::id_token(
        &provider.issuer,
        &[("nonce", format!("\"{}\"", first.nonce))],
    )?;
    provider.answers(&token).await;
    let old = session_cookie(&complete(&service, &first).await?)?;
    let old_id = old
        .split_once('=')
        .map(|(_, value)| ferrofed_viewer::session::SessionId::from_cookie(value))
        .ok_or("a session id")?;
    assert!(state.sessions().access_token(&old_id)?.is_some());

    let second = begin(&service).await?;
    provider.server.reset().await;
    provider.issuer.publish(&provider.server).await?;
    let token = Provider::id_token(
        &provider.issuer,
        &[("nonce", format!("\"{}\"", second.nonce))],
    )?;
    provider.answers(&token).await;
    let request = Request::get(format!(
        "/auth/callback?code=synthetic-code&state={}",
        second.state
    ))
    .header("cookie", format!("{}; {old}", second.cookie))
    .body(Body::empty())?;
    let (response, _body) = send(&service, request).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let new = session_cookie(&response)?;
    assert_ne!(old, new);
    assert!(state.sessions().access_token(&old_id)?.is_none());
    assert_eq!(1, sessions(&state)?);
    Ok(())
}

/// The claims of an ID Token signed with a shared secret.
#[derive(serde::Serialize)]
struct SharedSecretClaims {
    iss: String,
    aud: &'static str,
    sub: &'static str,
    iat: i64,
    exp: i64,
    nonce: String,
}

// The console verifies an ID Token with an asymmetric algorithm alone, so a
// published `oct` key and an `HS256` signature are refused.
#[tokio::test]
async fn an_id_token_signed_with_a_published_shared_secret_is_refused() -> Result<(), Box<dyn Error>>
{
    let server = Server::start().await;
    let issuer = Issuer::new(server.uri())?;
    let secret = b"a-synthetic-shared-secret-of-thirty-two";
    let k = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, secret);
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(format!(
                    r#"{{"keys":[{{"kty":"oct","kid":"k1","alg":"HS256","k":"{k}"}}]}}"#
                )),
        )
        .mount(&server)
        .await;
    let provider = Provider { server, issuer };
    let (state, service) = console(&provider.configuration(None))?;
    let begun = begin(&service).await?;
    let now = jiff::Timestamp::now().as_second();
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(String::from("k1"));
    let token = jsonwebtoken::encode(
        &header,
        &SharedSecretClaims {
            iss: provider.issuer.name().to_owned(),
            aud: CLIENT_ID,
            sub: "synthetic-operator",
            iat: now,
            exp: now + 300,
            nonce: begun.nonce.clone(),
        },
        &jsonwebtoken::EncodingKey::from_secret(secret),
    )?;
    provider.answers(&token).await;
    let response = complete(&service, &begun).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    assert_eq!(0, sessions(&state)?);
    Ok(())
}
