// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A test OpenID Provider the operator signs in at: an authorization
//! endpoint that approves every request at once, a token endpoint that
//! checks the PKCE verifier, the issuer's JWK Set, and an end-session
//! endpoint.
//!
//! The authorization endpoint redirects the browser back to the console
//! with a fresh code and the `state` it was sent (RFC 6749 §4.1.2), and
//! signs an ID Token carrying the request's `nonce` (OpenID Connect Core 1.0
//! §3.1.3.7). The token endpoint trades that code once, for the verifier
//! whose `S256` challenge the request carried (RFC 7636 §4.6), and answers
//! with the operator's access token, which the gateway trusts. Every key is
//! generated per run, so no private key is committed. No specification
//! governs the harness: our own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrofed_testkit::issuer::{Claims, Issuer, JWKS_PATH};
use ferrofed_testkit::mock::Server;
use jsonwebtoken::{Algorithm, Header};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::console::{AUDIENCE, ISSUER, OPERATOR_SCOPE};

/// The console's client id at the provider.
pub(crate) const CLIENT_ID: &str = "ferrofed-viewer";

/// The path of the authorization endpoint.
const AUTHORIZE: &str = "/authorize";

/// The path of the token endpoint.
const TOKEN: &str = "/token";

/// The path of the end-session endpoint.
pub(crate) const END_SESSION: &str = "/end-session";

/// What one approved authorization request leaves for the token request:
/// the ID Token to answer with and the PKCE challenge to hold the verifier
/// to.
#[derive(Debug)]
struct Approved {
    /// The signed ID Token.
    id_token: String,
    /// The `code_challenge` the request carried.
    challenge: String,
}

/// The codes the provider has issued and not yet traded.
type Codes = Arc<Mutex<BTreeMap<String, Approved>>>;

/// A running test provider.
#[derive(Debug)]
pub(crate) struct Provider {
    /// The mock server every endpoint answers on.
    server: Server,
    /// The key set the gateway trusts the operator's access token by.
    jwks: String,
    /// The operator's access token, which never reaches the browser.
    access_token: String,
}

impl Provider {
    /// Starts a provider whose issuer is its own base URL, which signs the
    /// operator's access token as [`ISSUER`] for [`AUDIENCE`] with
    /// [`OPERATOR_SCOPE`].
    pub(crate) async fn start() -> Result<Self, Box<dyn Error>> {
        let server = Server::start().await;
        let issuer = Issuer::new(server.uri())?;
        issuer.publish(&server).await?;
        let mut claims = Claims::new(ISSUER, AUDIENCE);
        let scope = claims.scope.take().unwrap_or_default();
        claims.scope = Some(format!("{scope} {OPERATOR_SCOPE}"));
        let access_token = issuer.mint(&claims)?;
        let jwks = issuer.jwks_json()?;
        let codes = Codes::default();
        Mock::given(method("GET"))
            .and(path(AUTHORIZE))
            .respond_with(Authorize {
                issuer: Arc::new(issuer),
                codes: Arc::clone(&codes),
            })
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(TOKEN))
            .respond_with(Token {
                codes,
                access_token: access_token.clone(),
            })
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(END_SESSION))
            .respond_with(EndSession)
            .mount(&server)
            .await;
        Ok(Self {
            server,
            jwks,
            access_token,
        })
    }

    /// The provider's base URL, which is its issuer identifier.
    pub(crate) fn uri(&self) -> String {
        self.server.uri()
    }

    /// The JWK Set the gateway verifies the operator's access token with.
    pub(crate) fn jwks(&self) -> &str {
        &self.jwks
    }

    /// The operator's access token, which the console keeps on its server.
    pub(crate) fn access_token(&self) -> &str {
        &self.access_token
    }

    /// The `[oidc]` table of a console at `console`, its base URL.
    pub(crate) fn console_configuration(&self, console: &str) -> String {
        let uri = self.uri();
        format!(
            "[oidc]\nissuer = \"{uri}\"\nauthorization_endpoint = \"{uri}{AUTHORIZE}\"\n\
             token_endpoint = \"{uri}{TOKEN}\"\njwks_uri = \"{uri}{JWKS_PATH}\"\n\
             client_id = \"{CLIENT_ID}\"\nredirect_uri = \"{console}/auth/callback\"\n\
             scopes = [\"openid\"]\nend_session_endpoint = \"{uri}{END_SESSION}\"\n\
             post_logout_redirect_uri = \"{console}/\"\n"
        )
    }

    /// The query pairs of every request the provider received at `at`.
    pub(crate) async fn requests_at(
        &self,
        at: &str,
    ) -> Result<Vec<BTreeMap<String, String>>, Box<dyn Error>> {
        let received = self
            .server
            .received_requests()
            .await
            .ok_or("the provider records its requests")?;
        Ok(received
            .iter()
            .filter(|request| request.url.path() == at)
            .map(|request| request.url.query_pairs().into_owned().collect())
            .collect())
    }

    /// The query pairs of every authorization request the provider received.
    pub(crate) async fn authorization_requests(
        &self,
    ) -> Result<Vec<BTreeMap<String, String>>, Box<dyn Error>> {
        self.requests_at(AUTHORIZE).await
    }
}

