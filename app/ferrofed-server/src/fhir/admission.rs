// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission of a request on the FHIR face, which every route that
//! reaches a member makes the same way.
//!
//! Every handler takes the request as [`Asked`] and answers it through
//! [`served`]: [`admit`] reads the request's ids, holds the face to a
//! configured federation and holds the caller to the one the gate admitted;
//! the route's own check of the admitted request runs before any member is
//! reached; [`Admitted::confine`] conveys the caller and confines the
//! request (§13.1, N24); [`Admitted::arrived`] is the request the façade
//! runs. No specification governs this split: our own design.

use std::convert::Infallible;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::extract::FromRequestParts;
use axum::response::Response;
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::session::SessionKey;
use http::HeaderMap;
use http::request::Parts;

use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::conveyed;
use crate::error::Code;
use crate::facade::request::Arrived;
use crate::federation::Federation;
use crate::request_id;
use crate::state::AppState;

/// A request on the FHIR face whose caller the gate admitted.
#[derive(Debug)]
pub(super) struct Admitted<'a> {
    /// The federation the request runs over.
    pub(super) federation: Arc<Federation>,
    /// The face's settings.
    pub(super) fhir: &'a Arc<FhirSettings>,
    /// The verified caller, admitted with the scopes that cover the route.
    pub(super) caller: &'a Caller,
    /// The gateway's id for the request.
    pub(super) outbound: OutboundId,
    /// The exchange id the answer names, empty when the client sent none.
    pub(super) request_id: &'a str,
    /// The gateway's id for the request, as the log names it.
    pub(super) logged: String,
    /// The request headers.
    headers: &'a HeaderMap,
    /// When the request arrived.
    started: Instant,
}

/// What a request carries to a member once it is confined: the caller's
/// conveyance, whom it is on behalf of, and the caller's session.
#[derive(Debug)]
pub(super) struct Confined {
    /// The conveyance, confined.
    conveyance: Conveyance,
    /// Whom the request is on behalf of.
    on_behalf: OnBehalfOf,
    /// The verified caller's session.
    session: SessionKey,
    /// The patient a contact point's treatment relationship confirmation
    /// binds, resolved at every member.
    trc: Option<crate::facade::trc::Binding>,
}

/// Admits a request on the FHIR face that arrived at `started` with
/// `headers`, from `caller`, or answers why not.
///
/// A deployment with no federation or no FHIR face answers `501`, and a
/// caller the gate did not admit with the scopes that cover the route the
/// answer `forbidden` gives, which each route words for itself.
///
/// # Errors
///
/// The [`Response`] the request is refused with.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(super) fn admit<'a>(
    state: &'a AppState,
    (outbound, caller): (Option<Extension<OutboundId>>, Option<&'a Caller>),
    (headers, started): (&'a HeaderMap, Instant),
    forbidden: impl FnOnce() -> Response,
) -> Result<Admitted<'a>, Response> {
    let outbound = outbound.map_or_else(OutboundId::mint, |Extension(id)| id);
    let request_id = request_id::of(headers).unwrap_or_default();
    let logged = outbound.to_string();
    let (Some(federation), Some(fhir)) = (state.federation(), state.fhir()) else {
        return Err(crate::error::fixed(Code::NotImplemented, request_id));
    };
    // NOTE: §13.1, N25: a request on the face is answered only for a caller the gate admitted
    // with the scopes that cover it, whatever path reached its handler.
    let Some(caller) = caller.filter(|caller| !caller.covering().is_empty()) else {
        return Err(forbidden());
    };
    Ok(Admitted {
        federation,
        fhir,
        caller,
        outbound,
        request_id,
        logged,
        headers,
        started,
    })
}

