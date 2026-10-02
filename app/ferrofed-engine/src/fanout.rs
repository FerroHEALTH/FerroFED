// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fan-out under one deadline and the all-or-nothing decision (§11.4, §11.5).
//!
//! One concurrent request goes to each in-scope node, the `meta.federation`
//! envelope is built from every outcome, and the decision is taken over it
//! (N37, N38, N40). A [`Plan`] names the endpoints to dispatch to, each with the node query the
//! rewrite produced for it, and the endpoints whose status was settled before
//! any request existed (`not-resolved` from the cross-reference, a
//! pre-filter's `consent-denied`, `excluded`, `not-localized`). [`fan_out`]
//! sends every query at once, each under a per-node deadline cut to the
//! overall budget, and stops waiting when the budget runs out: a node still
//! outstanding then is abandoned and reported `time-out`, abandoning it
//! touches no other request, and an answer that arrives later has nowhere to
//! go (§11.5). There is no retry and no hedging inside the budget
//! (`docs/architecture.md` section 9).
//!
//! The envelope is built before the decision, so a failing answer still
//! carries it (§11.4, CP-30). [`decide`] is the pure decision under the
//! request's [`Completion`]. All-or-nothing, the default (N37): any in-scope
//! `offline` or `time-out` fails the query `504`, any `node-error` fails it
//! `424`, and `504` wins when both occur. Best-effort, selected per request
//! with `openEHR-federation-completeness: partial`: the same failures are
//! reported and the answering nodes' rows come back with a `200`.
//! `not-resolved` and `consent-denied` are answers and fail nothing in either
//! mode (§11.3, N6). Every in-scope status short of `active` clears
//! `complete`, which the envelope derives from the statuses and never takes as
//! an input.
//!
//! The rows of the `active` nodes are merged under the plan's Tier order, cut
//! at its `LIMIT` (§11.6.1, N39) and, for a page at `OFFSET k`, sliced from
//! row `k` (§11.6.2) by `openehr_federation::merge`. A node
//! that returned `n` rows out of the federation order is reported `node-error`,
//! so under all-or-nothing the query fails `424` (decision A43).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use openehr_federation::envelope;
use openehr_federation::error::WireError;
use openehr_federation::id::EndpointId as WireEndpointId;
use openehr_federation::merge::{Disagreement, NodeAnswer, merge};
use openehr_federation::meta::{FederationMeta, TimeoutBudget};
use openehr_federation::object::Uri;
use openehr_federation::order::ResultOrder;
use openehr_federation::outcome::{EndpointOutcome, ErrorDetail, Outcome};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::Transport;
use openehr_its::rest::generated::query::{
    ResultSet, ResultSetColumn, ResultSetMetadata, ResultSetRow,
};
use tokio::task::{JoinError, JoinSet};

use crate::dispatch::{DispatchError, DispatchOptions, NodeClients, NodeQuery, NodeReply};
use crate::hygiene::Withheld;

/// The completion policy the budget applies under, as `OPTIONS {base}/` and
/// `meta.federation.timeout` name it (`docs/architecture.md` section 9).
pub const TIMEOUT_POLICY: &str = "abandon-and-mark";

/// The completion strategy a request runs under (§11.4, N37).
///
/// Both apply to reads only; a write goes to one node and succeeds or fails
/// there (§11.4, §12.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Completion {
    /// The default: an in-scope node that was asked and did not answer, or
    /// answered with an error, fails the query (`504` or `424`).
    #[default]
    AllOrNothing,
    /// Best-effort, opted into per request: the rows of the nodes that
    /// answered come back with a `200`, every other node is reported with its
    /// status, and `complete` is `false`.
    BestEffort,
}

/// The per-node timeout and the overall budget of one fan-out (§11.5, N38).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    per_node: Duration,
    overall: Duration,
}

