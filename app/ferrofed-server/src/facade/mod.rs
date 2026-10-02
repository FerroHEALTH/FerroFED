// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITS-REST façade: `POST {base}/v1/query/aql` answered as one federated
//! `RESULT_SET` over every member (§7, §9, §11; `docs/architecture.md`
//! sections 3 and 5).
//!
//! An unmodified openEHR client sends a §7.2 façade query and receives one
//! ITS-REST `RESULT_SET` with the rows of every member that answered and
//! `meta.federation` naming every endpoint (N1, N16, N17). The `query` group
//! is FerroFED's own handler over the generated DTOs, because the federated
//! `424` and `504` carry `meta.federation`, which the generated `ApiError`
//! cannot (decision A11).
//!
//! One request runs the pipeline of section 3: [`completeness`] reads the
//! completion strategy the request selects (§11.4), [`prefer`] reads the
//! client deadline that can shorten the budget (§11.5), [`intake`] types the query
//! parameters, the rewrite analyses the query and names the patient,
//! [`plan`] resolves the patient at every member and builds one node query
//! per member that knows them, the engine fans out under the budget and the
//! strategy, and [`cells`] builds each façade row with the subject columns
//! re-injected (N5).
//! A refused query is a `400` whose message locates the fault by byte range
//! and never quotes it (§5.4.3). Every strip, refusal and outbound-gate stop
//! is a [`security`] event, by position and never by value.

pub mod cells;
pub mod completeness;
pub mod intake;
pub mod plan;
pub mod prefer;
pub mod security;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use ferrofed_engine::fanout::{Budget, Completion, FanOutError, fan_out_within};
use ferrofed_identity::binding::SessionKey;
use http::{HeaderMap, HeaderValue, StatusCode};
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, Paging, analyse};
use openehr_its::rest::generated::common::Error as ItsError;
use openehr_its::rest::generated::query::{AdhocQueryExecute, ResultSet};

use crate::federation::Federation;
use crate::request_id;
use crate::state::AppState;

/// The route the federated query is served at, under the ITS-REST prefix.
pub const QUERY_AQL: &str = "/v1/query/aql";

/// `POST {base}/v1/query/aql`: the federated ad hoc query.
///
/// Without a federation, the gateway federates nothing and answers as the
/// unserved ITS-REST surface does.
pub async fn query_aql(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    let request_id = request_id::of(&headers).unwrap_or_default().to_owned();
    let Some(federation) = state.federation() else {
        return crate::body::error(StatusCode::NOT_IMPLEMENTED, "not_implemented", &request_id);
    };
    // TODO(#80): the authenticated client session the resolution bindings belong to.
    let session: Option<SessionKey> = None;
    let completion = match completeness::of(&headers, federation.best_effort()) {
        Ok(completion) => completion,
        Err(error) => return Failure::Completeness(error).into_response(),
    };
    let configured = federation.budget();
    let wait = prefer::wait(&headers);
    let budget = wait.map_or(configured, |wait| configured.shortened_to(wait));
    let query = Query {
        body: &body,
        completion,
        budget,
        started,
        request_id: &request_id,
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
        Err(failure) => failure.into_response(),
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
    /// A query parameter is not an AQL literal.
    #[error(transparent)]
    Parameter(#[from] intake::IntakeError),
    /// The query is refused before anything is dispatched.
    #[error(transparent)]
    Refused(#[from] Refusal),
    /// The fan-out could not be planned.
    #[error("the federated query could not be planned")]
    Plan(#[source] plan::TargetsError),
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

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Body
            | Self::Completeness(_)
            | Self::Parameter(_)
            | Self::Refused(_)
            | Self::Plan(plan::TargetsError::Patient(_)) => StatusCode::BAD_REQUEST,
            Self::Cells(_) => StatusCode::BAD_GATEWAY,
            Self::Plan(_) | Self::FanOut(_) | Self::Envelope(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        if status.is_server_error() {
            tracing::error!(error = %crate::chain(&self), "the federated query failed");
        }
        let message = self.to_string();
        (
            status,
            Json(ItsError {
                message,
                validation_errors: Vec::new(),
            }),
        )
            .into_response()
    }
}

/// One federated query, as the façade read it from the request.
#[derive(Debug, Clone, Copy)]
struct Query<'a> {
    /// The request body.
    body: &'a [u8],
    /// The completion strategy the request selects (§11.4).
    completion: Completion,
    /// The effective budget: the configured one, shortened by the client's
    /// `Prefer: wait` (§11.5).
    budget: Budget,
    /// When the request arrived, the instant the overall budget runs from.
    started: Instant,
    /// The request id, empty when the client sent none.
    request_id: &'a str,
    /// The client session the resolution bindings belong to.
    session: Option<&'a SessionKey>,
}

/// Runs one federated query and returns the status and the `RESULT_SET`.
///
/// Resolution and the fan-out share one overall budget, which runs from the
/// request's arrival, so the gateway answers within its declared budget
/// (§11.5). The `{node, ehr_id}` set a resolution produces is held as the
/// session's resolution bindings (§12.5.1 step 2, decision A20); without a
/// session there is nothing to scope them to, and none is held.
async fn federate(
    federation: &Federation,
    query: Query<'_>,
) -> Result<(StatusCode, ResultSet), Failure> {
    let Query {
        body,
        completion,
        budget,
        started,
        request_id,
        session,
    } = query;
    // NOTE: §5.4.3, the reader's message may quote the body, so a malformed
    // body is refused with a fixed message.
    let request: AdhocQueryExecute =
        serde_json::from_slice(body).map_err(|_quoted| Failure::Body)?;
    let parameters = intake::parameters(request.query_parameters.as_ref())?;
    let paging = Paging {
        offset: request.offset,
        fetch: request.fetch,
    };
    let analysis = analyse(&request.q, &parameters, paging, federation.context())
        .inspect_err(|refusal| security::refused(refusal, request_id))?;
    if let Analysis::Patient(query) = &analysis {
        security::stripped(query, request_id);
    }
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
    if let Some(session) = session {
        federation.bindings().record(
            session,
            Instant::now(),
            targets.resolved.iter().map(|(node, ehr_id)| (node, ehr_id)),
        );
    }
    let answer = fan_out_within(
        federation.clients(),
        federation.snapshot(),
        targets
            .plan
            .completing(completion)
            .ordered(analysis.order().clone()),
        budget,
        started,
        (!request_id.is_empty()).then_some(request_id),
    )
    .await
    .map_err(|error| {
        security::fan_out(&error, request_id);
        Failure::FanOut(error)
    })?;
    let mut status = answer.status();
    let mut result_set = answer
        .into_result_set(Some(request.q.clone()), Some(analysis.columns().to_vec()))
        .map_err(Failure::Envelope)?;
    // NOTE: decision A17, a cross-reference that could not answer fails the
    // query 424 under all-or-nothing; under best-effort it stays reported.
    if targets.resolution_failed
        && completion == Completion::AllOrNothing
        && status == StatusCode::OK
    {
        status = StatusCode::FAILED_DEPENDENCY;
    }
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
