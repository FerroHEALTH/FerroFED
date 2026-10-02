// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITS-REST façade: `POST {base}/v1/query/aql` answered as one federated
//! `RESULT_SET` over every member (§7, §9, §11).
//!
//! An unmodified openEHR client sends a §7.2 façade query and receives one
//! ITS-REST `RESULT_SET` with the rows of every member that answered and
//! `meta.federation` naming every endpoint (N1, N16, N17). The `query` group
//! is FerroFED's own handler over the generated DTOs, because the federated
//! `424` and `504` carry `meta.federation`, which the generated `ApiError`
//! cannot (N37, §11.4).
//!
//! One request runs the reference flow of §4: [`completeness`] reads the
//! completion strategy the request selects (§11.4), [`dedup`] the dedup mode
//! (§10), [`prefer`] reads the
//! client deadline that can shorten the budget (§11.5), [`intake`] types the query
//! parameters, the rewrite analyses the query and names the patient,
//! [`plan`] resolves the patient at every member and builds one node query
//! per member that knows them, the engine fans out under the budget and the
//! strategy, and [`cells`] builds each façade row with the subject columns
//! re-injected (N5).
//! A refused query is a `400` whose message locates the fault by byte range
//! and never quotes it (§5.4.3), and whose body names the refusal's stable
//! code ([`crate::error`]). Every strip, refusal and outbound-gate stop is
//! a [`security`] event, by position and never by value.

pub mod cells;
pub mod completeness;
pub mod dedup;
pub mod intake;
pub mod plan;
pub mod prefer;
pub mod security;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use ferrofed_engine::fanout::{Budget, Completion, FanOutError, fan_out_within};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::binding::SessionKey;
use http::{HeaderMap, HeaderValue, StatusCode};
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, Paging, analyse};
use openehr_federation::dedup::DedupMode;
use openehr_its::rest::generated::query::{AdhocQueryExecute, ResultSet};

use crate::error::{self, Code};
use crate::federation::Federation;
use crate::request_id;
use crate::state::AppState;

/// The route the federated query is served at, under the ITS-REST prefix.
pub const QUERY_AQL: &str = "/v1/query/aql";

/// `POST {base}/v1/query/aql`: the federated ad hoc query.
///
/// Without a federation, the gateway federates nothing and answers as the
/// unserved ITS-REST surface does. Every node the query reaches receives the
/// request's [`OutboundId`], never the client's `x-request-id` (§5.4.1, N33);
/// the client's id names the request only in the answer.
pub async fn query_aql(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    let request_id = request_id::of(&headers).unwrap_or_default().to_owned();
    let outbound = outbound.map_or_else(OutboundId::mint, |Extension(id)| id);
    let Some(federation) = state.federation() else {
        return error::fixed(Code::NotImplemented, &request_id);
    };
    // TODO(#80): the authenticated client session the resolution bindings belong to.
    let session: Option<SessionKey> = None;
    let completion = match completeness::of(&headers, federation.best_effort()) {
        Ok(completion) => completion,
        Err(error) => return Failure::Completeness(error).respond(&request_id),
    };
    let dedup = match dedup::of(&headers) {
        Ok(mode) => mode,
        Err(error) => return Failure::Dedup(error).respond(&request_id),
    };
    let configured = federation.budget();
    let wait = prefer::wait(&headers);
    let budget = wait.map_or(configured, |wait| configured.shortened_to(wait));
    let query = Query {
        body: &body,
        completion,
        dedup,
        budget,
        started,
        outbound,
        session: session.as_ref(),
    };
    match federate(federation, query).await {
        Ok((status, result_set)) => {
            let mut response = (status, Json(result_set)).into_response();
            if let Some(applied) = wait.filter(|_| budget != configured) {
                applied_wait(&mut response, applied);
            }
            response
        }
        Err(failure) => failure.respond(&request_id),
    }
}

/// Names the `wait` that set the budget in `Preference-Applied` (RFC 7240
/// §3).
#[expect(
    clippy::expect_used,
    reason = "`wait=` followed by decimal digits is always a valid header value"
)]
fn applied_wait(response: &mut Response, wait: Duration) {
    let value = HeaderValue::try_from(format!("wait={}", wait.as_secs()))
        .expect("`wait=` and digits should be a valid header value");
    response
        .headers_mut()
        .insert(prefer::PREFERENCE_APPLIED, value);
}

