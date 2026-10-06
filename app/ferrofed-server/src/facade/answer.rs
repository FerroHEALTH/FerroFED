// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer to a federated query: its modes read from the request, the
//! patient resolved and the fan-out run under one budget, and the
//! `RESULT_SET` built with its provenance, or the error answer naming why
//! there is none (§4, §9, §11).
//!
//! A server error is logged under the gateway's own id, never the client's
//! free-text request id, and no answer quotes the query, a parameter value
//! or a header value (§5.4.3).

use std::time::{Duration, Instant};

use axum::Json;
use axum::response::{IntoResponse, Response};
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::fanout::{
    Budget, Completion, FanOutError, FederatedAnswer, Plan, fan_out_within,
};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::consent::Requester;
use ferrofed_identity::session::SessionKey;
use ferrofed_registry::id::EhrId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::{HeaderMap, HeaderValue, StatusCode};
use openehr_federation::aql::Analysis;
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::subject::Subject;
use openehr_federation::attribute::EndpointAttribute;
use openehr_federation::dedup::DedupMode;
use openehr_federation::outcome::ErrorDetail;
use openehr_its::rest::generated::query::ResultSet;
use openehr_its::rest::runtime::ApiError;
use tracing::Instrument as _;
use tracing::field::Empty;

use crate::access::{Accessed, NoAccess};
use crate::error::{self, Code};
use crate::facade::accessed;
use crate::facade::provenance::{Dispatch, Provenance};
use crate::facade::request::{Arrived, Submitted, read};
use crate::facade::{
    cells, completeness, confined, dedup, follow_up, intake, owner, plan, prefer, scoped, security,
    target,
};
use crate::federation::Federation;

