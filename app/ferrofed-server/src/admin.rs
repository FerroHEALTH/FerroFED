// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener: the operator's surface beside the client face, bound
//! only when `[metrics] listen` is set, to a loopback address unless
//! `[metrics] allow_remote` allows another.
//!
//! It serves the Prometheus text exposition at `GET /metrics`
//! ([`metrics`]), and the distribution of a stored-query version the
//! registry holds to the members that miss it at `POST`
//! [`DISTRIBUTE`], and the clear of one `ehr_id` from this process's
//! routing state at `POST` [`CLEAR_EHR_ID`], and the export of the routing
//! state this process learned at `POST` [`EXPORT_IDENTITY_STATE`], and the
//! reload `SIGHUP` asks for at `POST` [`RELOAD`], for a container no
//! signal reaches ([`serving_router`]), with the running configuration
//! at `GET` [`crate::running::RUNNING`] beside it, a read. Under
//! `config.ui = "managed"` it also serves the configuration revisions under
//! `/admin/config` ([`revision::routes`], [`managed_router`]), whose reads
//! pass [`operator_read`]. Every other path is `404`. No route is on the
//! gateway's client listener, and none is part of the ITS-REST surface.
//!
//! Every write or administrative action, the distribution, the clear, the export and the reload among them, is
//! admitted by the gateway's client authentication ([`Gate`](crate::auth::Gate), built from
//! `[auth]`) only for a caller whose token carries `[auth] product_scope`,
//! from any peer ([`operator_only`]). Under `profile = "development"` a
//! loopback peer that presents no credential is admitted as well. `GET
//! /metrics` is open unless `[metrics] scrape_users` names Basic users, and
//! then answers only a scrape that authenticates as one of them with HTTP
//! Basic, checked against `[[auth.basic.users]]` ([`scrape`]). Every refusal
//! is an [`AdminRefusal`], and every write action that ran is recorded with
//! its [`Operator`].
// NOTE: no specification governs this: our own design; §12.7 gives drift repair
// no request, so the gateway offers it to the operator only, beside the metrics.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::rejection::ExtensionRejection;
use axum::extract::{MatchedPath, Path, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::serve::{Listener, ListenerExt};
use axum::{Json, Router};
use ferrofed_identity::dev::Profile;
use ferrofed_registry::export::IdentityStateExport;
use ferrofed_registry::id::EhrId;
use http::{HeaderMap, HeaderValue, StatusCode, header};

use crate::auth::caller::Caller;
use crate::auth::permission::Requirement;
use crate::auth::refusal::{BasicRefusal, Refused};
use crate::auth::trust::Trust;
use crate::config::auth::AuthSettings;
use crate::config::settings::Settings;
use crate::error::{self, Code};
use crate::facade::stored;
use crate::listener::TlsListener;
use crate::listener::certificates::Certificates;
use crate::metrics;
use crate::metrics::security::Event;
use crate::reload::{self, Reloader};
use crate::request_id;
use crate::revision;
use crate::revision::routes::Managed;
use crate::state::AppState;

/// The path of the operator's distribution of a held stored-query version:
/// the qualified name and the `major.minor.patch` version, as ITS-REST's
/// Definition API spells them.
pub const DISTRIBUTE: &str = "/admin/stored-queries/{qualified_query_name}/{version}/distribute";

/// The path of the operator's clear of one `ehr_id`, a node-local
/// identifier, from this process's `ehr_id` index and resolution bindings.
pub const CLEAR_EHR_ID: &str = "/admin/ehr-ids/{ehr_id}/clear";

/// The path of the operator's export of the routing state this process
/// learned, for a system that replaces the gateway.
pub const EXPORT_IDENTITY_STATE: &str = "/admin/identity-state/export";

/// The path of the operator's reload of the configuration file, the same
/// reload `SIGHUP` asks for ([`Reloader::reload`]).
pub const RELOAD: &str = "/admin/reload";

/// Why the admin listener refused a request before its route read anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AdminRefusal {
    /// Client authentication refused the caller of a write action: no
    /// credential, one that does not verify, or a token without the
    /// product scope.
    #[error("client authentication refused the caller of an admin action: {}", .0.refusal.description())]
    Caller(Refused),
    /// A scrape of `GET /metrics` carries no Basic credential of a user in
    /// `[metrics] scrape_users`.
    #[error("the scrape carries no Basic credential of a scrape user: {}", .0.reason())]
    Scrape(BasicRefusal),
}

impl AdminRefusal {
    /// The security event the refusal is counted as.
    #[must_use]
    pub const fn event(self) -> Event {
        match self {
            Self::Caller(_) => Event::AdminWriteRefused,
            Self::Scrape(_) => Event::ScrapeRefused,
        }
    }

