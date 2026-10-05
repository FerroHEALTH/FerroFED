// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health route, the sign-in routes, the
//! Leptos pages and the site bundle, under one set of browser security
//! headers.
//!
//! Every response carries a Content-Security-Policy whose script source is a
//! nonce minted for that response alone, `nosniff`, `DENY` framing and no
//! referrer, and every document is `no-store`. No specification governs the
//! console's HTTP surface: our own design.

use std::sync::Arc;

use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use http::{HeaderValue, StatusCode};
use leptos::config::LeptosOptions;
use leptos::nonce::Nonce;
use leptos_axum::LeptosRoutes as _;
use serde::Serialize;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{And, Predicate as _, SizeAbove};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::config::settings::Settings;
use crate::gateway::{Gateway, GatewayError};
use crate::session::Sessions;

/// The path of the console's own liveness route.
pub const HEALTH: &str = "/health";

/// The name cargo-leptos gives the site bundle's files under `pkg/`.
pub const OUTPUT_NAME: &str = "ferrofed-viewer";

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ViewerState {
    settings: Arc<Settings>,
    sessions: Sessions,
    gateway: Gateway,
    provider: reqwest::Client,
}

impl ViewerState {
    /// The state for `settings`, with an empty session store.
    ///
    /// # Errors
    /// Returns the [`GatewayError`] of a gateway client, or of the client of
    /// the OpenID Provider, that cannot be built.
    pub fn new(settings: Settings) -> Result<Self, GatewayError> {
        let gateway = Gateway::new(&settings.gateway)?;
        // NOTE: RFC 6749 §3.2: the token endpoint answers the request itself, so
        // the provider's client follows no redirect.
        let provider = reqwest::Client::builder()
            .timeout(settings.gateway.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| GatewayError::Transport { source })?;
        Ok(Self {
            sessions: Sessions::new(settings.session),
            settings: Arc::new(settings),
            gateway,
            provider,
        })
    }

    /// The HTTP client of the OpenID Provider, which the sign-in exchanges
    /// its code with.
    #[must_use]
    pub fn provider(&self) -> &reqwest::Client {
        &self.provider
    }

    /// The resolved configuration.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The session store.
    #[must_use]
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// The gateway client.
    #[must_use]
    pub fn gateway(&self) -> &Gateway {
        &self.gateway
    }
}

/// The Leptos options the console renders with: its site bundle under the
/// configured site root and its listen address.
#[must_use]
pub fn leptos_options(settings: &Settings) -> LeptosOptions {
    LeptosOptions::builder()
        .output_name(OUTPUT_NAME)
        .site_root(settings.site_root.to_string_lossy().into_owned())
        .site_pkg_dir("pkg")
        .site_addr(settings.listen)
        .build()
}