/// Runs the federated query `submitted` and answers it (§7, §9, §11).
pub(crate) async fn answer(
    federation: &Federation,
    arrived: Arrived<'_>,
    submitted: Submitted<'_>,
) -> Response {
    let Arrived {
        headers,
        request_id,
        outbound,
        conveyance,
        started,
        session,
        requester,
        on_behalf,
    } = arrived;
    let completion = match completeness::of(headers, federation.best_effort()) {
        Ok(completion) => completion,
        Err(error) => return Failure::Completeness(error).respond(request_id, outbound),
    };
    let dedup = match dedup::of(headers) {
        Ok(mode) => mode,
        Err(error) => return Failure::Dedup(error).respond(request_id, outbound),
    };
    let configured = federation.budget();
    let wait = prefer::wait(headers);
    let budget = wait.map_or(configured, |wait| configured.shortened_to(wait));
    let name = match &submitted {
        Submitted::Body(_) | Submitted::Query { .. } => None,
        Submitted::Stored { name, .. } => Some((*name).to_owned()),
    };
    let query = Query {
        sent: submitted,
        headers,
        completion,
        dedup,
        budget,
        started,
        outbound,
        conveyance,
        session,
        requester,
        on_behalf,
    };
    match federate(federation, query).await {
        Ok((status, mut result_set, provenance, accessed)) => {
            // NOTE: §12.7, N44: the answer to a stored query names the gateway's definition.
            result_set.name = name;
            let mut response = (status, Json(result_set)).into_response();
            if let Some(applied) = wait.filter(|_| budget != configured) {
                applied_wait(&mut response, applied);
            }
            // NOTE: Regulation (EU) 2025/327 Annex II 3.2: the gate stores the record before
            // the answer leaves, and admits an answer with none only when no node was sent it.
            if let Some(accessed) = accessed {
                response.extensions_mut().insert(accessed);
            } else {
                response.extensions_mut().insert(NoAccess);
            }
            provenance.stamp(response)
        }
        Err(failure) => failure.respond(request_id, outbound),
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
pub(super) enum Failure {
    /// The request body is not an ITS-REST `AdhocQueryExecute`.
    #[error("the request body is not an ITS-REST ad hoc query")]
    Body,
    /// The query string is not an ITS-REST ad hoc query.
    // NOTE: §5.4.3, the decoder's message may name a query parameter the client
    // chose, so the answer carries a fixed message.
    #[error("the query string is not an ITS-REST ad hoc query")]
    Query(#[source] ApiError),
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
    /// The directive or a targeting header names what the registry does not
    /// know, or two of them select different node sets (§8.4.1).
    #[error(transparent)]
    Target(#[from] target::TargetError),
    /// The fan-out could not be planned.
    #[error("the federated query could not be planned")]
    Plan(#[source] plan::TargetsError),
    /// The query is scoped to one `ehr_id`, and the order of §12.5.1 names
    /// no one member that owns it.
    #[error(transparent)]
    Routed(#[from] scoped::Unrouted),
    /// The caller's grant is confined to one patient, and the query names no
    /// patient, or reaches a `{node, ehr_id}` pair outside that patient's
    /// (§5.2, §12.5).
    #[error(
        "the query reaches beyond the patient the access token's patient/ grant is confined to, or names no patient (§5.2, §12.5)"
    )]
    Confined,
    /// The caller's grant is confined to one patient, and the patient the
    /// query names could not be checked against it at the bound member
    /// (§5.2, §11.2).
    #[error(transparent)]
    Unconfirmed(confined::Unconfined),
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
            Self::Body | Self::Query(_) => Code::BodyInvalid,
            Self::Completeness(completeness::CompletenessError::NotOffered) => {
                Code::PartialUnsupported
            }
            Self::Completeness(
                completeness::CompletenessError::Repeated | completeness::CompletenessError::Value,
            ) => Code::CompletenessInvalid,
            Self::Dedup(_) => Code::DedupInvalid,
            Self::Parameter(_) => Code::ParameterInvalid,
            Self::Refused(refusal) => Code::Refused(refusal.into()),
            Self::Target(error) => error.code(),
            Self::Plan(plan::TargetsError::Patient(_)) => Code::PatientInvalid,
            Self::Routed(unrouted) => unrouted.code(),
            Self::Confined => Code::PatientConfinement,
            Self::Unconfirmed(_) => Code::PatientContextUnavailable,
            Self::NoDestination => Code::NoDestination,
            // NOTE: §11.1, a node row the gateway cannot use is a node-error
            // at dispatch, so one reaching the cells is the gateway's fault.
            Self::Plan(_) | Self::FanOut(_) | Self::Cells(_) | Self::Envelope(_) => Code::Internal,
        }
    }

    /// The error answer of this failure, naming the exchange id `request_id`.
    ///
    /// The message is the failure's display text, which locates a fault and
    /// never quotes the query, a parameter value or a header value (§5.4.3).
    /// A server error is logged under `outbound`, the id the request line
    /// records, and never under the client's free-text `request_id`.
    fn respond(self, request_id: &str, outbound: OutboundId) -> Response {
        let code = self.code();
        if code.status().is_server_error() {
            tracing::error!(
                code = code.as_str(),
                error = %crate::chain(&self),
                request_id = %outbound,
                "the federated query failed"
            );
        }
        error::response(code, self.to_string(), request_id)
    }
}

/// One federated query, as the façade read it from the request.
#[derive(Debug)]
struct Query<'a> {
    /// What the request sent.
    sent: Submitted<'a>,
    /// The request headers, which may name the node set (§8.4).
    headers: &'a HeaderMap,
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
    /// Whom the request is on behalf of, conveyed to every node (§13.1, N24).
    conveyance: &'a Conveyance,
    /// The client session the resolution bindings belong to.
    session: Option<&'a SessionKey>,
    /// Who asks for the data, as the verified caller's token states it.
    requester: Option<&'a Requester>,
    /// The verified caller every identity exchange is made for.
    on_behalf: &'a OnBehalfOf,
}

/// Drops the `session`'s bindings that name a member the consent pre-filter
/// denied (N27a), holds the `{node, ehr_id}` set a resolution produced as the
/// `session`'s resolution bindings (§12.5.1 step 2), teaches the `ehr_id` index where
/// each `ehr_id` is held (step 3), and records the state the resolution
/// showed of the resolver.
fn remember(federation: &Federation, session: Option<&SessionKey>, targets: &plan::Targets) {
    let resolved = &targets.resolved;
    if let Some(session) = session {
        // NOTE: N27a; a consent denial drops every `ehr_id` the session cached for a denied
        // member before the new bindings are held (no specification governs this: our own design).
        federation
            .bindings()
            .forget_denied(session, &targets.denied);
        federation.bindings().record(
            session,
            Instant::now(),
            resolved.iter().map(|(node, ehr_id)| (node, ehr_id)),
        );
    }
    for (node, ehr_id) in resolved {
        owner::learn(federation.index(), ehr_id, node);
    }
    if let Some(observed) = targets.resolver {
        federation.dependencies().resolver(observed);
    }
}

/// Records what a fan-out showed of each member it dispatched to: its last
/// state for the health surface, read from the node's own answer, and its
/// request for the metrics surface, read from its §11.1 record.
fn observed(federation: &Federation, answer: &FederatedAnswer) {
    let records = answer.federation().endpoints();
    for (endpoint, contact) in answer.contacts() {
        federation.dependencies().contacted(endpoint, contact);
        let record = records
            .iter()
            .find(|record| record.id().as_str() == endpoint.as_str());
        if let Some(record) = record {
            let outcome = answer.observed(endpoint).unwrap_or(record.outcome());
            federation.requests().settled(endpoint, outcome, contact);
        }
    }
}

/// Runs one federated query and returns the status, the `RESULT_SET`, and
/// the endpoints the answer names as having acted for it (§7a.3, N31).
///
/// Resolution and the fan-out share one overall budget, which runs from the
/// request's arrival, so the gateway answers within its declared budget
/// (§11.5). The `{node, ehr_id}` set a resolution produces is held as the
/// session's resolution bindings (§12.5.1 step 2); without a
/// session there is nothing to scope them to, and none is held.
async fn federate(
    federation: &Federation,
    query: Query<'_>,
) -> Result<(StatusCode, ResultSet, Provenance, Option<Accessed>), Failure> {
    let Query {
        sent,
        headers,
        completion,
        dedup,
        budget,
        started,
        outbound,
        conveyance,
        session,
        requester,
        on_behalf,
    } = query;
    let logged = outbound.to_string();
    let request_id = logged.as_str();
    let stored = sent.stored_name().map(str::to_owned);
    let (request, named, analysis) =
        read(federation, (sent, headers), (completion, dedup), request_id)?;
    let deadline = started
        .checked_add(budget.overall())
        .ok_or(Failure::FanOut(FanOutError::Clock))?;
    let caller = (conveyance, on_behalf);
    confine(federation, caller, &analysis, (deadline, request_id)).await?;
    let scope = scoped::Scoped {
        headers,
        session,
        budget,
        started,
        outbound,
        conveyance,
    };
    let routed = scoped::routed(federation, &analysis, named.as_ref(), scope).await?;
    let selection = plan::Selection::of(named.as_ref(), routed.map(|owner| owner.endpoint.id()));
    let (targets, subject) = targeted(
        federation,
        (&analysis, selection),
        (requester, on_behalf),
        deadline,
    )
    .await?;
    held_within(federation, conveyance, (&analysis, &targets), request_id)?;
    if targets.plan.has_no_destination() {
        return Err(Failure::NoDestination);
    }
    remember(federation, session, &targets);
    let attributes = analysis.attributes();
    let plan = planned(
        federation,
        targets.plan,
        &analysis,
        (completion, dedup, attributes.clone()),
    );
    let routed_to = routed.map(|owner| owner.endpoint.id().as_str().to_owned());
    let dispatch = Dispatch::of(routed, &plan);
    let answer = fanned_out(federation, plan, budget, (started, conveyance, outbound))
        .await
        .map_err(|error| {
            security::fan_out(&error, request_id);
            Failure::FanOut(error)
        })?;
    observed(federation, &answer);
    follow_up::observe(federation, answer.seen(), request_id);
    let status = settled(answer.status(), targets.resolution_failed, completion);
    let acting = dispatch.provenance(federation.snapshot(), answer.federation(), status);
    let meta = federation.access_log().map(|_| answer.federation().clone());
    let provenance = answer.attributes().to_vec();
    let mut result_set = answer
        .into_result_set(Some(request.q.clone()), Some(analysis.columns().to_vec()))
        .map_err(Failure::Envelope)?;
    let rows = std::mem::take(&mut result_set.rows);
    result_set.rows = if status == StatusCode::OK {
        let added = cells::Added {
            subject,
            attributes: &attributes,
            values: &provenance,
        };
        cells::reinject(rows, &targets.sources, &added).map_err(Failure::Cells)?
    } else {
        Vec::new()
    };
    let accessed = federation.access_log().zip(meta).and_then(|(log, meta)| {
        let answered = accessed::Answered {
            request: &request,
            stored: stored.as_deref(),
            analysis: &analysis,
            resolved: &targets.resolved,
            routed: routed_to.as_deref(),
            federation: &meta,
            rows: &result_set.rows,
            status,
        };
        accessed::query(log, federation.snapshot(), &answered)
    });
    Ok((status, result_set, acting, accessed))
}

/// Refuses, when the caller in `conveyance` is confined to one patient, the
/// `targets` of `analysis` that reach beyond that patient ([`within`]).
///
/// # Errors
///
/// Returns [`Failure::Confined`] for such targets, with nothing sent.
fn held_within(
    federation: &Federation,
    conveyance: &Conveyance,
    (analysis, targets): (&Analysis, &plan::Targets),
    request_id: &str,
) -> Result<(), Failure> {
    if confined::is_confined(conveyance)
        && !within(federation.snapshot(), conveyance, analysis, targets)
    {
        confined::stopped("patient", request_id);
        return Err(Failure::Confined);
    }
    Ok(())
}

/// `plan` shaped for `analysis` under the request's completion strategy,
/// dedup mode and ENDPOINT attributes, withholding consent where
/// `federation` does not disclose it.
fn planned(
    federation: &Federation,
    plan: Plan,
    analysis: &Analysis,
    (completion, dedup, attributes): (Completion, DedupMode, Vec<EndpointAttribute>),
) -> Plan {
    let mut plan = plan
        .completing(completion)
        .ordered(analysis.order().clone())
        .deduplicating(dedup)
        .annotating(attributes);
    if let Some(recombination) = analysis.recombination() {
        plan = plan.recombining(recombination.clone());
    }
    if !federation.discloses_consent() {
        plan = plan.withholding_consent(ErrorDetail::Text(String::from(plan::UNAVAILABLE)));
    }
    plan
}

/// The status of a fan-out that answered `status`, its resolution failed
/// at a member when `resolution_failed`, under `completion`.
fn settled(status: StatusCode, resolution_failed: bool, completion: Completion) -> StatusCode {
    // NOTE: no specification governs this (§11.3 covers only an answered lookup):
    // our own design, a cross-reference that could not answer fails the query
    // 424 under all-or-nothing; under best-effort it stays reported.
    if resolution_failed && completion == Completion::AllOrNothing && status == StatusCode::OK {
        return StatusCode::FAILED_DEPENDENCY;
    }
    status
}

/// The targets of `analysis` within `selection`, with the subject a patient
/// query names: a patient query is localized, consent-checked and resolved
/// for `requester` on behalf of `on_behalf` before `deadline`
/// ([`plan::patient`]), and any other query is planned as it stands.
///
/// # Errors
///
/// Returns [`Failure::Plan`] when the targets cannot be planned.
async fn targeted<'q>(
    federation: &Federation,
    (analysis, selection): (&'q Analysis, plan::Selection<'_>),
    (requester, on_behalf): (Option<&Requester>, &OnBehalfOf),
    deadline: Instant,
) -> Result<(plan::Targets, Option<&'q Subject>), Failure> {
    Ok(match analysis {
        Analysis::Patient(query) => (
            plan::patient(
                federation,
                selection,
                (query, requester, on_behalf),
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
    })
}

/// Refuses, when the caller in `conveyance` is confined to one patient, a
/// query whose node queries read beyond the one `EHR` each is scoped to, and
/// a query that names another patient than the confined one.
///
/// A query beyond the one `EHR` names neither a patient nor an `ehr_id`, or
/// has a class beside that `EHR`, a second `EHR`, or an `EHR` under
/// `NOT CONTAINS`. A named patient is resolved at the bound member alone,
/// on behalf of `on_behalf`, before `deadline` ([`confined::names_own`]), so
/// no localizer, consent pre-filter or other member learns of another
/// patient.
///
/// # Errors
///
/// Returns [`Failure::Confined`] for such a query, before any lookup and
/// with nothing sent, and [`Failure::Unconfirmed`] when the named patient
/// cannot be checked at the bound member.
async fn confine(
    federation: &Federation,
    (conveyance, on_behalf): (&Conveyance, &OnBehalfOf),
    analysis: &Analysis,
    (deadline, request_id): (Instant, &str),
) -> Result<(), Failure> {
    let Some(confinement) = conveyance.confinement() else {
        return Ok(());
    };
    // NOTE: master08 §Resource Scopes, §7.1: a patient grant reaches "data within that patient's
    // EHR", so every class the query reads must be contained under the one scoped EHR.
    if !analysis.within_one_ehr() {
        confined::stopped("population", request_id);
        return Err(Failure::Confined);
    }
    if let Analysis::Patient(query) = analysis {
        let patient = plan::patient_ref(query.subject()).map_err(Failure::Plan)?;
        let own = confined::names_own(federation, confinement, &patient, on_behalf, deadline)
            .await
            .map_err(Failure::Unconfirmed)?;
        if !own {
            confined::stopped("patient", request_id);
            return Err(Failure::Confined);
        }
    }
    Ok(())
}

/// Whether every `{node, ehr_id}` pair `targets` would dispatch to is one of
/// the confined patient's in `conveyance`, and there is at least one (§5.2,
/// §12.5).
///
/// A patient query goes to each member under the `ehr_id` its patient
/// resolved to there, and a query scoped to one `ehr_id` goes under that
/// `ehr_id`. A plan that dispatches nothing is refused too, so a confined
/// caller never learns whether another patient is known anywhere.
fn within(
    snapshot: &RegistrySnapshot,
    conveyance: &Conveyance,
    analysis: &Analysis,
    targets: &plan::Targets,
) -> bool {
    let scope = match analysis {
        // NOTE: §12.5, an ehr_id that is no HIER_OBJECT_ID names no EHR, so it is no pair
        // of the confined patient.
        Analysis::Unscoped(query) => query.ehr_scope().and_then(|value| EhrId::new(value).ok()),
        Analysis::Patient(_) => None,
    };
    let mut dispatched = targets.plan.dispatched().peekable();
    dispatched.peek().is_some()
        && dispatched.all(|endpoint| {
            let ehr_id = match analysis {
                Analysis::Patient(_) => snapshot.endpoint(endpoint).and_then(|declared| {
                    targets
                        .resolved
                        .iter()
                        .find(|(node, _)| node == declared.node())
                        .map(|(_, ehr_id)| ehr_id)
                }),
                Analysis::Unscoped(_) => scope.as_ref(),
            };
            ehr_id.is_some_and(|ehr_id| confined::admits(conveyance, endpoint, ehr_id))
        })
}

/// Runs the fan-out of `plan` inside the `fan_out` span, which names how many
/// endpoints were asked and the status of the answer.
async fn fanned_out(
    federation: &Federation,
    plan: Plan,
    budget: Budget,
    (started, conveyance, outbound): (Instant, &Conveyance, OutboundId),
) -> Result<FederatedAnswer, FanOutError> {
    let span = tracing::info_span!(
        "fan_out",
        endpoints = plan.dispatched().count(),
        http.response.status_code = Empty,
    );
    let answer = fan_out_within(
        federation.clients(),
        federation.snapshot(),
        plan,
        budget,
        started,
        (conveyance, Some(outbound)),
    )
    .instrument(span.clone())
    .await?;
    span.record("http.response.status_code", answer.status().as_u16());
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Failure, cells, completeness, dedup, plan, target};
    use crate::error::Code;
    use ferrofed_engine::fanout::FanOutError;
    use ferrofed_identity::role::patient::PatientRefError;
    use http::StatusCode;
    use openehr_federation::aql::refusal::Refusal;
    use openehr_its::rest::runtime::ApiError;

    #[test]
    fn each_failure_answers_its_code_and_status() {
        let table = [
            (Failure::Body, "body-invalid", StatusCode::BAD_REQUEST),
            (
                Failure::Query(ApiError::BadRequest(
                    "the required query parameter `q` is missing".to_owned(),
                )),
                "body-invalid",
                StatusCode::BAD_REQUEST,
            ),
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
                Failure::Target(target::TargetError::UnknownEndpoint {
                    by: target::Mechanism::EndpointHeader,
                    position: 1,
                    at: None,
                }),
                "endpoint-unknown",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Target(target::TargetError::UnknownOrganisation {
                    by: target::Mechanism::OrganisationDirective,
                    position: 1,
                    at: None,
                }),
                "organisation-unknown",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Target(target::TargetError::Empty(
                    target::Mechanism::OrganisationHeader,
                )),
                "organisation-unknown",
                StatusCode::BAD_REQUEST,
            ),
            (
                Failure::Target(target::TargetError::Conflict {
                    first: target::Selected {
                        by: target::Mechanism::EndpointDirective,
                        endpoints: BTreeSet::new(),
                    },
                    second: target::Selected {
                        by: target::Mechanism::OrganisationHeader,
                        endpoints: BTreeSet::new(),
                    },
                }),
                "targeting-conflict",
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
