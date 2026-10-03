// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Routing a request that has no owner to find: the creation of an EHR, a
//! definition request and a DEMOGRAPHIC request each go to one endpoint that
//! is named, never found (§7a.1, §12.4, §12.6, N23, N32, N43).
//!
//! The client names it in the targeting headers, or, for the DEMOGRAPHIC
//! area, the deployment configured it and a header may only confirm it.
//! Nothing is probed and no node is picked implicitly.

use std::time::Instant;

use axum::response::Response;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus, RegistrySnapshot};
use http::HeaderMap;
use openehr_its::rest::routes::RouteMatch;

use super::{Arrived, Deadlines, Failure, Provenance, answered, failed, forward, refused_carriers};
use crate::error::{self, Code};
use crate::facade::owner;
use crate::federation::Federation;

/// Who names the one node a request without an owner is routed to.
#[derive(Debug, Clone, Copy)]
pub(super) enum Chooser<'a> {
    /// The client, in the targeting headers (§8.4, §12.4, §12.6).
    Client,
    /// The deployment, which configured this endpoint for the area; the
    /// targeting headers may name it and no other (§7a.1, N32).
    Configured(&'a EndpointId),
}

/// Routes a request to the one endpoint `chooser` names, and answers as that
/// node did (§7a.1, §12.4, §12.6, N23, N32, N43); `area` names what was
/// routed in the debug event.
///
/// A new EHR has no owner for a binding or the index to name, and a template
/// or a stored query lives at the node it was sent to, so only the client
/// can name the node; a DEMOGRAPHIC request goes to the endpoint the
/// deployment configured. The query string and the declared values are
/// checked first, as on the EHR route ([`refused_carriers`]); then the
/// endpoint is [`chosen`]: nothing is probed and no node is picked
/// implicitly. The body is forwarded byte-identical, and the node's answer,
/// an error included, comes back as the node sent it with the acting
/// endpoint named; no two nodes' answers are combined (N22, N31, N33).
pub(super) async fn named(
    federation: &Federation,
    arrived: Arrived<'_>,
    matched: &RouteMatch,
    area: &'static str,
    chooser: Chooser<'_>,
) -> Response {
    let started = Instant::now();
    let request_id = arrived.request_id;
    let logged = arrived.outbound.to_string();
    if let Some(refused) = refused_carriers(matched, &arrived, &logged) {
        return refused;
    }
    let snapshot = federation.snapshot();
    let endpoint = match chosen(snapshot, arrived.headers, chooser, &logged) {
        Ok(endpoint) => endpoint,
        Err((code, message)) => return error::response(code, message, request_id),
    };
    if endpoint.status() == EndpointStatus::Suspended {
        return error::fixed(Code::NoDestination, request_id);
    }
    let Some(budget) = Deadlines::from(federation, started) else {
        tracing::error!(
            request_id = logged,
            "the routed request's deadline cannot be represented"
        );
        return error::fixed(Code::Internal, request_id);
    };
    tracing::debug!(
        endpoint = %endpoint.id(),
        area,
        request_id = logged,
        "routed to the endpoint the targeting headers name"
    );
    let provenance = Provenance::of(snapshot, endpoint);
    match forward(federation, endpoint, &arrived, &budget, &logged).await {
        Ok(forwarded) => provenance.stamp(answered(forwarded)),
        Err(Failure::Internal) => error::fixed(Code::Internal, request_id),
        Err(Failure::Forward(failure)) => failed(&failure, provenance, (request_id, &logged)),
    }
}

/// The one endpoint of `snapshot` that `chooser` and the targeting headers
/// of `headers` select together, or the code and the message refusing the
/// request (§8.4.1, §12.6).
///
/// For the client, the headers must name exactly one endpoint the registry
/// holds: none is `target-required`, several `endpoint-several`, and an
/// unknown id or `*` `endpoint-unknown`. For a configured endpoint, the
/// headers may name that endpoint or nothing; a header that names another
/// is `targeting-conflict`, naming both, and the header refusals above stand.
fn chosen<'a>(
    snapshot: &'a RegistrySnapshot,
    headers: &HeaderMap,
    chooser: Chooser<'_>,
    logged: &str,
) -> Result<&'a Endpoint, (Code, String)> {
    let targeted = owner::targeted(snapshot, headers)
        .map_err(|untargeted| (untargeted.code(), untargeted.to_string()))?;
    let configured = match (chooser, targeted) {
        (Chooser::Client, Some(endpoint)) => return Ok(endpoint),
        (Chooser::Client, None) => {
            let code = Code::TargetRequired;
            return Err((code, code.message().to_owned()));
        }
        (Chooser::Configured(configured), Some(endpoint)) if endpoint.id() == configured => {
            return Ok(endpoint);
        }
        // NOTE: §7a.1, N32: the deployment's configured endpoint is the explicit
        // choice, so a header may confirm it and never redirect the request (§8.4.1).
        (Chooser::Configured(configured), Some(endpoint)) => {
            let message = format!(
                "the targeting headers select the endpoint {}, and this area is routed \
                 to the configured endpoint {configured} alone (§7a.1, §8.4.1, N32)",
                endpoint.id()
            );
            return Err((Code::TargetingConflict, message));
        }
        (Chooser::Configured(configured), None) => configured,
    };
    snapshot.endpoint(configured).ok_or_else(|| {
        tracing::error!(
            endpoint = %configured,
            request_id = logged,
            "the configured endpoint left the snapshot"
        );
        (Code::Internal, Code::Internal.message().to_owned())
    })
}
