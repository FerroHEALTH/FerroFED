// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a federated query shows of the patient data it reached: the facts
//! the access log builds its record from ([`crate::access`]; Regulation (EU)
//! 2025/327 Annex II 3.2).
//!
//! The data subject is the patient the query names and the `ehr_id` it
//! resolved to at each member (§5.2); the origins are the endpoints
//! `meta.federation.endpoints[]` records as asked, with their status and
//! the rows each sent (§11.1); the categories are read from the archetype
//! roots of the delivered rows ([`cells::root_objects`]) and from the ids the
//! bound query constrains its data to. A query that delivered no row is
//! recorded too. No specification governs the facts: our own design.

use std::sync::Arc;

use ehds_logging::classify::{Basis, Evidence, Queried};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientIdentifier};
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use openehr_federation::aql::Analysis;
use openehr_federation::meta::FederationMeta;
use openehr_federation::outcome::{ConsentRefusal, Outcome};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::generated::query::{AdhocQueryExecute, ResultSetRow};
use secrecy::SecretString;

use crate::access::{AccessLog, Accessed, Asked};
use crate::facade::cells;

/// The operation of an ad hoc query (ITS-REST Query API).
const ADHOC: &str = "query_execute_adhoc_query";

/// The operation of a stored query (ITS-REST Query API).
const STORED: &str = "query_execute_stored_query";

/// One answered federated query, as the façade holds it after the merge.
#[derive(Debug)]
pub(super) struct Answered<'a> {
    /// The request as read.
    pub(super) request: &'a AdhocQueryExecute,
    /// The stored query's name, for a stored-query execution.
    pub(super) stored: Option<&'a str>,
    /// The analysis of the façade query.
    pub(super) analysis: &'a Analysis,
    /// The `{node, ehr_id}` pairs the patient resolved to.
    pub(super) resolved: &'a [(NodeId, EhrId)],
    /// The endpoint an `ehr_id`-scoped query was routed to.
    pub(super) routed: Option<&'a str>,
    /// The `meta.federation` record of the answer.
    pub(super) federation: &'a FederationMeta,
    /// The rows delivered to the client.
    pub(super) rows: &'a [ResultSetRow],
    /// The status the client is answered with.
    pub(super) status: StatusCode,
}

/// The facts of `answered`, recorded in `log`, over `snapshot`, or `None`
/// when no node was sent the query, so no data was reached: every member
/// was ruled out, not resolved or refused by the consent pre-filter.
pub(super) fn query(
    log: &Arc<AccessLog>,
    snapshot: &RegistrySnapshot,
    answered: &Answered<'_>,
) -> Option<Accessed> {
    let contacted = answered
        .federation
        .endpoints()
        .iter()
        .any(|record| dispatched(record.outcome()));
    if !contacted {
        return None;
    }
    let analysis = answered.analysis;
    let (objects, unrooted) = cells::root_objects(answered.rows, analysis.sources());
    let constrained = analysis.constrained();
    let mut evidence = Evidence::reached(Basis::Returned, objects).queried(Queried {
        templates: constrained.templates().clone(),
        archetypes: constrained.archetypes().clone(),
        every_root_bound: constrained.every_root_bound(),
    });
    if unrooted {
        evidence = evidence.with_unrooted();
    }
    let delivered_any = answered.status == StatusCode::OK && !answered.rows.is_empty();
    let origins = answered
        .federation
        .endpoints()
        .iter()
        .filter(|record| {
            !matches!(
                record.status(),
                EndpointStatus::Excluded | EndpointStatus::NotLocalized
            )
        })
        .map(|record| {
            // NOTE: §11.1: a row count past the platform's word size is no count this
            // gateway could have merged, so the record leaves it out.
            let rows = record
                .row_count()
                .and_then(|rows| usize::try_from(rows).ok());
            Asked {
                endpoint: record.id().as_str().to_owned(),
                node: record.node_id().map(str::to_owned).or_else(|| {
                    snapshot
                        .endpoints()
                        .find(|endpoint| endpoint.id().as_str() == record.id().as_str())
                        .map(|endpoint| endpoint.node().as_str().to_owned())
                }),
                system_id: record.system_id().map(str::to_owned),
                status: record.status().as_str().to_owned(),
                rows,
                contributed: delivered_any
                    && record.status() == EndpointStatus::Active
                    && rows.is_some_and(|rows| rows > 0),
            }
        })
        .collect();
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Query,
        operation: if answered.stored.is_some() {
            STORED
        } else {
            ADHOC
        },
        resource: None,
        // NOTE: BALP 1.1.4 entity:query holds the request; the request read from JSON writes
        // back, and its query text alone stands in for it if it ever does not.
        query: Some(SecretString::from(
            serde_json::to_string(answered.request).unwrap_or_else(|_| answered.request.q.clone()),
        )),
        stored_query: answered.stored.map(str::to_owned),
        subject: subject(snapshot, answered),
        evidence,
        delivered: Some(answered.rows.len()),
        origins,
    })
}

/// Whether an endpoint that reported `outcome` was sent the query (§11.1):
/// it answered, failed or timed out, or refused on consent itself (N27).
fn dispatched(outcome: &Outcome) -> bool {
    matches!(
        outcome,
        Outcome::Active { .. }
            | Outcome::Offline { .. }
            | Outcome::TimeOut { .. }
            | Outcome::NodeError { .. }
            | Outcome::ConsentDenied {
                refused_by: ConsentRefusal::Node { .. },
                ..
            }
    )
}

/// Whose data `answered` reached: the patient it names and the `ehr_id` it
/// resolved to at each member, or the one `ehr_id` it is scoped to.
fn subject(snapshot: &RegistrySnapshot, answered: &Answered<'_>) -> DataSubject {
    match answered.analysis {
        Analysis::Patient(query) => DataSubject {
            patient: Some(PatientIdentifier {
                namespace: query.subject().namespace().to_owned(),
                value: SecretString::from(query.subject().value().to_owned()),
            }),
            ehrs: answered
                .resolved
                .iter()
                .map(|(node, ehr_id)| EhrAt {
                    endpoint: snapshot
                        .endpoints()
                        .find(|endpoint| endpoint.node() == node)
                        .map_or_else(
                            || node.as_str().to_owned(),
                            |endpoint| endpoint.id().as_str().to_owned(),
                        ),
                    ehr_id: ehr_id.as_str().to_owned(),
                })
                .collect(),
        },
        Analysis::Unscoped(query) => DataSubject {
            patient: None,
            ehrs: query
                .ehr_scope()
                .map(|ehr_id| EhrAt {
                    endpoint: answered.routed.unwrap_or_default().to_owned(),
                    ehr_id: ehr_id.to_owned(),
                })
                .into_iter()
                .collect(),
        },
    }
}
