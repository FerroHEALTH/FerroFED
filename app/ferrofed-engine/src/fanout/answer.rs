// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer of one fan-out: the `meta.federation` envelope over every
//! outcome, the decision over it (§11.4), the rows of the `active` endpoints
//! merged or recombined (§11.6), and the record of the dedup applied (§10.2).

use std::collections::BTreeMap;

use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use openehr_base::prelude::ObjectVersionId;
use openehr_federation::aggregate::Recombination;
use openehr_federation::attribute::EndpointAttribute;
use openehr_federation::dedup::DedupMode;
use openehr_federation::envelope;
use openehr_federation::error::WireError;
use openehr_federation::merge::{Disagreement, Merged, NodeAnswer, combine, merge};
use openehr_federation::meta::FederationMeta;
use openehr_federation::object::Uri;
use openehr_federation::order::ResultOrder;
use openehr_federation::outcome::{ConsentRefusal, EndpointOutcome, ErrorDetail, Outcome};
use openehr_its::rest::generated::query::{
    ResultSet, ResultSetColumn, ResultSetMetadata, ResultSetRow,
};
use tracing::field::Empty;

use super::seen::{self, Seen};
use super::{Budget, Completion, FanOutError, FederatedAnswer, Verdict, decide};
use crate::dispatch::Contact;

/// The outcome the answer reports for a node that failed with `outcome`, and
/// the outcome it replaced, when the plan withholds consent with `concealed`.
///
/// A node's own consent refusal becomes `not-resolved` carrying `concealed`,
/// with no `latency_ms`, so its record is the one a member that does not know
/// the patient has; every other outcome stays as it is.
// NOTE: Regulation (EU) 2025/327 Art 8 against N40: a dispatched member keeps no latency here,
// since a `not-resolved` record with one would show the refusal to the clinician.
pub(super) fn concealed(
    outcome: Outcome,
    concealed: Option<&ErrorDetail>,
) -> (Outcome, Option<Outcome>) {
    match (concealed, &outcome) {
        (
            Some(error),
            Outcome::ConsentDenied {
                refused_by: ConsentRefusal::Node { .. },
                ..
            },
        ) => (
            Outcome::NotResolved {
                error: error.clone(),
            },
            Some(outcome),
        ),
        _ => (outcome, None),
    }
}

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
///
/// The versions every answering endpoint's rows show it holding are kept
/// beside the answer, a failing one included, since the node sent them
/// whatever the decision (§12.2, N21). It runs inside the `merge` span, which
/// names how many endpoints were recorded and how many rows the answer holds.
pub(super) fn answer(
    snapshot: &RegistrySnapshot,
    records: BTreeMap<EndpointId, (Outcome, Option<Vec<ResultSetRow>>)>,
    shaping: Shaping<'_>,
    budget: Budget,
    completion: Completion,
) -> Result<FederatedAnswer, FanOutError> {
    let span = tracing::info_span!("merge", endpoints = records.len(), rows = Empty);
    let _merging = span.enter();
    let mut answers = Vec::new();
    let mut statuses = Vec::with_capacity(records.len());
    let mut versions = Seen::new();
    for (endpoint, (outcome, answered)) in records {
        // NOTE: §9.5, `row_count` is what the node contributed, counted before
        // any federation-level `DISTINCT`, dedup or `LIMIT` touches the rows.
        let row_count = answered.as_ref().map(Vec::len);
        if let Some(answered) = answered {
            seen::record(&mut versions, &endpoint, &answered);
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
    span.record("rows", rows.len());
    Ok(FederatedAnswer {
        verdict,
        federation,
        rows,
        attributes: attributed,
        seen: versions
            .into_iter()
            .map(|((endpoint, _), version)| (endpoint, version))
            .collect(),
        contacts: BTreeMap::new(),
        observed: BTreeMap::new(),
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

/// Records in `federation` that a Step-1 service did not answer, as
/// `<member>.error`: the localizer as `localization.error` (§14.1: the
/// failure SHOULD be carried in `meta.federation` as well as on each
/// endpoint), and the consent pre-filter as `consent.error`.
pub(super) fn report_unavailable(
    federation: &mut FederationMeta,
    member: &'static str,
    error: ErrorDetail,
) -> Result<(), FanOutError> {
    // NOTE: §14.1 names no member for it, and the schema leaves `federation` open
    // for it: our own design, `<member>.error` in the shape of an endpoint's `error`.
    let report = BTreeMap::from([("error", error)]);
    federation
        .extra_mut()
        .insert_serialized(member, &report)
        .map_err(FanOutError::Envelope)?;
    Ok(())
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

    /// The values of the plan's ENDPOINT attributes beside each row, in
    /// [`FederatedAnswer::rows`] order, from the registry entry of the
    /// endpoint the row came from (§9.3, N12; [`super::Plan::annotating`]). An entry
    /// is empty when the plan adds no attribute, and for the one row of a
    /// recombined aggregate, which comes from no single endpoint.
    #[must_use]
    pub fn attributes(&self) -> &[Vec<String>] {
        &self.attributes
    }

    /// The versions the rows of each answering endpoint show it holding, one
    /// per endpoint and `creating_system_id`, in endpoint id order: what the
    /// follow-up routing table learns from (§12.2, N21).
    ///
    /// They are read from every endpoint that sent rows, a failing answer's
    /// included.
    pub fn seen(&self) -> impl Iterator<Item = (&EndpointId, &ObjectVersionId)> {
        self.seen
            .iter()
            .map(|(endpoint, version)| (endpoint, version))
    }

    /// What the request to each endpoint the plan dispatched to showed of
    /// the node, in endpoint id order: its own HTTP status where it answered,
    /// which the §11.1 record in [`FederatedAnswer::federation`] carries only
    /// as text. An endpoint settled with no request has none.
    pub fn contacts(&self) -> impl Iterator<Item = (&EndpointId, Contact)> {
        self.contacts
            .iter()
            .map(|(endpoint, contact)| (endpoint, *contact))
    }

    /// The outcome the gateway observed of `endpoint`, where the answer
    /// reports another: a node's consent refusal the plan withholds
    /// ([`super::Plan::withholding_consent`]) is `not-resolved` in
    /// [`FederatedAnswer::federation`] and `consent-denied` here, with its
    /// latency, for the operator's metrics.
    #[must_use]
    pub fn observed(&self, endpoint: &EndpointId) -> Option<&Outcome> {
        self.observed.get(endpoint)
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
            additional_properties: BTreeMap::new(),
        })
    }
}
