// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator sign-in at an OpenID Provider: the OAuth 2.0 authorization code
//! grant with PKCE, run by the console's server half.
//!
//! `GET /login` begins a server-side session, mints the `state` and the
//! PKCE verifier, keeps both in the session, and redirects the browser to
//! the provider's authorization endpoint (RFC 6749 §4.1.1, RFC 7636 §4.3).
//! `GET /auth/callback` is where the provider sends the operator back: it
//! takes the pending sign-in from the session once and holds the returned
//! `state` to it (RFC 6749 §10.12). The browser never sees a token. No
//! specification of the federation governs sign-in to the console: our own
//! design on RFC 6749 and RFC 7636.

use axum::Extension;
use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use http::header::{CACHE_CONTROL, COOKIE, LOCATION, SET_COOKIE};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::Deserialize;
use url::Url;

use crate::config::settings::OidcSettings;
use crate::server::ViewerState;
use crate::session::{PendingSignIn, SessionError, SessionId, challenge};

/// The path of the sign-in route.
pub const LOGIN: &str = "/login";

/// The path of the redirection endpoint the provider sends the operator back
/// to.
pub const CALLBACK: &str = "/auth/callback";

/// Returns the authorization request for `pending` at the provider `oidc`.
///
/// The request is the authorization endpoint with the code grant's
/// parameters and the `S256` PKCE challenge appended to whatever query it
/// already carries, which RFC 6749 §3.1 requires a client to keep.
#[must_use]
pub fn authorization_request(oidc: &OidcSettings, pending: &PendingSignIn) -> Url {
    let mut url = oidc.authorization_endpoint.clone();
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &oidc.client_id)
        .append_pair("redirect_uri", oidc.redirect_uri.as_str())
        .append_pair("scope", &oidc.scopes.join(" "))
        .append_pair("state", pending.state())
        .append_pair("code_challenge", &challenge(pending.verifier()))
        .append_pair("code_challenge_method", "S256");
    url
}

/// Serves `GET /login`: begins a session and redirects to the provider.
pub async fn login(Extension(state): Extension<ViewerState>) -> Response {
    let Some(oidc) = state.settings().oidc.as_ref() else {
        return plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "sign-in is not configured on this console",
        );
    };
    let pending = match PendingSignIn::mint() {
        Ok(pending) => pending,
        Err(error) => return refused(&error),
    };
    let location = authorization_request(oidc, &pending);
    let id = match state.sessions().begin(pending) {
        Ok(id) => id,
        Err(error) => return refused(&error),
    };
    let cookie = state.sessions().cookie(&id).to_string();
    let (Ok(location), Ok(cookie)) = (
        HeaderValue::from_str(location.as_str()),
        HeaderValue::from_str(&cookie),
    ) else {
        tracing::error!("the sign-in redirect could not be written as headers");
        return plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the sign-in redirect could not be written",
        );
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(LOCATION, location);
    headers.insert(SET_COOKIE, cookie);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The query the provider sends the operator back with (RFC 6749 §4.1.2 and
/// §4.1.2.1).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CallbackQuery {
    /// The authorization code.
    pub code: Option<String>,
    /// The `state` the authorization request carried.
    pub state: Option<String>,
    /// The error code of a refused request.
    pub error: Option<String>,
}

/// Serves `GET /auth/callback`: holds the returned `state` to the session's
/// pending sign-in.
pub async fn callback(
    Extension(state): Extension<ViewerState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some(id) = session_cookie(&headers) else {
        return plain(StatusCode::BAD_REQUEST, "no sign-in is pending");
    };
    let pending = match state.sessions().take_pending(&id) {
        Ok(Some(pending)) => pending,
        Ok(None) => return plain(StatusCode::BAD_REQUEST, "no sign-in is pending"),
        Err(error) => return refused(&error),
    };
    // NOTE: RFC 6749 §10.12: a redirect whose `state` is not the one the
    // request carried is refused before anything else it says is read.
    if !query
        .state
        .as_deref()
        .is_some_and(|sent| pending.answers(sent))
    {
        return plain(
            StatusCode::BAD_REQUEST,
            "the sign-in state does not match this session",
        );
    }
    if query.error.is_some() {
        return plain(
            StatusCode::UNAUTHORIZED,
            "the OpenID Provider did not sign the operator in",
        );
    }
    if query.code.is_none() {
        return plain(StatusCode::BAD_REQUEST, "the redirect carries no code");
    }
    // TODO(#276): exchange the code at the token endpoint with the PKCE
    // verifier (RFC 6749 §4.1.3, RFC 7636 §4.5) and hold the tokens in the
    // session, so the console calls the gateway as the operator.
    plain(
        StatusCode::NOT_IMPLEMENTED,
        "the sign-in code exchange is not built yet",
    )
}

/// The session id the request's `Cookie` header carries, if any.
#[must_use]
pub fn session_cookie(headers: &HeaderMap) -> Option<SessionId> {
    // NOTE: RFC 6265 §5.4: a header or pair that does not read as a cookie
    // is not the session cookie, so it is skipped and never an error.
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(cookie::Cookie::split_parse)
        .filter_map(Result::ok)
        .find(|cookie| cookie.name() == crate::session::COOKIE)
        .map(|cookie| SessionId::from_cookie(cookie.value()))
}

/// The answer to a session store that failed.
fn refused(error: &SessionError) -> Response {
    match error {
        SessionError::Full => plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "the console holds as many sessions as it may; try again later",
        ),
        SessionError::Random | SessionError::Poisoned => {
            tracing::error!(%error, "a sign-in session could not be begun");
            plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                "a sign-in session could not be begun",
            )
        }
    }
}

/// A plain-text answer that no cache keeps.
fn plain(status: StatusCode, text: &'static str) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], text).into_response()
}