/// The ID Token `issuer` signs as the provider `name` for the sign-in that
/// sent `nonce`.
fn id_token(issuer: &Issuer, name: &str, nonce: &str) -> Result<String, Box<dyn Error>> {
    let now = jiff::Timestamp::now().as_second();
    let payload = serde_json::json!({
        "iss": name,
        "aud": CLIENT_ID,
        "sub": "synthetic-operator",
        "iat": now,
        "exp": now.saturating_add(300),
        "nonce": nonce,
    });
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(String::from("k1"));
    header.typ = Some(String::from("JWT"));
    Ok(issuer.sign(&header, &payload.to_string())?)
}

/// A plain-text answer of `status` saying `why`.
fn refused(status: u16, why: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_string(why.to_owned())
}

/// The authorization endpoint: approves every request at once.
struct Authorize {
    /// The key the ID Token is signed with.
    issuer: Arc<Issuer>,
    /// Where the approved request is kept for the token request.
    codes: Codes,
}

impl Respond for Authorize {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let pairs: BTreeMap<String, String> = request.url.query_pairs().into_owned().collect();
        let field = |name: &str| pairs.get(name).map(String::as_str);
        let (Some(redirect), Some(state), Some(nonce), Some(challenge), Some("S256")) = (
            field("redirect_uri"),
            field("state"),
            field("nonce"),
            field("code_challenge"),
            field("code_challenge_method"),
        ) else {
            return refused(400, "the authorization request is incomplete");
        };
        if field("client_id") != Some(CLIENT_ID) || field("response_type") != Some("code") {
            return refused(
                400,
                "the authorization request names another client or grant",
            );
        }
        let Ok(mut location) = Url::parse(redirect) else {
            return refused(400, "the redirect_uri is no URL");
        };
        let Ok(id_token) = id_token(&self.issuer, self.issuer.name(), nonce) else {
            return refused(500, "the ID Token could not be signed");
        };
        let code = uuid::Uuid::new_v4().to_string();
        self.codes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                code.clone(),
                Approved {
                    id_token,
                    challenge: challenge.to_owned(),
                },
            );
        location
            .query_pairs_mut()
            .append_pair("code", &code)
            .append_pair("state", state);
        ResponseTemplate::new(303).insert_header("location", location.as_str())
    }
}

/// The token endpoint: trades a code once, for its PKCE verifier.
struct Token {
    /// The codes the authorization endpoint issued.
    codes: Codes,
    /// The operator's access token.
    access_token: String,
}

impl Respond for Token {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let form: BTreeMap<String, String> = url::form_urlencoded::parse(&request.body)
            .into_owned()
            .collect();
        let field = |name: &str| form.get(name).map(String::as_str);
        let (Some("authorization_code"), Some(code), Some(verifier)) =
            (field("grant_type"), field("code"), field("code_verifier"))
        else {
            return refused(400, "the token request is incomplete");
        };
        let approved = self
            .codes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(code);
        let Some(approved) = approved else {
            return refused(400, "the code was never issued or is spent");
        };
        let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
        if challenge != approved.challenge {
            return refused(400, "the code verifier does not answer the challenge");
        }
        let body = serde_json::json!({
            "access_token": self.access_token,
            "token_type": "Bearer",
            "expires_in": 300,
            "id_token": approved.id_token,
        });
        ResponseTemplate::new(200)
            .insert_header("content-type", "application/json")
            .set_body_string(body.to_string())
    }
}

/// The end-session endpoint: sends the browser to the post-logout redirect
/// it was given (OpenID Connect RP-Initiated Logout 1.0 §2).
struct EndSession;

impl Respond for EndSession {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let redirect = request
            .url
            .query_pairs()
            .find(|(name, _)| name == "post_logout_redirect_uri")
            .map(|(_, value)| value.into_owned());
        match redirect {
            Some(redirect) => ResponseTemplate::new(303).insert_header("location", redirect),
            None => refused(400, "no post_logout_redirect_uri"),
        }
    }
}