impl Admitted<'_> {
    /// Conveys the admitted caller and confines the request (§13.1, N24).
    ///
    /// # Errors
    ///
    /// The [`Response`] of a caller that cannot be conveyed or a request
    /// that cannot be confined.
    #[expect(
        clippy::result_large_err,
        reason = "the refusal is the handler's answer, returned at most once per request"
    )]
    pub(super) async fn confine(&self) -> Result<Confined, Response> {
        let caller = Some(self.caller);
        let conveyance = conveyed::of(self.federation.signer(), caller)
            .map_err(|unconveyed| unconveyed.respond(self.request_id, &self.logged))?;
        let on_behalf = self.caller.on_behalf();
        let conveyance =
            crate::facade::confined_by(&self.federation, caller, self.started, conveyance)
                .await
                .map_err(|unconfined| unconfined.respond(self.request_id, &self.logged))?;
        let deadline = self
            .started
            .checked_add(self.federation.budget().overall())
            .ok_or_else(|| crate::error::fixed(Code::Internal, self.request_id))?;
        // NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: the TRC's patient is resolved before any
        // member is asked, so every read and write of the request is held to it.
        let trc = crate::facade::trc::bind(&self.federation, self.caller, deadline)
            .await
            .map_err(|unconfined| unconfined.respond(self.request_id, &self.logged))?;
        Ok(Confined {
            conveyance,
            on_behalf,
            session: self.caller.session(),
            trc,
        })
    }

    /// The request the façade runs, carrying what `confined` holds.
    pub(super) fn arrived<'b>(&'b self, confined: &'b Confined) -> Arrived<'b> {
        Arrived {
            headers: self.headers,
            request_id: self.request_id,
            outbound: self.outbound,
            conveyance: &confined.conveyance,
            started: self.started,
            session: Some(&confined.session),
            requester: self.caller.requester(),
            on_behalf: &confined.on_behalf,
            trc: confined.trc.as_ref(),
        }
    }
}

/// What every request on the FHIR face brings its handler: the gateway's
/// state, the gateway's id of the request, the caller the gate verified and
/// the headers.
pub(super) struct Asked {
    /// The gateway's state.
    pub(super) state: Arc<AppState>,
    /// The gateway's id of the request, when the request layer minted one.
    pub(super) outbound: Option<Extension<OutboundId>>,
    /// The caller the gate verified, when it admitted one.
    pub(super) caller: Option<Extension<Caller>>,
    /// The request's headers.
    pub(super) headers: HeaderMap,
}

impl FromRequestParts<Arc<AppState>> for Asked {
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(Self {
            state: Arc::clone(state),
            outbound: parts.extensions.get::<OutboundId>().copied().map(Extension),
            caller: parts.extensions.get::<Caller>().cloned().map(Extension),
            headers: parts.headers.clone(),
        }))
    }
}

/// Admits, conveys and confines the request `asked`, whose handler began
/// at `started`, and answers it with `serve`
/// over what was admitted, the request the façade runs and what `held`
/// read of the admitted request before it was confined.
///
/// A caller the gate did not admit is answered as `forbidden` says, and a
/// request `held` breaks off with its answer, before anything is conveyed.
pub(super) async fn served<T>(
    (asked, started): (&Asked, Instant),
    forbidden: impl FnOnce() -> Response,
    held: impl FnOnce(&Admitted<'_>) -> ControlFlow<Response, T>,
    serve: impl AsyncFnOnce(&Admitted<'_>, Arrived<'_>, T) -> Response,
) -> Response {
    let ids = (asked.outbound, asked.caller.as_deref());
    let admitted = match admit(&asked.state, ids, (&asked.headers, started), forbidden) {
        Ok(admitted) => admitted,
        Err(refused) => return refused,
    };
    let kept = match held(&admitted) {
        ControlFlow::Continue(kept) => kept,
        ControlFlow::Break(refused) => return refused,
    };
    let confined = match admitted.confine().await {
        Ok(confined) => confined,
        Err(refused) => return refused,
    };
    let arrived = admitted.arrived(&confined);
    serve(&admitted, arrived, kept).await
}