/// Why a federated query has no `RESULT_SET` to answer with.
#[derive(Debug, thiserror::Error)]
enum Failure {
    /// The request body is not an ITS-REST `AdhocQueryExecute`.
    #[error("the request body is not an ITS-REST ad hoc query")]
    Body,
    /// The completeness header is refused.
    #[error(transparent)]
    Completeness(#[from] completeness::CompletenessError),
    /// The dedup header is refused.
    #[error(transparent)]
    Dedup(#[from] dedup::DedupError),
    /// A query parameter is not an AQL literal.
    #[error(transparent)]
    Parameter(#[from] intake::IntakeError),
    /// The query is refused before anything is dispatched.
    #[error(transparent)]
    Refused(#[from] Refusal),
    /// The fan-out could not be planned.
    #[error("the federated query could not be planned")]
    Plan(#[source] plan::TargetsError),
    /// Node selection left no registry member in scope, so the request
    /// resolves to no destination (§11.2, §11.3).
    #[error(
        "no registry member is in scope for this request, so it cannot be resolved to any destination (§11.2, §11.3)"
    )]
    NoDestination,
    /// The fan-out failed on the gateway's side.
    #[error("the federated query could not be dispatched")]
    FanOut(#[source] FanOutError),
    /// A node's rows do not match the query it was sent.
    #[error("a node answered rows that do not match the dispatched query")]
    Cells(#[source] cells::CellError),
    /// The envelope could not be encoded.
    #[error("the federated answer could not be encoded")]
    Envelope(#[source] openehr_federation::error::WireError),
}

impl Failure {
    /// The code of this failure, which names its status (§11.2).
    fn code(&self) -> Code {
        match self {
            Self::Body => Code::BodyInvalid,
            Self::Completeness(completeness::CompletenessError::NotOffered) => {
                Code::PartialUnsupported
            }
            Self::Completeness(
                completeness::CompletenessError::Repeated | completeness::CompletenessError::Value,
            ) => Code::CompletenessInvalid,
            Self::Dedup(_) => Code::DedupInvalid,
            Self::Parameter(_) => Code::ParameterInvalid,
            Self::Refused(refusal) => Code::Refused(refusal.into()),
            Self::Plan(plan::TargetsError::Patient(_)) => Code::PatientInvalid,
            Self::NoDestination => Code::NoDestination,
            // NOTE: §11.1, a node row the gateway cannot use is a node-error
            // at dispatch, so one reaching the cells is the gateway's fault.
            Self::Plan(_) | Self::FanOut(_) | Self::Cells(_) | Self::Envelope(_) => Code::Internal,
        }
    }

    /// The error answer of this failure, naming `request_id`.
    ///
    /// The message is the failure's display text, which locates a fault and
    /// never quotes the query, a parameter value or a header value (§5.4.3).
    fn respond(self, request_id: &str) -> Response {
        let code = self.code();
        if code.status().is_server_error() {
            tracing::error!(
                code = code.as_str(),
                error = %crate::chain(&self),
                "the federated query failed"
            );
        }
        error::response(code, self.to_string(), request_id)
    }
}

/// One federated query, as the façade read it from the request.
#[derive(Debug, Clone, Copy)]
struct Query<'a> {
    /// The request body.
    body: &'a [u8],
    /// The completion strategy the request selects (§11.4).
    completion: Completion,
    /// The dedup mode the request selects (§10).
    dedup: DedupMode,
    /// The effective budget: the configured one, shortened by the client's
    /// `Prefer: wait` (§11.5).
    budget: Budget,
    /// When the request arrived, the instant the overall budget runs from.
    started: Instant,
    /// The gateway's id of the request: the one every node receives and the
    /// security events record, never the client's.
    outbound: OutboundId,
    /// The client session the resolution bindings belong to.
    session: Option<&'a SessionKey>,
}

/// Analyses the façade query of `request` under the request's `completion`
/// and `dedup` mode, recording a refusal or a strip as a security event
/// (§5.4.3).
///
/// An aggregate recombined across nodes is refused under best-effort: it is
/// exactly correct only over every node in scope (§11.6.3), and the gateway
/// never serves an all-or-nothing answer to a request that asked for
/// `partial` (§11.4).
fn analysed(
    federation: &Federation,
    request: &AdhocQueryExecute,
    (completion, dedup): (Completion, DedupMode),
    request_id: &str,
) -> Result<Analysis, Failure> {
    let parameters = intake::parameters(request.query_parameters.as_ref())?;
    let paging = Paging {
        offset: request.offset,
        fetch: request.fetch,
    };
    let context = federation.context().clone().with_dedup(dedup);
    let analysis = analyse(&request.q, &parameters, paging, &context)
        .and_then(|analysis| {
            if completion == Completion::BestEffort {
                analysis.admit_best_effort()?;
            }
            Ok(analysis)
        })
        .inspect_err(|refusal| security::refused(refusal, request_id))?;
    if let Analysis::Patient(query) = &analysis {
        security::stripped(query, request_id);
    }
    Ok(analysis)
}

/// Runs one federated query and returns the status and the `RESULT_SET`.
///
/// Resolution and the fan-out share one overall budget, which runs from the
/// request's arrival, so the gateway answers within its declared budget
/// (§11.5). The `{node, ehr_id}` set a resolution produces is held as the
/// session's resolution bindings (§12.5.1 step 2); without a
/// session there is nothing to scope them to, and none is held.
async fn federate(
    federation: &Federation,
    query: Query<'_>,
) -> Result<(StatusCode, ResultSet), Failure> {
    let Query {
        body,
        completion,
        dedup,
        budget,
        started,
        outbound,
        session,
    } = query;
    let logged = outbound.to_string();
    let request_id = logged.as_str();
    // NOTE: §5.4.3, the reader's message may quote the body, so a malformed
    // body is refused with a fixed message.
    let request: AdhocQueryExecute =
        serde_json::from_slice(body).map_err(|_quoted| Failure::Body)?;
    let analysis = analysed(federation, &request, (completion, dedup), request_id)?;
    let deadline = started
        .checked_add(budget.overall())
        .ok_or(Failure::FanOut(FanOutError::Clock))?;
    // TODO(#70): pass the endpoints the directive names; #71 adds the header.
    let selection = plan::Selection::Undirected;
    let (targets, subject) = match &analysis {
        Analysis::Patient(query) => (
            plan::patient(
                federation.snapshot(),
                selection,
                federation.resolver(),
                query,
                deadline,
            )
            .await
            .map_err(Failure::Plan)?,
            Some(query.subject()),
        ),
        Analysis::Unscoped(query) => (
            plan::unscoped(federation.snapshot(), selection, query).map_err(Failure::Plan)?,
            None,
        ),
    };
    if targets.plan.has_no_destination() {
        return Err(Failure::NoDestination);
    }
    if let Some(session) = session {
        federation.bindings().record(
            session,
            Instant::now(),
            targets.resolved.iter().map(|(node, ehr_id)| (node, ehr_id)),
        );
    }
    let mut plan = targets
        .plan
        .completing(completion)
        .ordered(analysis.order().clone())
        .deduplicating(dedup);
    if let Some(recombination) = analysis.recombination() {
        plan = plan.recombining(recombination.clone());
    }
    let answer = fan_out_within(
        federation.clients(),
        federation.snapshot(),
        plan,
        budget,
        started,
        Some(outbound),
    )
    .await
    .map_err(|error| {
        security::fan_out(&error, request_id);
        Failure::FanOut(error)
    })?;
    let mut status = answer.status();
    // NOTE: no specification governs this (§11.3 covers only an answered lookup):
    // our own design, a cross-reference that could not answer fails the query
    // 424 under all-or-nothing; under best-effort it stays reported.
    if targets.resolution_failed
        && completion == Completion::AllOrNothing
        && status == StatusCode::OK
    {
        status = StatusCode::FAILED_DEPENDENCY;
    }
    let mut result_set = answer
        .into_result_set(Some(request.q.clone()), Some(analysis.columns().to_vec()))
        .map_err(Failure::Envelope)?;
    let rows = if status == StatusCode::OK {
        cells::reinject(
            std::mem::take(&mut result_set.rows),
            &targets.sources,
            subject,
        )
        .map_err(Failure::Cells)?
    } else {
        Vec::new()
    };
    result_set.rows = rows;
    Ok((status, result_set))
}

#[cfg(test)]
mod tests {
    use super::{Failure, cells, completeness, dedup, plan};
    use crate::error::Code;
    use ferrofed_engine::fanout::FanOutError;
    use ferrofed_identity::patient::PatientRefError;
    use http::StatusCode;
    use openehr_federation::aql::refusal::Refusal;

    #[test]
    fn each_failure_answers_its_code_and_status() {
        let table = [
            (Failure::Body, "body-invalid", StatusCode::BAD_REQUEST),
            (
                Failure::Completeness(completeness::CompletenessError::Value),
                "completeness-invalid",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Completeness(completeness::CompletenessError::Repeated),
                "completeness-invalid",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Completeness(completeness::CompletenessError::NotOffered),
                "partial-unsupported",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Dedup(dedup::DedupError::Value),
                "dedup-invalid",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Dedup(dedup::DedupError::Repeated),
                "dedup-invalid",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Refused(Refusal::OffsetUnsupported),
                "offset-unsupported",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Plan(plan::TargetsError::Patient(PatientRefError::EmptyNamespace)),
                "patient-invalid",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::FanOut(FanOutError::Clock),
                "internal",
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                Failure::Cells(cells::CellError::NoSubject),
                "internal",
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (failure, code, status) in table {
            let answered: Code = failure.code();
            assert_eq!(code, answered.as_str(), "{failure:?}");
            assert_eq!(status, answered.status(), "{failure:?}");
        }
    }
}
