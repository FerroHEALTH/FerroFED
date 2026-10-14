// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission of a request on the FHIR face, which every route that
//! reaches a member makes the same way.
//!
//! [`admit`] reads the request's ids, holds the face to a configured
//! federation and holds the caller to the one the gate admitted;
//! [`Admitted::confine`] conveys the caller and confines the request
//! (§13.1, N24); [`Admitted::arrived`] is the request the façade runs. A
//! route that checks something of its own before any member is reached does
//! so between the first two. No specification governs this split: our own
//! design.

use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::response::Response;
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::session::SessionKey;
use http::HeaderMap;

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
        let conveyance = conveyed::of(&self.federation, caller)
            .map_err(|unconveyed| unconveyed.respond(self.request_id, &self.logged))?;
        let on_behalf = self.caller.on_behalf();
        let conveyance =
            crate::facade::confined_by(&self.federation, caller, self.started, conveyance)
                .await
                .map_err(|unconfined| unconfined.respond(self.request_id, &self.logged))?;
        Ok(Confined {
            conveyance,
            on_behalf,
            session: self.caller.session(),
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
        }
    }
}
