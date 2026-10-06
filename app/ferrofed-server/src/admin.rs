// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener: the operator's surface beside the client face, bound
//! only when `[metrics] listen` is set, to a loopback address unless
//! `[metrics] allow_remote` allows another.
//!
//! It serves the Prometheus text exposition at `GET /metrics`
//! ([`metrics`]), and the distribution of a stored-query version the
//! registry holds to the members that miss it at `POST`
//! [`DISTRIBUTE`]. Every other path is `404`. Neither route is on the
//! gateway's client listener, and neither is part of the ITS-REST surface.
//!
//! Every write or administrative action, the distribution among them, is
//! admitted by the gateway's client authentication ([`Gate`], built from
//! `[auth]`) only for a caller whose token carries the operator scope its
//! issuer's entry names, from any peer ([`operator_only`]). Under
//! `profile = "development"` a loopback peer that presents no credential is
//! admitted as well. `GET /metrics` is open unless `[metrics] scrape_token`
//! is set, and then answers only a scrape that carries that token as its
//! bearer token ([`scrape`]). Every refusal is an [`AdminRefusal`].
// NOTE: no specification governs this: our own design; §12.7 gives drift repair
// no request, so the gateway offers it to the operator only, beside the metrics.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::rejection::ExtensionRejection;
use axum::extract::{Path, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::serve::{Listener, ListenerExt};
use ferrofed_identity::dev::Profile;
use ferrofed_registry::secret::Secret;
use http::{HeaderMap, HeaderValue, StatusCode, header};

use crate::auth::Gate;
use crate::auth::permission::Requirement;
use crate::auth::refusal::{REALM, Refusal};
use crate::config::auth::AuthSettings;
use crate::config::settings::Settings;
use crate::error::{self, Code};
use crate::facade::stored;
use crate::metrics;
use crate::metrics::security::Event;
use crate::request_id;
use crate::state::AppState;

/// The path of the operator's distribution of a held stored-query version:
/// the qualified name and the `major.minor.patch` version, as ITS-REST's
/// Definition API spells them.
pub const DISTRIBUTE: &str = "/admin/stored-queries/{qualified_query_name}/{version}/distribute";

/// Why the admin listener refused a request before its route read anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AdminRefusal {
    /// Client authentication refused the caller of a write action: no
    /// credential, one that does not verify, or a token without the
    /// operator scope.
    #[error("client authentication refused the caller of an admin action: {}", .0.description())]
    Caller(Refusal),
    /// A scrape of `GET /metrics` carries no credential, and the listener
    /// has a scrape token.
    #[error("the scrape carries no bearer token")]
    ScrapeMissing,
    /// A scrape of `GET /metrics` carries a credential that is not the
    /// scrape token as its one bearer token.
    #[error("the scrape carries a credential other than the scrape token")]
    ScrapeToken,
}

impl AdminRefusal {
    /// The security event the refusal is counted as.
    #[must_use]
    pub const fn event(self) -> Event {
        match self {
            Self::Caller(_) => Event::AdminWriteRefused,
            Self::ScrapeMissing | Self::ScrapeToken => Event::ScrapeRefused,
        }
    }

    /// The reason the security log names.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Caller(refusal) => refusal.reason(),
            Self::ScrapeMissing => "missing",
            Self::ScrapeToken => "scrape-token",
        }
    }

    /// The answer to the refused request: the gate's answer for a caller,
    /// and `401` with the RFC 6750 §3 challenge for a scrape.
    #[must_use]
    pub fn response(self, request_id: &str) -> Response {
        let challenge = match self {
            Self::Caller(refusal) => return refusal.response(request_id),
            Self::ScrapeMissing => format!("Bearer realm=\"{REALM}\""),
            Self::ScrapeToken => format!(
                "Bearer realm=\"{REALM}\", error=\"invalid_token\", error_description=\"{self}\""
            ),
        };
        let mut response =
            error::response(Code::Unauthenticated, self.to_string(), request_id).into_response();
        if let Ok(value) = HeaderValue::from_str(&challenge) {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, value);
        }
        response
    }
}

/// Who the admin listener admits: the verifier of `[auth]`, the deployment
/// profile, and the scrape token.
#[derive(Debug)]
pub struct Access {
    /// The verifier of every write action's caller.
    gate: Gate,
    /// The deployment profile.
    profile: Profile,
    /// The bearer token a scrape must carry, when one is set.
    scrape_token: Option<Secret>,
}

impl Access {
    /// Returns the access that verifies a write action's caller as `auth`
    /// describes, under `profile`, with `scrape_token` guarding the scrape.
    #[must_use]
    pub fn new(auth: &AuthSettings, profile: Profile, scrape_token: Option<Secret>) -> Self {
        Self {
            gate: Gate::new(auth),
            profile,
            scrape_token,
        }
    }

    /// Returns the access `settings` describe: `[auth]`, the profile, and
    /// `[metrics] scrape_token`.
    #[must_use]
    pub fn of(settings: &Settings) -> Self {
        Self::new(
            &settings.server.auth,
            settings.profile,
            settings.metrics.scrape_token.clone(),
        )
    }