impl Budget {
    /// A budget of `per_node` for each node's request and `overall` for the
    /// whole fan-out.
    ///
    /// # Errors
    ///
    /// Returns [`BudgetError::Zero`] when either duration is zero: a gateway
    /// MUST apply both (§11.5), and a zero budget abandons every node before
    /// it is asked.
    pub fn new(per_node: Duration, overall: Duration) -> Result<Self, BudgetError> {
        if per_node.is_zero() {
            return Err(BudgetError::Zero { which: "per-node" });
        }
        if overall.is_zero() {
            return Err(BudgetError::Zero { which: "overall" });
        }
        Ok(Self { per_node, overall })
    }

    /// The per-node timeout in force: the configured one, never longer than
    /// the overall budget.
    #[must_use]
    pub fn per_node(&self) -> Duration {
        self.per_node.min(self.overall)
    }

    /// The overall budget.
    #[must_use]
    pub fn overall(&self) -> Duration {
        self.overall
    }

    /// Returns this budget with the overall budget cut to `wait`, the client
    /// deadline of a `Prefer: wait` (§11.5).
    ///
    /// A shorter wait is honoured and a longer one changes nothing, because a
    /// client can shorten the budget and never extend it. The per-node
    /// timeout follows, since it never runs past the overall budget. A wait
    /// of zero leaves no time to ask any node, so every node is reported
    /// `time-out` and none is sent a request.
    #[must_use]
    pub fn shortened_to(self, wait: Duration) -> Self {
        Self {
            per_node: self.per_node,
            overall: self.overall.min(wait),
        }
    }

    /// The effective budget as `meta.federation.timeout` reports it (§11.5).
    fn record(&self) -> TimeoutBudget {
        TimeoutBudget {
            per_node_ms: Some(whole_ms(self.per_node())),
            overall_ms: Some(whole_ms(self.overall)),
            policy: Some(TIMEOUT_POLICY.to_owned()),
            ..TimeoutBudget::default()
        }
    }
}

/// A budget that cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BudgetError {
    /// One of the two budgets is zero.
    #[error("the {which} budget is zero")]
    Zero {
        /// `per-node` or `overall`.
        which: &'static str,
    },
}

/// What one fan-out does with each endpoint: the node queries to send, and
/// the statuses settled before any request existed.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    dispatch: BTreeMap<EndpointId, NodeQuery>,
    settled: BTreeMap<EndpointId, Outcome>,
    withheld: Arc<Withheld>,
    completion: Completion,
    order: ResultOrder,
}

impl Plan {
    /// An empty plan.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This plan refusing to send any request that carries one of the
    /// identifiers `withheld`, the ones resolution consumed (§5.4.1, N33).
    #[must_use]
    pub fn withholding(mut self, withheld: Withheld) -> Self {
        self.withheld = Arc::new(withheld);
        self
    }

    /// This plan decided under `completion` instead of the all-or-nothing
    /// default (§11.4).
    #[must_use]
    pub fn completing(mut self, completion: Completion) -> Self {
        self.completion = completion;
        self
    }

    /// This plan merging the node answers under `order`, the Tier order and
    /// `LIMIT` the rewrite wrote into the node queries, and the `OFFSET` the
    /// Tier skips (§11.6.1, §11.6.2, N39).
    #[must_use]
    pub fn ordered(mut self, order: ResultOrder) -> Self {
        self.order = order;
        self
    }

    /// Adds `endpoint`, to be asked `query`.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError::Duplicate`] when the plan already names the
    /// endpoint: every endpoint has exactly one status in a query (N16).
    pub fn dispatch(mut self, endpoint: EndpointId, query: NodeQuery) -> Result<Self, PlanError> {
        self.refuse_duplicate(&endpoint)?;
        self.dispatch.insert(endpoint, query);
        Ok(self)
    }

    /// Adds `endpoint` with the status `outcome`, settled with no request.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError::Duplicate`] when the plan already names the
    /// endpoint, and [`PlanError::DispatchedStatus`] when `outcome` is a
    /// status that only a request can produce (`active`, `offline`,
    /// `time-out`, `node-error`, a node's own consent refusal), since no
    /// request was made to measure it (N40).
    pub fn settle(mut self, endpoint: EndpointId, outcome: Outcome) -> Result<Self, PlanError> {
        if outcome.latency_ms().is_some() {
            return Err(PlanError::DispatchedStatus {
                endpoint,
                status: outcome.status(),
            });
        }
        self.refuse_duplicate(&endpoint)?;
        self.settled.insert(endpoint, outcome);
        Ok(self)
    }