    /// The reason the security log names.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Caller(refused) => refused.refusal.reason(),
            Self::Scrape(refusal) => refusal.reason(),
        }
    }

    /// The answer to the refused request, its challenge naming `realm`: the
    /// gate's answer for a caller, and the Basic challenge of RFC 7617 for a
    /// scrape.
    #[must_use]
    pub fn response(self, realm: &str, request_id: &str) -> Response {
        match self {
            Self::Caller(refused) => refused.refusal.response(realm, request_id),
            Self::Scrape(refusal) => refusal.response(realm, request_id),
        }
    }
}

/// The operator a write action was admitted for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Operator {
    /// A caller the gate verified, with the operator scope.
    Verified(Box<Caller>),
    /// A loopback peer that sent no credential, admitted under
    /// `profile = "development"` alone.
    DevelopmentLoopback,
}

impl Operator {
    /// The issuer that vouched for the operator, when one did.
    #[must_use]
    pub fn issuer(&self) -> Option<&str> {
        match self {
            Self::Verified(caller) => Some(caller.issuer()),
            Self::DevelopmentLoopback => None,
        }
    }

    /// The operator's subject at that issuer, when one vouched for it.
    #[must_use]
    pub fn subject(&self) -> Option<&str> {
        match self {
            Self::Verified(caller) => Some(caller.subject()),
            Self::DevelopmentLoopback => None,
        }
    }
}

/// Who the admin listener admits: the verifier of `[auth]`, the deployment
/// profile, and the Basic users a scrape may authenticate as.
#[derive(Debug)]
pub struct Access {
    /// The verifier of every write action's caller and every scrape,
    /// replaced whole on a reload.
    trust: Arc<Trust>,
    /// The deployment profile.
    profile: Profile,
    /// The Basic users a scrape may authenticate as; empty leaves the
    /// scrape open.
    scrape_users: Vec<String>,
}

impl Access {
    /// Returns the access that verifies a write action's caller as `auth`
    /// describes, under `profile`, with `scrape_users` guarding the scrape.
    #[must_use]
    pub fn new(auth: &AuthSettings, profile: Profile, scrape_users: Vec<String>) -> Self {
        Self::sharing(Arc::new(Trust::new(auth)), profile, scrape_users)
    }

    /// Returns the access that verifies a write action's caller with the
    /// gate `trust` holds when the request arrives, the one the client
    /// listener shares, so a reload of `[auth]` reaches both.
    #[must_use]
    pub fn sharing(trust: Arc<Trust>, profile: Profile, scrape_users: Vec<String>) -> Self {
        Self {
            trust,
            profile,
            scrape_users,
        }
    }

    /// Returns the access `settings` describe: `[auth]`, the profile, and
    /// `[metrics] scrape_users`.
    #[must_use]
    pub fn of(settings: &Settings) -> Self {
        Self::new(
            &settings.server.auth,
            settings.profile,
            settings.metrics.scrape_users.clone(),
        )
    }

    /// Returns the realm every challenge of the gate in place names.
    fn realm(&self) -> String {
        self.trust.gate().realm().to_owned()
    }

    /// Admits the caller of a write action with `headers` from `peer`, and
    /// returns the operator it admitted.
    ///
    /// # Errors
    /// Returns [`AdminRefusal::Caller`] with the gate's refusal.
    pub async fn admit_operator(
        &self,
        peer: Option<SocketAddr>,
        headers: &HeaderMap,
    ) -> Result<Operator, AdminRefusal> {
        let gate = self.trust.gate();
        // NOTE: no specification governs this: our own design; development keeps the
        // credential-less loopback operator, and every other profile asks the gate.
        if self.profile == Profile::Development
            && !gate.presents_credential(headers)
            && peer.is_some_and(|peer| peer.ip().is_loopback())
        {
            return Ok(Operator::DevelopmentLoopback);
        }
        gate.admit(headers, (Requirement::Operator, None), false)
            .await
            .map(|caller| Operator::Verified(Box::new(caller)))
            .map_err(AdminRefusal::Caller)
    }

    /// Admits a scrape with `headers`: any, unless scrape users are named,
    /// and then one whose Basic credential is one of theirs.
    ///
    /// # Errors
    /// Returns [`AdminRefusal::Scrape`] with the gate's refusal of the Basic
    /// credential.
    pub async fn admit_scrape(&self, headers: &HeaderMap) -> Result<(), AdminRefusal> {
        if self.scrape_users.is_empty() {
            return Ok(());
        }
        self.trust
            .gate()
            .admit_basic(headers, &self.scrape_users)
            .await
            .map(|_user| ())
            .map_err(AdminRefusal::Scrape)
    }
}

