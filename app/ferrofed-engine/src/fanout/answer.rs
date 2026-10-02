// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer of one fan-out: the `meta.federation` envelope over every
//! outcome, the decision over it (§11.4), the rows of the `active` endpoints
//! merged or recombined (§11.6), and the record of the dedup applied (§10.2).

use std::collections::BTreeMap;

use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_federation::aggregate::Recombination;
use openehr_federation::attribute::EndpointAttribute;
use openehr_federation::dedup::DedupMode;
use openehr_federation::merge::{Disagreement, Merged, NodeAnswer, combine, merge};
use openehr_federation::meta::FederationMeta;
use openehr_federation::object::Uri;
use openehr_federation::order::ResultOrder;
use openehr_federation::outcome::{EndpointOutcome, ErrorDetail, Outcome};
use openehr_its::rest::generated::query::ResultSetRow;

use super::{Budget, Completion, FanOutError, FederatedAnswer, decide};

/// How the rows of the `active` endpoints become the answer's rows.
#[derive(Debug, Clone, Copy)]
pub(super) struct Shaping<'a> {
    /// The Tier order, `LIMIT` and `OFFSET` (§11.6.1, §11.6.2).
    pub(super) order: &'a ResultOrder,
    /// The recombination of an aggregate query (§11.6.3).
    pub(super) recombination: Option<&'a Recombination>,
    /// The dedup mode the request selected (§10).
    pub(super) dedup: DedupMode,
    /// The ENDPOINT attributes added beside each endpoint's rows (§9.3).
    pub(super) attributes: &'a [EndpointAttribute],
}

/// The envelope over `records` in endpoint id order, the decision over it
/// under `completion`, and the rows of the `active` endpoints, merged under
/// the order or recombined into one aggregate row, when the query did not
/// fail.
///
/// `meta.federation.dedup` records the mode on every answer, a failing one
/// included (§10.2: "the mode actually applied MUST be recorded"); what the
/// mode suppressed is recorded only beside rows, so a failing answer, which
/// returns none, carries the mode alone.
pub(super) fn answer(
    snapshot: &RegistrySnapshot,
    records: BTreeMap<EndpointId, (Outcome, Option<Vec<ResultSetRow>>)>,
    shaping: Shaping<'_>,
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
            let mut answer = NodeAnswer::new(endpoint.as_str(), answered);
            if let Some(system_id) = system_id(snapshot, &endpoint) {
                answer = answer.with_system_id(system_id);
            }
            if !shaping.attributes.is_empty() {
                answer =
                    answer.with_attributes(attributes(snapshot, &endpoint, shaping.attributes)?);
            }
            answers.push(answer);
        }
        statuses.push((endpoint, outcome, row_count));
    }
    let (merged, unrepresentable) = match shaping.recombination {
        Some(recombination) => match combine(answers, recombination, shaping.order) {
            Ok(merged) => (merged, None),
            Err(error) => (Merged::default(), Some(error)),
        },
        None => (merge(answers, shaping.order), None),
    };
    let suppressed = merged.suppressed().clone();
    let mut attributed = merged.attributes().to_vec();
    let (mut rows, refused) = merged.into_parts();
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
    let dedup = if verdict.failed() {
        shaping.dedup.record()
    } else {
        suppressed
            .record(shaping.dedup)
            .map_err(FanOutError::Envelope)?
    };
    let federation = FederationMeta::new(endpoints)
        .map_err(FanOutError::Envelope)?
        .with_timeout(budget.record())
        .with_dedup(dedup);
    if verdict.failed() {
        rows.clear();
        attributed.clear();
    } else if let Some(error) = unrepresentable {
        return Err(FanOutError::Unrepresentable(error));
    }
    Ok(FederatedAnswer {
        verdict,
        federation,
        rows,
        attributes: attributed,
    })
}

/// The values of `selected` for `endpoint`, from its registry entry (§9.3,
/// N12): its id, its managing organisation and its URL as
/// `meta.federation.endpoints[]` reports them (§9.5), and its node's
/// `system_id`.
fn attributes(
    snapshot: &RegistrySnapshot,
    endpoint: &EndpointId,
    selected: &[EndpointAttribute],
) -> Result<Vec<String>, FanOutError> {
    let unknown = || FanOutError::UnknownEndpoint {
        endpoint: endpoint.clone(),
    };
    let registered = snapshot.endpoint(endpoint).ok_or_else(unknown)?;
    selected
        .iter()
        .map(|attribute| {
            Ok(match attribute {
                EndpointAttribute::EndpointId => endpoint.as_str().to_owned(),
                EndpointAttribute::Organisation => {
                    registered.managing_organisation().as_str().to_owned()
                }
                EndpointAttribute::SystemId => system_id(snapshot, endpoint)
                    .ok_or_else(unknown)?
                    .to_owned(),
                EndpointAttribute::Url => registered.url().as_str().to_owned(),
            })
        })
        .collect()
}

/// The openEHR `system_id` of the node behind `endpoint`, as the registry
/// records it (§12a.1).
fn system_id<'a>(snapshot: &'a RegistrySnapshot, endpoint: &EndpointId) -> Option<&'a str> {
    let registered = snapshot.endpoint(endpoint)?;
    snapshot
        .node(registered.node())
        .map(|node| node.system_id().as_str())
}

/// The `node-error` of an `active` endpoint whose answer the merge refused, a
/// response the gateway could not use (§11.1; no specification governs the
/// order check: our own design).
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
    let id = openehr_federation::id::EndpointId::new(endpoint.as_str()).map_err(record_error)?;
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