    /// The endpoints the plan dispatches to, in endpoint id order.
    pub fn dispatched(&self) -> impl Iterator<Item = &EndpointId> {
        self.dispatch.keys()
    }

    fn refuse_duplicate(&self, endpoint: &EndpointId) -> Result<(), PlanError> {
        if self.dispatch.contains_key(endpoint) || self.settled.contains_key(endpoint) {
            return Err(PlanError::Duplicate {
                endpoint: endpoint.clone(),
            });
        }
        Ok(())
    }
}

/// A plan that names an endpoint twice, or settles a status no request made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    /// The endpoint is already in the plan.
    #[error("endpoint {endpoint} is named twice in one fan-out")]
    Duplicate {
        /// The endpoint.
        endpoint: EndpointId,
    },
    /// The status needs a dispatched request to be reported.
    #[error("endpoint {endpoint} cannot be settled as {status} without a request")]
    DispatchedStatus {
        /// The endpoint.
        endpoint: EndpointId,
        /// The status the plan was given.
        status: EndpointStatus,
    },
}

/// What the completion strategy makes of a fan-out (§11.4, N37).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// No in-scope endpoint failed: the answer is a `200`, with
    /// `meta.federation.complete` saying whether every one was `active`.
    Answered,
    /// Best-effort: an in-scope endpoint failed, it is reported with its
    /// status, and the rows of the endpoints that answered come back with a
    /// `200` and `complete: false`.
    Partial,
    /// An in-scope endpoint was `offline` or `time-out`: `504`.
    Unanswered,
    /// An in-scope endpoint was `node-error`, and none unanswered: `424`.
    NodeFailed,
}

impl Verdict {
    /// The HTTP status of the answer (§11.2, §11.4).
    #[must_use]
    pub fn status(self) -> StatusCode {
        match self {
            Self::Answered | Self::Partial => StatusCode::OK,
            Self::Unanswered => StatusCode::GATEWAY_TIMEOUT,
            Self::NodeFailed => StatusCode::FAILED_DEPENDENCY,
        }
    }

    /// Whether the query failed, so no rows are returned.
    #[must_use]
    pub fn failed(self) -> bool {
        matches!(self, Self::Unanswered | Self::NodeFailed)
    }
}

/// The decision over the reported endpoints under `completion` (§11.4, N37).
///
/// Under all-or-nothing, `504` takes precedence over `424`: an unanswered node
/// is an unknown, which a retry or a `partial` request recovers, while a node
/// error is already read. Under best-effort the same failures make the answer
/// [`Verdict::Partial`] and fail nothing. An endpoint that was not in scope
/// (`excluded`, `not-localized`) decides nothing, and `not-resolved` and
/// `consent-denied` are answers in either mode (§11.3).
#[must_use]
pub fn decide(endpoints: &[EndpointOutcome], completion: Completion) -> Verdict {
    let failing = endpoints
        .iter()
        .map(EndpointOutcome::status)
        .filter(|status| status.is_in_scope() && status.fails_all_or_nothing());
    let mut verdict = Verdict::Answered;
    for status in failing {
        if completion == Completion::BestEffort {
            return Verdict::Partial;
        }
        match status {
            EndpointStatus::Offline | EndpointStatus::TimeOut => return Verdict::Unanswered,
            _ => verdict = Verdict::NodeFailed,
        }
    }
    verdict
}

/// The answer of one fan-out: the decision, the envelope, and the rows of the
/// `active` endpoints when the query did not fail.
#[derive(Debug, Clone)]
pub struct FederatedAnswer {
    verdict: Verdict,
    federation: FederationMeta,
    rows: Vec<ResultSetRow>,
}