    /// Admits the caller of a write action with `headers` from `peer`.
    ///
    /// # Errors
    /// Returns [`AdminRefusal::Caller`] with the gate's refusal.
    pub async fn admit_operator(
        &self,
        peer: Option<SocketAddr>,
        headers: &HeaderMap,
    ) -> Result<(), AdminRefusal> {
        // NOTE: no specification governs this: our own design; development keeps the
        // credential-less loopback operator, and every other profile asks the gate.
        if self.profile == Profile::Development
            && !self.gate.presents_credential(headers)
            && peer.is_some_and(|peer| peer.ip().is_loopback())
        {
            return Ok(());
        }
        self.gate
            .admit(headers, (Requirement::Operator, None), false)
            .await
            .map(|_caller| ())
            .map_err(AdminRefusal::Caller)
    }

    /// Admits a scrape with `headers`: any, unless a scrape token is set.
    ///
    /// # Errors
    /// Returns [`AdminRefusal::ScrapeMissing`] for a scrape with no
    /// `Authorization`, and [`AdminRefusal::ScrapeToken`] for one whose
    /// credential is not the scrape token.
    pub fn admit_scrape(&self, headers: &HeaderMap) -> Result<(), AdminRefusal> {
        let Some(expected) = &self.scrape_token else {
            return Ok(());
        };
        if !headers.contains_key(header::AUTHORIZATION) {
            return Err(AdminRefusal::ScrapeMissing);
        }
        if crate::auth::bearer_matches(headers, expected.expose()) {
            Ok(())
        } else {
            Err(AdminRefusal::ScrapeToken)
        }
    }
}

/// Returns the admin listener's application over `state`, admitting as
/// `access` says: `GET /metrics` and `POST` [`DISTRIBUTE`], and `404` on
/// every other path.
///
/// A development profile admits a loopback peer to the write actions
/// without a credential; the peer is the [`ConnectInfo<SocketAddr>`] the
/// server records, so the application is served with
/// `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn router(state: Arc<AppState>, access: Access) -> Router {
    let access = Arc::new(access);
    let metrics = metrics::routes(Arc::clone(state.metrics()))
        .route_layer(middleware::from_fn_with_state(Arc::clone(&access), scrape));
    Router::new()
        .route(DISTRIBUTE, post(distribute))
        .route_layer(middleware::from_fn_with_state(access, operator_only))
        .with_state(state)
        .merge(metrics)
        .fallback(|| async { StatusCode::NOT_FOUND })
}

/// Returns the address and the application of the admin listener
/// `settings` describe over `state`, or `None` when `[metrics] listen` is
/// unset, so no admin route exists.
#[must_use]
pub fn listener(settings: &Settings, state: &Arc<AppState>) -> Option<(SocketAddr, Router)> {
    settings
        .metrics
        .listen
        .map(|address| (address, router(Arc::clone(state), Access::of(settings))))
}

/// Serves the admin listener's `app` on `listener`, plain or TLS, recording
/// each connection's peer for [`operator_only`].
///
/// # Errors
/// Returns the I/O error from accepting or serving connections.
pub async fn serve<L>(listener: L, app: Router) -> std::io::Result<()>
where
    L: Listener<Addr = SocketAddr>,
{
    // NOTE: no specification governs this: our own design; axum records the
    // peer of any listener whose connections it taps, so a TLS one is tapped.
    let tapped = listener.tap_io(|_connection: &mut L::Io| {});
    axum::serve(
        tapped,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

/// The middleware in front of every write action: a caller the gate admits
/// with the operator scope passes ([`Access::admit_operator`]).
///
/// Every other request is answered with its [`AdminRefusal`] before the
/// action reads anything.
pub async fn operator_only(
    State(access): State<Arc<Access>>,
    peer: Result<ConnectInfo<SocketAddr>, ExtensionRejection>,
    request: Request,
    next: Next,
) -> Response {
    // NOTE: no specification governs this: our own design; a peer the server did
    // not record is read as unknown, so it is never the loopback operator.
    let peer = peer.ok().map(|ConnectInfo(peer)| peer);
    match access.admit_operator(peer, request.headers()).await {
        Ok(()) => next.run(request).await,
        Err(refusal) => refuse(refusal, request.headers()),
    }
}

/// The middleware in front of `GET /metrics`: a scrape passes unless a
/// scrape token is set and the scrape does not carry it
/// ([`Access::admit_scrape`]).
pub async fn scrape(State(access): State<Arc<Access>>, request: Request, next: Next) -> Response {
    match access.admit_scrape(request.headers()) {
        Ok(()) => next.run(request).await,
        Err(refusal) => refuse(refusal, request.headers()),
    }
}

/// Counts, logs and answers `refusal` of a request with `headers`.
fn refuse(refusal: AdminRefusal, headers: &HeaderMap) -> Response {
    refusal.event().record();
    tracing::warn!(
        target: crate::facade::security::TARGET,
        event = refusal.event().as_str(),
        reason = refusal.reason(),
        "a request to the admin listener was refused at its authentication"
    );
    refusal.response(request_id::of(headers).unwrap_or_default())
}

/// `POST` [`DISTRIBUTE`]: the registry's held copy sent to the members the
/// targeting headers name (`facade::stored::distribute_held`).
async fn distribute(
    State(state): State<Arc<AppState>>,
    Path((name, version)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    stored::distribute_held(&state, (&name, &version), (&headers, &body)).await
}
