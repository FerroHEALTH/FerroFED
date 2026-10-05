// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a configuration from TOML text, the console's service, and
//! one request through it.

use std::collections::BTreeMap;
use std::error::Error;

use axum::body::Body;
use ferrofed_viewer::config::Config;
use ferrofed_viewer::config::settings::Settings;
use ferrofed_viewer::server::{ViewerState, router};
use http::{Request, Response};
use tower::ServiceExt as _;

/// A configuration with an OpenID Provider on loopback, for the sign-in
/// tests.
pub(crate) const WITH_OIDC: &str = r#"
[session]
secure_cookie = false

[oidc]
issuer = "https://idp.example.org/realms/ferrofed"
authorization_endpoint = "https://idp.example.org/realms/ferrofed/auth?kc_idp_hint=example"
client_id = "ferrofed-viewer"
redirect_uri = "https://console.example.org/auth/callback"
scopes = ["openid", "profile"]
"#;

/// Resolves the configuration `text` writes, with no environment.
pub(crate) fn settings(text: &str) -> Result<Settings, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?)
}

/// The state and the service for the configuration `text` writes.
pub(crate) fn console(text: &str) -> Result<(ViewerState, axum::Router), Box<dyn Error>> {
    let state = ViewerState::new(settings(text)?)?;
    let service = router(state.clone());
    Ok((state, service))
}

/// Sends `request` through `service` and reads the whole answer.
pub(crate) async fn send(
    service: &axum::Router,
    request: Request<Body>,
) -> Result<(Response<()>, String), Box<dyn Error>> {
    let response = service.clone().oneshot(request).await?;
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;
    Ok((
        Response::from_parts(parts, ()),
        String::from_utf8(bytes.to_vec())?,
    ))
}

/// A `GET` of `path` with no body.
pub(crate) fn get(path: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::get(path).body(Body::empty())?)
}

/// The value of the header `name` on `response`, or an empty string.
pub(crate) fn header<'a>(response: &'a Response<()>, name: &str) -> &'a str {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}
