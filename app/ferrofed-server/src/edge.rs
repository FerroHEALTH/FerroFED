// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the gateway decides inside the stack of `ferrohealth-server`: the
//! body of a panicked request and of each refusal the stack writes, and the
//! request log.
//!
//! A refusal and a panic are answered in the gateway's error shape, an
//! RFC 9457 problem naming the stable code and the request, and under
//! `{fhir-base}` as an `OperationOutcome` (FHIR R4 `http.html`). A refusal
//! of the overload bound or the caller rate is counted by its limit
//! ([`Limit`]). The panic message reaches neither the body nor the log
//! (§5.4.3). No specification governs the body shape: our own design.

use std::sync::Arc;

use axum::response::{IntoResponse as _, Response};
use ferrohealth_server::panic::PanicBody;
use ferrohealth_server::refusal::RefusalBody;
use ferrohealth_server::stack::StackPolicy;
use ferrohealth_telemetry::request_log::RequestLog;
use http::{StatusCode, Uri};

use crate::error::{self, Code};
use crate::metrics::nodes::Limit;
use crate::state::AppState;

/// The stack policy of the gateway over `state`, each request logged by
/// `request_log`.
#[must_use]
pub fn policy(state: &Arc<AppState>, request_log: RequestLog) -> StackPolicy {
    StackPolicy {
        panic_body: panic_body(Arc::clone(state)),
        refusal_body: refusal_body(Arc::clone(state)),
        request_log,
    }
}

/// The body of a panicked request: `500 internal`.
fn panic_body(state: Arc<AppState>) -> PanicBody {
    Arc::new(move |uri, request_id| {
        answered(
            &state,
            uri,
            (StatusCode::INTERNAL_SERVER_ERROR, Some(Code::Internal)),
            (request_id, ferrohealth_server::problem::INTERNAL),
        )
    })
}

/// The body of a refusal the stack writes: `429 rate-limited` and
/// `503 overloaded`, each counted by its limit, and a `408` or a `413` as
/// an `about:blank` problem.
fn refusal_body(state: Arc<AppState>) -> RefusalBody {
    Arc::new(move |uri, request_id, status, reason| {
        let code = match status {
            StatusCode::SERVICE_UNAVAILABLE => {
                state.metrics().shed(Limit::Concurrency);
                Some(Code::Overloaded)
            }
            StatusCode::TOO_MANY_REQUESTS => {
                state.metrics().shed(Limit::CallerRate);
                Some(Code::RateLimited)
            }
            _ => None,
        };
        answered(&state, uri, (status, code), (request_id, reason))
    })
}

/// The answer of `status` and its `code` for a request to `uri`: an
/// `OperationOutcome` under the FHIR face, the gateway's problem of `code`
/// elsewhere, and without a code an `about:blank` problem whose detail is
/// the stack's `reason`, a fixed sentence (RFC 9457 §4.2.1).
fn answered(
    state: &AppState,
    uri: &Uri,
    (status, code): (StatusCode, Option<Code>),
    (request_id, reason): (&str, &str),
) -> Response {
    match (state.fhir(), code) {
        (Some(fhir), _) if crate::fhir::classify(fhir, uri.path()).is_some() => {
            crate::fhir::errors::refused(
                status,
                code.map_or("the request was refused", Code::message),
            )
        }
        (_, Some(code)) => error::fixed(code, request_id),
        (_, None) => ferrohealth_server::problem::Problem::new(status)
            .with_detail(reason)
            .with_request_id(request_id)
            .into_response(),
    }
}