impl FederatedAnswer {
    /// The decision under the plan's completion strategy.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// The HTTP status of the answer.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.verdict.status()
    }

    /// The `meta.federation` record, present on a failing answer too
    /// (§11.4).
    #[must_use]
    pub fn federation(&self) -> &FederationMeta {
        &self.federation
    }

    /// The rows of the `active` endpoints in endpoint id order, empty when the
    /// query failed: a failing query MUST NOT return the rows it did obtain,
    /// and a best-effort answer returns exactly those (§11.4). Unresponsive
    /// nodes never contribute rows (§11.1).
    #[must_use]
    pub fn rows(&self) -> &[ResultSetRow] {
        &self.rows
    }

    /// The federated ITS-REST `RESULT_SET` of this answer, with the façade's
    /// own `q` and `columns[]` (N17, §9.2) and `meta.federation` under `meta`
    /// (§9.1). A failing answer gets the same shape with no rows.
    ///
    /// # Errors
    ///
    /// Returns [`WireError`] when the envelope cannot be encoded.
    pub fn into_result_set(
        self,
        q: Option<String>,
        columns: Option<Vec<ResultSetColumn>>,
    ) -> Result<ResultSet, WireError> {
        let mut meta = ResultSetMetadata {
            _href: None,
            _type: None,
            _schema_version: None,
            _created: None,
            _generator: None,
            _executed_aql: None,
            additional_properties: BTreeMap::new(),
        };
        envelope::attach(&self.federation, &mut meta)?;
        Ok(ResultSet {
            meta: Some(meta),
            name: None,
            q,
            columns,
            rows: self.rows,
        })
    }
}