/// Returns the admin listener's application over `state`, admitting as
/// `access` says.
///
/// It serves `GET /metrics`, `POST` [`DISTRIBUTE`], `POST` [`CLEAR_EHR_ID`]
/// and `POST` [`EXPORT_IDENTITY_STATE`], and `404` on every other path.
///
/// A development profile admits a loopback peer to the write actions
/// without a credential; the peer is the [`ConnectInfo<SocketAddr>`] the
/// server records, so the application is served with
/// `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn router(state: Arc<AppState>, access: Access) -> Router {
    managed_router(state, access, None)
}

/// Returns the admin listener's application with the revision routes of `managed`.
///
/// It is [`router`] with the configuration revisions of [`revision::routes`]
/// beside it when `managed` is set (`config.ui = "managed"`); without it, as
/// under `config.ui = "off"`, no revision route exists and each of their
/// paths is `404`.
pub fn managed_router(
    state: Arc<AppState>,
    access: Access,
    managed: Option<Arc<Managed>>,
) -> Router {
    serving_router(state, access, managed, None)
}

/// Returns the admin listener's application as [`managed_router`] does,
/// with `POST` [`RELOAD`] and `GET` [`crate::running::RUNNING`] beside it
/// when `reloader` is set.
///
/// The reload is a write action: admitted for the operator scope and
/// recorded with its operator ([`operator_only`]); the running
/// configuration is a read ([`operator_read`]). Without `reloader` both
/// paths are `404`.
pub fn serving_router(
    state: Arc<AppState>,
    access: Access,
    managed: Option<Arc<Managed>>,
    reloader: Option<Arc<Reloader>>,
) -> Router {
    let access = Arc::new(access);
    let metrics = metrics::routes(Arc::clone(state.metrics()))
        .route_layer(middleware::from_fn_with_state(Arc::clone(&access), scrape));
    let mut app = Router::new()
        .route(DISTRIBUTE, post(distribute))
        .route(CLEAR_EHR_ID, post(clear_ehr_id))
        .route(EXPORT_IDENTITY_STATE, post(export_identity_state))
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&access),
            operator_only,
        ))
        .with_state(state)
        .merge(metrics);
    if let Some(reloader) = reloader {
        let running = crate::running::routes(Arc::clone(&reloader)).route_layer(
            middleware::from_fn_with_state(Arc::clone(&access), operator_read),
        );
        app = app.merge(running);
        let reloading = Router::new()
            .route(RELOAD, post(reload::action::reload))
            .route_layer(middleware::from_fn_with_state(
                Arc::clone(&access),
                operator_only,
            ))
            .with_state(reloader);
        app = app.merge(reloading);
    }
    if let Some(managed) = managed {
        let writes = revision::routes::writes(Arc::clone(&managed)).route_layer(
            middleware::from_fn_with_state(Arc::clone(&access), operator_only),
        );
        let reads = revision::routes::reads(managed)
            .route_layer(middleware::from_fn_with_state(access, operator_read));
        app = app.merge(writes).merge(reads);
    } else {
        // NOTE: no specification governs this: our own design; a console tells an
        // unmanaged gateway from a wrong admin URL by this code, not by a bare 404.
        let unmanaged = Router::new()
            .route(revision::routes::CONFIG, get(unmanaged))
            .route_layer(middleware::from_fn_with_state(access, operator_read));
        app = app.merge(unmanaged);
    }
    app.fallback(|| async { StatusCode::NOT_FOUND })
}

/// Returns the address and the application of the admin listener
/// `settings` describe over `state`, or `None` when `[metrics] listen` is
/// unset, so no admin route exists.
#[must_use]
pub fn listener(settings: &Settings, state: &Arc<AppState>) -> Option<(SocketAddr, Router)> {
    managed_listener(settings, state, None)
}

/// Returns the address and the application of the admin listener as
/// [`listener`] does, with the configuration revisions `managed` serves
/// ([`managed_router`]).
#[must_use]
pub fn managed_listener(
    settings: &Settings,
    state: &Arc<AppState>,
    managed: Option<Arc<Managed>>,
) -> Option<(SocketAddr, Router)> {
    settings.metrics.listen.map(|address| {
        let app = managed_router(Arc::clone(state), Access::of(settings), managed);
        (address, app)
    })
}

