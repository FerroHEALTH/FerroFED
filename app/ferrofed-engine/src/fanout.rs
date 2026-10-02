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
//! carries it (§11.4, CP-30). [`decide`] is the pure decision: any in-scope
//! `offline` or `time-out` fails the query `504`, any `node-error` fails it
//! `424`, and `504` wins when both occur. `not-resolved` and `consent-denied`
//! are answers and fail nothing (§11.3, N6); they clear `complete`, which the
//! envelope derives from the statuses.
//!
//! The rows of the `active` nodes are concatenated in endpoint id order. That
//! is this increment's whole merge: `ORDER BY`, `LIMIT`, `DISTINCT` and the
//! re-injected columns arrive with #52 and the issues after it.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use openehr_federation::envelope;
use openehr_federation::error::WireError;
use openehr_federation::id::EndpointId as WireEndpointId;
use openehr_federation::meta::{FederationMeta, TimeoutBudget};
use openehr_federation::object::Uri;
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

/// What the default all-or-nothing strategy makes of a fan-out (§11.4, N37).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// No in-scope endpoint failed: the answer is a `200`, with
    /// `meta.federation.complete` saying whether every one was `active`.
    Answered,
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
            Self::Answered => StatusCode::OK,
            Self::Unanswered => StatusCode::GATEWAY_TIMEOUT,
            Self::NodeFailed => StatusCode::FAILED_DEPENDENCY,
        }
    }

    /// Whether the query failed, so no rows are returned.
    #[must_use]
    pub fn failed(self) -> bool {
        self != Self::Answered
    }
}