/// A fan-out that could not produce an answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FanOutError {
    /// The plan names an endpoint the registry snapshot or its clients do
    /// not hold.
    #[error("endpoint {endpoint} is not in the registry snapshot")]
    UnknownEndpoint {
        /// The endpoint.
        endpoint: EndpointId,
    },
    /// A request could not leave the gateway (no credential, a request the
    /// client runtime refused), so there is no endpoint status to report: a
    /// gateway internal error (§11.2).
    #[error("a request could not leave the gateway")]
    Dispatch(#[from] DispatchError),
    /// A dispatch task ended without a reply.
    #[error("a dispatch task ended without a reply")]
    Task(#[from] JoinError),
    /// The overall budget runs past what the platform clock can represent.
    #[error("the overall budget runs past the platform clock")]
    Clock,
    /// An endpoint record could not be written to the envelope.
    #[error("the record of endpoint {endpoint} could not be written to the envelope")]
    Record {
        /// The endpoint.
        endpoint: EndpointId,
        /// What the wire types refused.
        #[source]
        source: WireError,
    },
    /// The envelope could not be built from the endpoint records.
    #[error("the meta.federation envelope could not be built")]
    Envelope(#[source] WireError),
}

/// Sends every query of `plan` to its node at once and decides the answer
/// (§11.4, §11.5).
///
/// Each request carries a per-node deadline, the budget's per-node timeout
/// from now and never past the overall deadline; `request_id` travels to
/// every node. When the overall budget runs out, every node still outstanding
/// is abandoned and reported `time-out` with the time it was given, and its
/// task is dropped: a late answer contributes nothing.
///
/// # Errors
///
/// Returns [`FanOutError::UnknownEndpoint`] when `plan` names an endpoint
/// `snapshot` or `clients` lack, [`FanOutError::Dispatch`] when a request
/// could not leave the gateway, [`FanOutError::Task`] when a dispatch task
/// panicked, [`FanOutError::Clock`] when the budget overflows the clock, and
/// [`FanOutError::Record`] or [`FanOutError::Envelope`] when the envelope
/// cannot be written.
pub async fn fan_out<T>(
    clients: &NodeClients<T>,
    snapshot: &RegistrySnapshot,
    plan: Plan,
    budget: Budget,
    request_id: Option<&str>,
) -> Result<FederatedAnswer, FanOutError>
where
    T: Transport + Clone + 'static,
{
    fan_out_within(clients, snapshot, plan, budget, Instant::now(), request_id).await
}

/// Sends every query of `plan` to its node at once, inside the overall budget
/// of a request that started at `started` (§11.5).
///
/// The overall deadline runs from `started`, so the time the request spent
/// before the dispatch, resolving the patient, comes out of the same budget:
/// the gateway answers within its declared overall budget. The per-node
/// deadline runs from the dispatch and never past the overall deadline, and a
/// node's latency is measured from the dispatch (§9.5, N40). A node whose
/// deadline passed before its request could be sent is `time-out` with no
/// request sent. Everything else is as [`fan_out`].
///
/// # Errors
///
/// As [`fan_out`].
pub async fn fan_out_within<T>(
    clients: &NodeClients<T>,
    snapshot: &RegistrySnapshot,
    plan: Plan,
    budget: Budget,
    started: Instant,
    request_id: Option<&str>,
) -> Result<FederatedAnswer, FanOutError>
where
    T: Transport + Clone + 'static,
{
    let deadline = started
        .checked_add(budget.overall())
        .ok_or(FanOutError::Clock)?;
    let dispatched = Instant::now();
    let node_deadline = dispatched
        .checked_add(budget.per_node())
        .map_or(deadline, |at| at.min(deadline));
    let Plan {
        dispatch,
        settled,
        withheld,
        completion,
        order: result_order,
    } = plan;
    let order: Vec<EndpointId> = dispatch.keys().cloned().collect();
    let mut tasks = JoinSet::new();
    for (index, (endpoint, query)) in dispatch.into_iter().enumerate() {
        let client = clients
            .get(&endpoint)
            .ok_or_else(|| FanOutError::UnknownEndpoint {
                endpoint: endpoint.clone(),
            })?
            .clone();
        let mut options = DispatchOptions::new(node_deadline).with_withheld(Arc::clone(&withheld));
        if let Some(id) = request_id {
            options = options.with_request_id(id);
        }
        tasks.spawn(async move { (index, client.query(&query, &options).await) });
    }
    let mut replies: Vec<Option<NodeReply>> = vec![None; order.len()];
    let until = tokio::time::Instant::from_std(deadline);
    loop {
        match tokio::time::timeout_at(until, tasks.join_next()).await {
            Ok(Some(joined)) => {
                let (index, reply) = joined?;
                if let Some(slot) = replies.get_mut(index) {
                    *slot = Some(reply?);
                }
            }
            Ok(None) => break,
            Err(_elapsed) => {
                tasks.abort_all();
                break;
            }
        }
    }
    let abandoned_ms = whole_ms(dispatched.elapsed());
    let mut records: BTreeMap<EndpointId, (Outcome, Option<Vec<ResultSetRow>>)> = settled
        .into_iter()
        .map(|(endpoint, outcome)| (endpoint, (outcome, None)))
        .collect();
    for (endpoint, reply) in order.into_iter().zip(replies) {
        let record = match reply {
            Some(NodeReply::Answered {
                result_set,
                latency_ms,
            }) => (Outcome::Active { latency_ms }, Some(result_set.rows)),
            Some(NodeReply::Failed { outcome }) => (outcome, None),
            None => (abandoned(abandoned_ms, budget.overall()), None),
        };
        records.insert(endpoint, record);
    }
    answer(snapshot, records, &result_order, budget, completion)
}

/// The `time-out` of a node still outstanding when the overall budget ran out
/// (§11.5).
fn abandoned(latency_ms: u64, overall: Duration) -> Outcome {
    Outcome::TimeOut {
        latency_ms,
        error: ErrorDetail::Text(format!(
            "abandoned with no answer when the overall budget of {} ms ran out",
            whole_ms(overall)
        )),
    }
}

/// The envelope over `records` in endpoint id order, the decision over it
/// under `completion`, and the merged rows of the `active` endpoints when the
/// query did not fail.
fn answer(
    snapshot: &RegistrySnapshot,
    records: BTreeMap<EndpointId, (Outcome, Option<Vec<ResultSetRow>>)>,
    order: &ResultOrder,
    budget: Budget,
    completion: Completion,
) -> Result<FederatedAnswer, FanOutError> {
    let mut answers = Vec::new();
    let mut statuses = Vec::with_capacity(records.len());
    for (endpoint, (outcome, answered)) in records {
        // NOTE: §9.5, `row_count` is what the node contributed, counted before
        // any federation-level `DISTINCT`, dedup or `LIMIT` touches the rows.
        let row_count = answered.as_ref().map(Vec::len);
        if let Some(answered) = answered {
            answers.push(NodeAnswer::new(endpoint.as_str(), answered));
        }
        statuses.push((endpoint, outcome, row_count));
    }
    let (mut rows, refused) = merge(answers, order).into_parts();
    let mut endpoints = Vec::with_capacity(statuses.len());
    for (endpoint, outcome, row_count) in statuses {
        let refusal = refused
            .iter()
            .find(|refusal| refusal.endpoint() == endpoint.as_str());
        let record = match refusal {
            Some(refusal) => endpoint_record(
                snapshot,
                &endpoint,
                disagreeing(outcome, refusal.reason()),
                None,
            )?,
            None => endpoint_record(snapshot, &endpoint, outcome, row_count)?,
        };
        endpoints.push(record);
    }
    let verdict = decide(&endpoints, completion);
    let federation = FederationMeta::new(endpoints)
        .map_err(FanOutError::Envelope)?
        .with_timeout(budget.record());
    if verdict.failed() {
        rows.clear();
    }
    Ok(FederatedAnswer {
        verdict,
        federation,
        rows,
    })
}

/// The `node-error` of an `active` endpoint whose answer the merge refused, a
/// response the gateway could not use (§11.1, decision A43).
///
/// Only an `active` endpoint has rows to refuse, so any other outcome is kept.
fn disagreeing(outcome: Outcome, reason: Disagreement) -> Outcome {
    match outcome {
        Outcome::Active { latency_ms } => Outcome::NodeError {
            latency_ms,
            error: ErrorDetail::Text(reason.to_string()),
        },
        other => other,
    }
}

/// One `meta.federation.endpoints[]` entry, with the registry's node, system
/// id, managing organisation, base URL, and the node's product and version
/// when the registry records them, never otherwise (§9.5, N20, N40).
fn endpoint_record(
    snapshot: &RegistrySnapshot,
    endpoint: &EndpointId,
    outcome: Outcome,
    row_count: Option<usize>,
) -> Result<EndpointOutcome, FanOutError> {
    let record_error = |source| FanOutError::Record {
        endpoint: endpoint.clone(),
        source,
    };
    let registered = snapshot
        .endpoint(endpoint)
        .ok_or_else(|| FanOutError::UnknownEndpoint {
            endpoint: endpoint.clone(),
        })?;
    let id = WireEndpointId::new(endpoint.as_str()).map_err(record_error)?;
    let url = Uri::new(registered.url().as_str()).map_err(record_error)?;
    let mut record = EndpointOutcome::new(id, outcome)
        .with_node_id(registered.node().as_str())
        .with_organisation(registered.managing_organisation().as_str())
        .with_url(url);
    if let Some(node) = snapshot.node(registered.node()) {
        record = record.with_system_id(node.system_id().as_str());
        if let Some(product) = node.product() {
            record = record.with_product(product);
        }
        if let Some(version) = node.version() {
            record = record.with_version(version);
        }
    }
    if let Some(count) = row_count {
        // NOTE: §9.5, a count past u64::MAX rows cannot arrive in one response.
        let count = u64::try_from(count).unwrap_or(u64::MAX);
        record = record.with_row_count(count).map_err(record_error)?;
    }
    Ok(record)
}

/// `duration` in whole milliseconds, saturating at `u64::MAX`.
fn whole_ms(duration: Duration) -> u64 {
    // NOTE: §9.5 reports latency in whole milliseconds; no budget runs past
    // u64::MAX ms, so saturating loses nothing.
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