/// Binds the admin listener at `address` and serves `app` on it, plain or
/// over `tls`, in a task of its own, logging the address it bound.
///
/// # Errors
/// Returns the I/O error of binding `address` or of reading the address
/// bound.
pub async fn start(
    address: SocketAddr,
    app: Router,
    tls: Option<Arc<Certificates>>,
) -> std::io::Result<()> {
    let bound = tokio::net::TcpListener::bind(address).await?;
    // NOTE: no specification governs this: our own design; the bound address, so a
    // listener configured on port 0 names the port the system gave it.
    let local = bound.local_addr()?;
    tracing::info!(
        listen = %local,
        tls = tls.is_some(),
        path = metrics::PATH,
        distribute = DISTRIBUTE,
        clear_ehr_id = CLEAR_EHR_ID,
        export_identity_state = EXPORT_IDENTITY_STATE,
        reload = RELOAD,
        "serving the admin listener"
    );
    tokio::spawn(async move {
        let stopped = match tls {
            None => serve(bound, app).await,
            Some(certificates) => match TlsListener::new(bound, certificates) {
                Ok(tls) => serve(tls, app).await,
                Err(error) => Err(error),
            },
        };
        if let Err(error) = stopped {
            tracing::error!(%error, "the metrics listener stopped");
        }
    });
    Ok(())
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
/// action reads anything. An admitted action is recorded before it runs,
/// and its outcome once it has answered or was abandoned.
pub async fn operator_only(
    State(access): State<Arc<Access>>,
    peer: Result<ConnectInfo<SocketAddr>, ExtensionRejection>,
    mut request: Request,
    next: Next,
) -> Response {
    // NOTE: no specification governs this: our own design; a peer the server did
    // not record is read as unknown, so it is never the loopback operator.
    let peer = peer.ok().map(|ConnectInfo(peer)| peer);
    match access.admit_operator(peer, request.headers()).await {
        Ok(operator) => {
            let template = request
                .extensions()
                .get::<MatchedPath>()
                .map_or("unmatched", MatchedPath::as_str);
            let action = format!("{} {template}", request.method());
            request.extensions_mut().insert(operator.clone());
            let mut running = Running::begin(operator, action);
            let response = next.run(request).await;
            running.answered(response.status());
            response
        }
        Err(refusal) => refuse(&access, refusal, request.headers()),
    }
}

/// The middleware in front of every read of the configuration revisions.
///
/// A caller the gate admits with the operator scope passes, as in front of
/// a write action ([`Access::admit_operator`]), and the read is not
/// recorded as an action.
pub async fn operator_read(
    State(access): State<Arc<Access>>,
    peer: Result<ConnectInfo<SocketAddr>, ExtensionRejection>,
    request: Request,
    next: Next,
) -> Response {
    let peer = peer.ok().map(|ConnectInfo(peer)| peer);
    match access.admit_operator(peer, request.headers()).await {
        Ok(_operator) => next.run(request).await,
        Err(refusal) => refuse(&access, refusal, request.headers()),
    }
}

/// An admitted write action, recorded under `ferrofed::security` before it
/// has any effect, whose outcome is recorded when it is dropped.
///
/// Each record carries the operator's issuer and subject, the method and
/// route template of the action, the outcome on the closing record, and the
/// time, and nothing else: no credential, header, path value or body. An
/// action dropped before it answered, its client gone or its handler
/// panicking, closes with the outcome `abandoned`. The subject is the one
/// the issuer put in the token, which may name a person; it is kept as
/// issued, because a per-process keyed hash could not attribute an action
/// once the process restarts.
#[derive(Debug)]
struct Running {
    /// The operator who runs the action.
    operator: Operator,
    /// The method and the route template.
    action: String,
    /// The status the action was answered, once it was.
    status: Option<StatusCode>,
}

impl Running {
    /// Counts and records `operator` beginning `action`.
    fn begin(operator: Operator, action: String) -> Self {
        Event::AdminWriteAdmitted.record();
        // NOTE: no specification governs this: our own design; tracing returns no error,
        // so the record cannot refuse the action, and the security target is never filtered.
        tracing::info!(
            target: crate::facade::security::TARGET,
            event = Event::AdminWriteAdmitted.as_str(),
            issuer = operator.issuer(),
            subject = operator.subject(),
            action = action.as_str(),
            at = %jiff::Timestamp::now(),
            "an operator began a write action on the admin listener"
        );
        Self {
            operator,
            action,
            status: None,
        }
    }

    /// Notes the `status` the action was answered.
    fn answered(&mut self, status: StatusCode) {
        self.status = Some(status);
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let outcome = self.status.map_or_else(
            || String::from("abandoned"),
            |status| status.as_u16().to_string(),
        );
        tracing::info!(
            target: crate::facade::security::TARGET,
            event = FINISHED,
            issuer = self.operator.issuer(),
            subject = self.operator.subject(),
            action = self.action.as_str(),
            outcome,
            at = %jiff::Timestamp::now(),
            "an operator's write action on the admin listener ended"
        );
    }
}

/// The `event` of the record that closes an admitted write action.
pub const FINISHED: &str = "admin-write-finished";

/// The middleware in front of `GET /metrics`: a scrape passes unless
/// scrape users are named and the scrape authenticates as none of them
/// ([`Access::admit_scrape`]).
pub async fn scrape(State(access): State<Arc<Access>>, request: Request, next: Next) -> Response {
    match access.admit_scrape(request.headers()).await {
        Ok(()) => next.run(request).await,
        Err(refusal) => refuse(&access, refusal, request.headers()),
    }
}

/// Counts, logs and answers `refusal` of a request with `headers`.
fn refuse(access: &Access, refusal: AdminRefusal, headers: &HeaderMap) -> Response {
    refusal.event().record();
    let check = match refusal {
        AdminRefusal::Caller(refused) => refused
            .check
            .map(ferrohealth_smart::refusal::Reason::as_str),
        AdminRefusal::Scrape(_) => None,
    };
    tracing::warn!(
        target: crate::facade::security::TARGET,
        event = refusal.event().as_str(),
        reason = refusal.reason(),
        check,
        "a request to the admin listener was refused at its authentication"
    );
    refusal.response(&access.realm(), request_id::of(headers).unwrap_or_default())
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

/// `GET` [`revision::routes::CONFIG`] under `config.ui = "off"`: `404`
/// (`configuration-unmanaged`), so a caller learns the gateway serves no
/// configuration revisions rather than that the path is unknown.
async fn unmanaged(headers: HeaderMap) -> Response {
    error::fixed(
        Code::ConfigurationUnmanaged,
        request_id::of(&headers).unwrap_or_default(),
    )
}

/// The message of a clear that carries a body.
const CLEAR_TAKES_NO_BODY: &str =
    "the clear of an ehr_id takes no body: it names the ehr_id in the path alone";

/// `POST` [`CLEAR_EHR_ID`]: forgets the path `ehr_id` from this process's
/// `ehr_id` index and from the resolution bindings of every session, and
/// answers `200` with the
/// [`ClearedEhrId`](ferrofed_registry::operator::ClearedEhrId) saying what was held.
///
/// The action takes no `body`. An `ehr_id` this process holds nothing for
/// answers `200` too, with `held` saying `nothing`: each replica holds its
/// own routing state, so another replica may still hold it. It is refused
/// with `400` (`ehr-id-invalid`) for a path value that is no
/// `HIER_OBJECT_ID`, `400` (`body-invalid`) for a body, and `501` when the
/// gateway runs without a registry.
// NOTE: §12b.2, §18: remedying a collision is the federation's governance, and
// only the operator can observe a node's remedy, so the clear is theirs alone.
async fn clear_ehr_id(
    State(state): State<Arc<AppState>>,
    Path(ehr_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    if !body.is_empty() {
        return error::response(Code::BodyInvalid, CLEAR_TAKES_NO_BODY, request_id);
    }
    let Ok(ehr_id) = EhrId::new(ehr_id) else {
        return error::fixed(Code::EhrIdInvalid, request_id);
    };
    let Some(federation) = state.federation() else {
        return error::fixed(Code::NotImplemented, request_id);
    };
    Json(federation.clear_ehr_id(&ehr_id)).into_response()
}

/// The message of an export that carries a body.
const EXPORT_TAKES_NO_BODY: &str = "the export of the identity state takes no body";

/// `POST` [`EXPORT_IDENTITY_STATE`]: answers `200` with the
/// [`IdentityStateExport`] of this process, marked `Cache-Control: no-store`.
///
/// The action takes no `body`. It is refused with `400` (`body-invalid`)
/// for a body, and `501` when the gateway runs without a registry. Each
/// replica learns its own maps, so the export names the replica
/// (`server.replica`).
// NOTE: Regulation (EU) 2025/327 Annex II 2.6; a POST, so the export is admitted and
// recorded with its operator as every admin action is: our own design.
async fn export_identity_state(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    if !body.is_empty() {
        return error::response(Code::BodyInvalid, EXPORT_TAKES_NO_BODY, request_id);
    }
    let Some(federation) = state.federation() else {
        return error::fixed(Code::NotImplemented, request_id);
    };
    let export: IdentityStateExport = federation.export_identity_state(state.replica());
    (
        [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Json(export),
    )
        .into_response()
}