/// Builds the console's whole HTTP service over `state`.
pub fn router(state: ViewerState) -> axum::Router {
    let options = leptos_options(state.settings());
    let routes = leptos_axum::generate_route_list(crate::app::App);
    let context = {
        let state = state.clone();
        move || {
            leptos::context::provide_context(state.clone());
            provide_request_nonce();
        }
    };
    let pages = axum::Router::new()
        .route(HEALTH, get(health))
        .route(crate::oidc::LOGIN, get(crate::oidc::login))
        .route(crate::oidc::CALLBACK, get(crate::oidc::callback))
        .route(
            crate::oidc::logout::LOGOUT,
            axum::routing::post(crate::oidc::logout::logout),
        )
        .leptos_routes_with_context(&options, routes, context.clone(), {
            let options = options.clone();
            move || crate::app::shell(options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler_with_context(
            context,
            crate::app::shell,
        ))
        .layer(axum::middleware::from_fn(require_session))
        .layer(axum::middleware::from_fn(same_origin_only))
        .layer(Extension(state));
    with_security_headers(pages.layer(bundle_compression())).with_state(options)
}

/// Sends a request for an operator view that carries no live signed-in
/// session to sign-in, before anything is rendered or asked of the gateway.
///
/// The server functions check the session themselves as well, because each
/// is a public endpoint of its own.
async fn require_session(
    Extension(state): Extension<ViewerState>,
    request: Request,
    next: Next,
) -> Response {
    // NOTE: no specification governs this: our own design; a trailing slash or
    // another case names the same view, so the gate never misses one the router serves.
    let path = request
        .uri()
        .path()
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if !crate::views::PATHS.contains(&path.as_str()) {
        return next.run(request).await;
    }
    let name = state.sessions().cookie_name(crate::session::COOKIE);
    let signed_in = crate::oidc::cookie_named(request.headers(), &name)
        .map(|id| state.sessions().access_token(&id));
    match signed_in {
        Some(Ok(Some(_token))) => next.run(request).await,
        Some(Err(error)) => {
            tracing::error!(%error, "the session store could not be read");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Some(Ok(None)) | None => {
            let mut response = StatusCode::SEE_OTHER.into_response();
            response.headers_mut().insert(
                http::header::LOCATION,
                HeaderValue::from_static(crate::app::SIGN_IN),
            );
            response
        }
    }
}

/// The fetch metadata header that names where a request comes from.
const SEC_FETCH_SITE: &str = "sec-fetch-site";

/// Refuses every request that is not a safe method, a server function call
/// and a sign-out among them, unless it comes from the console's own pages
/// ([`same_origin`]), before anything reads the session or asks the gateway.
///
/// The session cookie is `SameSite=Lax` as well, so a cross-site `POST`
/// carries no session in the first place; this check holds without it.
async fn same_origin_only(
    Extension(state): Extension<ViewerState>,
    request: Request,
    next: Next,
) -> Response {
    if request.method().is_safe() {
        return next.run(request).await;
    }
    let origin = state
        .settings()
        .oidc
        .as_ref()
        .map(crate::config::settings::OidcSettings::origin);
    if same_origin(request.headers(), origin.as_deref()) {
        return next.run(request).await;
    }
    tracing::warn!(
        method = %request.method(),
        "a request that did not come from the console's own pages was refused"
    );
    (
        StatusCode::FORBIDDEN,
        [(CACHE_CONTROL, "no-store")],
        "the console takes this request only from its own pages",
    )
        .into_response()
}

/// Whether `headers` show a request sent from a page of the console at
/// `origin`, its own `scheme://host:port`.
///
/// `Sec-Fetch-Site` decides when the browser sends it, and only
/// `same-origin` passes. Without it `Origin` must be `origin`, and without
/// that the origin of `Referer` must be. A request that shows none of them,
/// or a console with no known origin, is refused, so an unsafe request is
/// never taken on trust (the Fetch Metadata Request Headers; RFC 6454 §7).
#[must_use]
pub fn same_origin(headers: &http::HeaderMap, origin: Option<&str>) -> bool {
    if let Some(site) = headers.get(SEC_FETCH_SITE) {
        return site.as_bytes() == b"same-origin";
    }
    let Some(origin) = origin else {
        return false;
    };
    if let Some(sent) = headers.get(http::header::ORIGIN) {
        return sent.as_bytes() == origin.as_bytes();
    }
    // NOTE: RFC 9110 §10.1.3: a `Referer` that is no URL names no origin, which
    // refuses the request as a missing one does.
    headers
        .get(http::header::REFERER)
        .and_then(|referer| referer.to_str().ok())
        .and_then(|referer| url::Url::parse(referer).ok())
        .is_some_and(|referer| referer.origin().ascii_serialization() == origin)
}

/// The body of the liveness route.
#[derive(Debug, Serialize)]
struct Health {
    /// Always `up`: the route answers while the process serves.
    status: &'static str,
}

/// Serves `GET /health`: `200` while the process serves.
async fn health() -> Response {
    (StatusCode::OK, axum::Json(Health { status: "up" })).into_response()
}

/// The media types of the site bundle: the WebAssembly, its JavaScript glue
/// and the stylesheet.
const BUNDLE_TYPES: [&str; 4] = [
    "application/wasm",
    "text/javascript",
    "application/javascript",
    "text/css",
];

/// A test of a response, the shape the compression layer's predicate takes.
type ResponsePredicate = fn(StatusCode, http::Version, &http::HeaderMap, &http::Extensions) -> bool;

/// Whether a response carries a file of the site bundle, by its media type.
fn is_bundle(
    _status: StatusCode,
    _version: http::Version,
    headers: &http::HeaderMap,
    _extensions: &http::Extensions,
) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|media| BUNDLE_TYPES.iter().any(|bundle| media.starts_with(bundle)))
}

/// Compresses the site bundle with brotli or gzip, as the request's
/// `Accept-Encoding` chooses, and marks it `Vary: Accept-Encoding`.
///
/// Only the bundle is compressed. A document or a server function's answer
/// carries what the operator entered beside data an attacker would want, so
/// it is never compressed, which keeps a compression side channel shut
/// (BREACH). The Leptos book (`deployment/binary_size`) has a site serve its
/// WebAssembly compressed. No specification governs this: our own design.
fn bundle_compression() -> CompressionLayer<And<SizeAbove, ResponsePredicate>> {
    let bundle: ResponsePredicate = is_bundle;
    CompressionLayer::new()
        .br(true)
        .gzip(true)
        .compress_when(SizeAbove::new(256).and(bundle))
}

/// Wraps `router` in the browser security headers every response carries.
fn with_security_headers(router: axum::Router<LeptosOptions>) -> axum::Router<LeptosOptions> {
    router
        .layer(axum::middleware::from_fn(content_security_policy))
        .layer(axum::middleware::from_fn(cache_control))
        .layer(SetResponseHeaderLayer::overriding(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
}

/// Mints this response's script nonce, hands it to the renderer through
/// the request, and answers with the policy that names it.
async fn content_security_policy(mut request: Request, next: Next) -> Response {
    let nonce = Nonce::new();
    let policy = HeaderValue::from_str(&policy(&nonce));
    request.extensions_mut().insert(nonce);
    let mut response = next.run(request).await;
    match policy {
        Ok(policy) => {
            response
                .headers_mut()
                .insert(CONTENT_SECURITY_POLICY, policy);
            response
        }
        Err(_invalid) => {
            tracing::error!("the Content-Security-Policy could not be written as a header");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Re-provides the request's nonce into the render, so the bootstrap script
/// carries the nonce the response header authorizes.
///
/// `leptos_axum` provides a nonce of its own before this runs, and the
/// request parts it provides carry the one [`content_security_policy`]
/// minted, which replaces it.
fn provide_request_nonce() {
    let Some(parts) = leptos::context::use_context::<http::request::Parts>() else {
        return;
    };
    if let Some(nonce) = parts.extensions.get::<Nonce>() {
        leptos::context::provide_context(nonce.clone());
    }
}

/// The Content-Security-Policy of one response, naming its script nonce.
///
/// The WebAssembly bundle needs `'wasm-unsafe-eval'` to be compiled, which
/// admits no JavaScript `eval` (CSP Level 3).
#[must_use]
pub fn policy(nonce: &Nonce) -> String {
    format!(
        "default-src 'self'; \
         script-src 'self' 'wasm-unsafe-eval' 'nonce-{nonce}'; \
         style-src 'self'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         object-src 'none'; \
         base-uri 'self'; \
         form-action 'self'; \
         frame-ancestors 'none'"
    )
}

/// Marks every document `no-store`, and the site bundle `no-cache`, so a
/// rebuilt bundle is fetched again (RFC 9111 §5.2.2).
async fn cache_control(request: Request, next: Next) -> Response {
    let bundle = request.uri().path().starts_with("/pkg/");
    let mut response = next.run(request).await;
    let value = if bundle && response.status().is_success() {
        "no-cache"
    } else {
        "no-store"
    };
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(value));
    response
}

/// Serves `router` on `listener` until `shutdown` resolves.
///
/// # Errors
/// Returns the I/O error of a listener that fails.
pub async fn serve(
    listener: tokio::net::TcpListener,
    router: axum::Router,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
}

#[cfg(test)]
mod tests {
    use super::policy;
    use leptos::nonce::Nonce;

    #[test]
    fn the_policy_allows_scripts_only_by_nonce_and_never_eval() {
        let nonce = Nonce::new();
        let policy = policy(&nonce);
        let script = policy
            .split(';')
            .map(str::trim)
            .find(|directive| directive.starts_with("script-src"))
            .expect("a script-src directive");
        assert_eq!(
            format!("script-src 'self' 'wasm-unsafe-eval' 'nonce-{nonce}'"),
            script
        );
        assert!(!policy.contains("'unsafe-eval'"), "{policy}");
        assert!(!policy.contains("'unsafe-inline'"), "{policy}");
    }
}
