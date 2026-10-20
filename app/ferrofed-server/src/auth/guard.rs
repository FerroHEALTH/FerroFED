// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The middleware every route of the client listener passes, and what each
//! request it guards requires.
//!
//! The middleware takes the gate in place once per request ([`Trust`]), so
//! a reload of `[auth]` reaches the requests that arrive after it and never
//! one that is being admitted.

use std::sync::Arc;

use axum::extract::{OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use ferrofed_engine::outbound_id::OutboundId;
use http::Method;
use openehr_its::rest::routes::{self, Lookup};

use crate::ITS_REST_PREFIX;
use crate::auth::caller::VerifiedBy;
use crate::auth::permission::{self, Requirement, Resource};
use crate::auth::trust::Trust;
use crate::base_path::BasePath;
use crate::facade::security::TARGET;
use crate::request_id;
use crate::state::AppState;

/// The gate and where it stands: the middleware's state.
#[derive(Debug)]
pub struct Guard {
    /// The verifier in place, replaced whole on a reload.
    trust: Arc<Trust>,
    /// The base path every guarded route sits under.
    base: BasePath,
    /// The state whose running federation says whether any node exchanges
    /// a caller's token.
    state: Arc<AppState>,
}

impl Guard {
    /// Returns the guard of the surface under `base`, verifying with the
    /// gate `trust` holds when each request arrives, keeping a caller's
    /// token only while `state`'s federation exchanges tokens at some node.
    #[must_use]
    pub fn new(trust: Arc<Trust>, base: BasePath, state: Arc<AppState>) -> Self {
        Self { trust, base, state }
    }

    /// Whether the running federation exchanges a caller's token at any
    /// node (RFC 8693), so the gate keeps the token it verified.
    fn exchanging(&self) -> bool {
        self.state
            .federation()
            .is_some_and(|federation| federation.clients().exchanging())
    }

    /// What a request of `method` to `path` requires, with the resource its
    /// path names, or `None` for a path outside the gate.
    #[must_use]
    pub fn requirement(
        &self,
        method: &Method,
        path: &str,
    ) -> Option<(Requirement, Option<String>)> {
        if let Some(face) = self
            .state
            .fhir()
            .and_then(|fhir| crate::fhir::classify(fhir, path))
        {
            return Some((crate::fhir::requirement(face, method), None));
        }
        let root = path == self.base.as_str() || path == self.base.join("/");
        if root && method == Method::OPTIONS {
            return Some((Requirement::Caller, None));
        }
        if crate::operator::addresses(&self.base, path) {
            return Some((Requirement::Operator, None));
        }
        let prefix = self.base.join(ITS_REST_PREFIX.trim_end_matches('/'));
        let relative = path
            .strip_prefix(prefix.as_str())
            .filter(|relative| relative.starts_with('/'))?;
        if method == Method::OPTIONS {
            return Some((Requirement::Caller, None));
        }
        let Lookup::Matched(matched) = routes::lookup(method, relative) else {
            return Some((Requirement::Caller, None));
        };
        // NOTE: no specification governs this: our own design; the table lists
        // every operation (a test holds it so), and an unlisted one is refused.
        let requirement = permission::of(&matched).unwrap_or(Requirement::Refused);
        let named = match requirement {
            Requirement::Scope {
                resource: Resource::Path(name),
                ..
            } => matched
                .path_param(name)
                .and_then(|param| param.decoded().ok()),
            _ => None,
        };
        Some((requirement, named))
    }
}

/// The middleware every route of the gateway passes: a request inside the
/// gate is admitted with its [`Caller`](crate::auth::caller::Caller) in its
/// extensions, or refused.
///
/// Every request also carries the national contact points of the gate it
/// took, which `OPTIONS {base}/` declares.
pub async fn guard(State(guard): State<Arc<Guard>>, mut request: Request, next: Next) -> Response {
    let gate = guard.trust.gate();
    request
        .extensions_mut()
        .insert(Arc::clone(gate.contact_points()));
    let path = request
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| request.uri().path(), |original| original.path())
        .to_owned();
    let Some((requirement, named)) = guard.requirement(request.method(), &path) else {
        return next.run(request).await;
    };
    let outbound = request_id::outbound(request.extensions()).map(|id: OutboundId| id.to_string());
    match gate
        .admit(
            request.headers(),
            (requirement, named.as_deref()),
            guard.exchanging(),
        )
        .await
    {
        Ok(caller) => {
            if caller.verified_by() == VerifiedBy::Edge {
                gate.references.edge_asserted(&caller, outbound.as_deref());
            }
            request.extensions_mut().insert(caller);
            next.run(request).await
        }
        Err(refused) => {
            let refusal = refused.refusal;
            crate::metrics::security::refused(refusal);
            tracing::warn!(
                target: TARGET,
                event = "caller-refused",
                reason = refusal.reason(),
                check = refused.check.map(ferrohealth_smart::refusal::Reason::as_str),
                status = refusal.code().status().as_u16(),
                request_id = outbound,
                "a request was refused at client authentication"
            );
            let request_id = request_id::of(request.headers()).unwrap_or_default();
            refusal.response(gate.realm(), request_id)
        }
    }
}