/// The all-or-nothing decision over the reported endpoints (§11.4, N37).
///
/// `504` takes precedence over `424`: an unanswered node is an unknown, which
/// a retry or a `partial` request recovers, while a node error is already read.
/// An endpoint that was not in scope (`excluded`, `not-localized`) decides
/// nothing.
#[must_use]
pub fn decide(endpoints: &[EndpointOutcome]) -> Verdict {
    let failing = endpoints
        .iter()
        .map(EndpointOutcome::status)
        .filter(|status| status.is_in_scope() && status.fails_all_or_nothing());
    let mut verdict = Verdict::Answered;
    for status in failing {
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
    /// The all-or-nothing decision.
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
    /// query failed: a failing query MUST NOT return the rows it did obtain
    /// (§11.4).
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
    let started = Instant::now();
    let deadline = started
        .checked_add(budget.overall())
        .ok_or(FanOutError::Clock)?;
    let node_deadline = started
        .checked_add(budget.per_node())
        .map_or(deadline, |at| at.min(deadline));
    let Plan {
        dispatch,
        settled,
        withheld,
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
    let abandoned_ms = whole_ms(started.elapsed());
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
    answer(snapshot, records, budget)
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

/// The envelope over `records` in endpoint id order, the decision over it,
/// and the rows of the `active` endpoints when the query did not fail.
fn answer(
    snapshot: &RegistrySnapshot,
    records: BTreeMap<EndpointId, (Outcome, Option<Vec<ResultSetRow>>)>,
    budget: Budget,
) -> Result<FederatedAnswer, FanOutError> {
    let mut endpoints = Vec::with_capacity(records.len());
    let mut rows = Vec::new();
    for (endpoint, (outcome, answered)) in records {
        let row_count = answered.as_ref().map(Vec::len);
        endpoints.push(endpoint_record(snapshot, &endpoint, outcome, row_count)?);
        if let Some(answered) = answered {
            rows.extend(answered);
        }
    }
    let verdict = decide(&endpoints);
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

/// One `meta.federation.endpoints[]` entry, with the registry's node, system
/// id, managing organisation and base URL (§9.5, N20, N40).
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

#[cfg(test)]
mod tests {
    use super::{Budget, BudgetError, Plan, PlanError, Verdict, decide};
    use crate::dispatch::NodeQuery;
    use ferrofed_registry::id::EndpointId;
    use openehr_federation::id::EndpointId as WireEndpointId;
    use openehr_federation::outcome::{ConsentRefusal, EndpointOutcome, ErrorDetail, Outcome};
    use std::time::Duration;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn failure() -> ErrorDetail {
        ErrorDetail::Text("synthetic failure".to_owned())
    }

    fn record(id: &str, outcome: Outcome) -> Result<EndpointOutcome, Box<dyn std::error::Error>> {
        Ok(EndpointOutcome::new(WireEndpointId::new(id)?, outcome))
    }

    fn active() -> Outcome {
        Outcome::Active { latency_ms: 5 }
    }

    fn time_out() -> Outcome {
        Outcome::TimeOut {
            latency_ms: 5,
            error: failure(),
        }
    }

    fn offline() -> Outcome {
        Outcome::Offline {
            latency_ms: 5,
            error: failure(),
        }
    }

    fn node_error() -> Outcome {
        Outcome::NodeError {
            latency_ms: 5,
            error: failure(),
        }
    }

    fn not_resolved() -> Outcome {
        Outcome::NotResolved { error: failure() }
    }

    fn verdict_of(outcomes: Vec<Outcome>) -> Result<Verdict, Box<dyn std::error::Error>> {
        let mut endpoints = Vec::new();
        for (index, outcome) in outcomes.into_iter().enumerate() {
            endpoints.push(record(&format!("node_{index}"), outcome)?);
        }
        Ok(decide(&endpoints))
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn the_decision_table_of_all_or_nothing() -> TestResult {
        assert_eq!(verdict_of(vec![])?, Verdict::Answered);
        assert_eq!(verdict_of(vec![active(), active()])?, Verdict::Answered);
        assert_eq!(verdict_of(vec![active(), time_out()])?, Verdict::Unanswered);
        assert_eq!(verdict_of(vec![active(), offline()])?, Verdict::Unanswered);
        assert_eq!(
            verdict_of(vec![active(), node_error()])?,
            Verdict::NodeFailed
        );
        assert_eq!(
            verdict_of(vec![node_error(), time_out()])?,
            Verdict::Unanswered,
            "504 takes precedence over 424"
        );
        assert_eq!(
            verdict_of(vec![time_out(), node_error()])?,
            Verdict::Unanswered,
            "precedence does not depend on the order"
        );
        assert_eq!(
            verdict_of(vec![not_resolved(), not_resolved()])?,
            Verdict::Answered
        );
        assert_eq!(
            verdict_of(vec![
                active(),
                Outcome::ConsentDenied {
                    refused_by: ConsentRefusal::PreFilter,
                    error: None
                }
            ])?,
            Verdict::Answered
        );
        assert_eq!(
            verdict_of(vec![active(), Outcome::Excluded { error: None }])?,
            Verdict::Answered
        );
        Ok(())
    }

    #[test]
    fn the_statuses_map_to_their_http_status() {
        assert_eq!(Verdict::Answered.status(), http::StatusCode::OK);
        assert_eq!(
            Verdict::Unanswered.status(),
            http::StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(
            Verdict::NodeFailed.status(),
            http::StatusCode::FAILED_DEPENDENCY
        );
        assert!(!Verdict::Answered.failed());
        assert!(Verdict::Unanswered.failed());
        assert!(Verdict::NodeFailed.failed());
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_plan_names_each_endpoint_once() -> TestResult {
        let endpoint = EndpointId::new("node-a-pub")?;
        let plan = Plan::new().dispatch(endpoint.clone(), NodeQuery::new("SELECT 1"))?;
        assert_eq!(
            plan.clone()
                .dispatch(endpoint.clone(), NodeQuery::new("SELECT 2"))
                .err(),
            Some(PlanError::Duplicate {
                endpoint: endpoint.clone()
            })
        );
        assert_eq!(
            plan.settle(endpoint.clone(), not_resolved()).err(),
            Some(PlanError::Duplicate { endpoint })
        );
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_status_only_a_request_produces_is_never_settled() -> TestResult {
        for outcome in [active(), time_out(), offline(), node_error()] {
            let status = outcome.status();
            let endpoint = EndpointId::new("node-a-pub")?;
            assert_eq!(
                Plan::new().settle(endpoint.clone(), outcome).err(),
                Some(PlanError::DispatchedStatus { endpoint, status })
            );
        }
        let refused_by_node = Outcome::ConsentDenied {
            refused_by: ConsentRefusal::Node { latency_ms: 3 },
            error: None,
        };
        assert!(
            Plan::new()
                .settle(EndpointId::new("node-a-pub")?, refused_by_node)
                .is_err(),
            "a node's own consent refusal needs a request"
        );
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_budget_applies_both_timeouts() -> TestResult {
        assert_eq!(
            Budget::new(Duration::ZERO, Duration::from_secs(1)).err(),
            Some(BudgetError::Zero { which: "per-node" })
        );
        assert_eq!(
            Budget::new(Duration::from_secs(1), Duration::ZERO).err(),
            Some(BudgetError::Zero { which: "overall" })
        );
        let budget = Budget::new(Duration::from_secs(9), Duration::from_secs(2))?;
        assert_eq!(
            budget.per_node(),
            Duration::from_secs(2),
            "the per-node timeout never runs past the overall budget"
        );
        Ok(())
    }
}
