// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a federated query shows of the patient data it reached: the facts
//! the access log builds its record from ([`crate::access`]; Regulation (EU)
//! 2025/327 Annex II 3.2).
//!
//! The data subject is the patient the query names and the `ehr_id` it
//! resolved to at each member (§5.2); the origins are the endpoints the
//! query left the gateway for, with the status and the rows each sent as
//! `meta.federation.endpoints[]` records them (§11.1); the categories are
//! read from the archetype roots of the delivered rows
//! ([`cells::root_objects`]) and from the ids the bound query constrains its
//! data to, and those of each origin from its own rows before the merge
//! ([`cells::node_root_objects`]). A query that delivered no row is recorded
//! too, and one no request left the gateway for is not. No specification
//! governs the facts: our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use ehds_logging::classify::{Basis, Evidence, Queried, RootObject};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientIdentifier, PatientLookup};
use ferrofed_engine::fanout::reader::RowReader;
use ferrofed_engine::fanout::{FederatedAnswer, Plan};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use openehr_federation::aql::{Analysis, ColumnSource};
use openehr_federation::meta::FederationMeta;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::generated::query::{AdhocQueryExecute, ResultSetRow};
use secrecy::SecretString;

use crate::access::{AccessLog, Accessed, Asked};
use crate::facade::cells;
use crate::federation::Federation;

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
    /// The endpoints a request left the gateway for ([`kept`]).
    pub(super) sent: &'a [EndpointId],
    /// What each endpoint's own rows held, read before the merge.
    pub(super) reached: Option<&'a Reached>,
    /// The rows delivered to the client.
    pub(super) rows: &'a [ResultSetRow],
    /// The status the client is answered with.
    pub(super) status: StatusCode,
    /// The members whose consent pre-filter denial was set aside for an
    /// emergency purpose.
    pub(super) set_aside: &'a BTreeSet<NodeId>,
}

/// The facts of `answered`, recorded in `log`, over `snapshot`, or `None`
/// when no request left the gateway, so no data was reached: every member
/// was ruled out, not resolved, refused by the consent pre-filter, or waited
/// out its deadline for a slot of its in-flight cap. A query that set a
/// consent pre-filter denial aside for an emergency purpose is recorded
/// whatever it reached (Regulation (EU) 2025/327 Art 11(5)).
pub(super) fn query(
    log: &Arc<AccessLog>,
    snapshot: &RegistrySnapshot,
    answered: &Answered<'_>,
) -> Option<Accessed> {
    if answered.sent.is_empty() && answered.rows.is_empty() && answered.set_aside.is_empty() {
        return None;
    }
    let analysis = answered.analysis;
    let (objects, unrooted) = cells::root_objects(answered.rows, analysis.sources());
    let constrained = analysis.constrained();
    let queried = Queried {
        templates: constrained.templates().clone(),
        archetypes: constrained.archetypes().clone(),
        every_root_bound: constrained.every_root_bound(),
    };
    let evidence = evidence(objects, unrooted, queried.clone());
    let delivered_any = answered.status == StatusCode::OK && !answered.rows.is_empty();
    let origins = answered
        .federation
        .endpoints()
        .iter()
        .filter(|record| {
            answered
                .sent
                .iter()
                .any(|sent| sent.as_str() == record.id().as_str())
        })
        .map(|record| {
            // NOTE: §11.1: a row count past the platform's word size is no count this
            // gateway could have merged, so the record leaves it out.
            let rows = record
                .row_count()
                .and_then(|rows| usize::try_from(rows).ok());
            let contributed = delivered_any
                && record.status() == EndpointStatus::Active
                && rows.is_some_and(|rows| rows > 0);
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
                contributed,
                evidence: answered
                    .reached
                    .filter(|_| contributed)
                    .and_then(|reached| reached.of(record.id().as_str(), &queried)),
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
        consent_set_aside: answered
            .set_aside
            .iter()
            .map(|member| member.as_str().to_owned())
            .collect(),
    })
}

/// The evidence of root `objects` reached as delivered rows, beside a value
/// that is no root when `unrooted`, from a query constrained as `queried`.
fn evidence(objects: Vec<RootObject>, unrooted: bool, queried: Queried) -> Evidence {
    let evidence = Evidence::reached(Basis::Returned, objects).queried(queried);
    if unrooted {
        evidence.with_unrooted()
    } else {
        evidence
    }
}

/// What a `federation` that keeps an access log keeps of `answer` for the
/// record: its `meta.federation` record, and the endpoints a request left
/// the gateway for, each of which answered, failed or gave no answer.
///
/// An endpoint settled with no request, or one whose request waited out its
/// deadline for a slot of the in-flight cap, was never sent the query and is
/// no origin of its data (Regulation (EU) 2025/327 Annex II 3.2(e)).
pub(super) fn kept(
    federation: &Federation,
    answer: &FederatedAnswer,
) -> Option<(FederationMeta, Vec<EndpointId>)> {
    federation.access_log()?;
    let sent = answer
        .contacts()
        .filter(|(_, contact)| contact.left())
        .map(|(endpoint, _)| endpoint.clone())
        .collect();
    Some((answer.federation().clone(), sent))
}

/// What the rows each endpoint answered with held, read before the merge
/// combines them, so each origin of a merged answer is classified by its own
/// rows without a column that would change which rows `DISTINCT` keeps (N13).
#[derive(Debug, Default)]
pub(super) struct Reached(Mutex<BTreeMap<EndpointId, (Vec<RootObject>, bool)>>);

impl Reached {
    /// The evidence of the rows `endpoint` answered with, from a query
    /// constrained as `queried`, or `None` when it answered none.
    fn of(&self, endpoint: &str, queried: &Queried) -> Option<Evidence> {
        let read = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let (objects, unrooted) = read
            .iter()
            .find(|(read, _)| read.as_str() == endpoint)
            .map(|(_, reached)| reached)?;
        Some(evidence(objects.clone(), *unrooted, queried.clone()))
    }
}

/// `plan`, for a `federation` that keeps an access log, reading each
/// endpoint's rows against the node columns of `analysis` before the merge,
/// with what they held.
pub(super) fn reading(
    federation: &Federation,
    plan: Plan,
    analysis: &Analysis,
) -> (Plan, Option<Arc<Reached>>) {
    if federation.access_log().is_none() {
        return (plan, None);
    }
    let reached = Arc::new(Reached::default());
    let read = Arc::clone(&reached);
    let sources: Vec<ColumnSource> = analysis.sources().to_vec();
    let reader = RowReader::new(move |endpoint, rows| {
        let held = cells::node_root_objects(rows, &sources);
        read.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(endpoint.clone(), held);
    });
    (plan.reading(reader), Some(reached))
}

/// Whose data `answered` reached: the patient it names and the `ehr_id` it
/// resolved to at each member the query was sent to, at the endpoint it was
/// sent through, or the one `ehr_id` it is scoped to at the endpoint it was
/// routed to, when that endpoint was sent it.
///
/// An `ehr_id` at a member the query never left the gateway for was not
/// reached, so the record does not name it.
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
                .filter_map(|(node, ehr_id)| {
                    let sent = answered.sent.iter().find(|sent| {
                        snapshot
                            .endpoint(sent)
                            .is_some_and(|endpoint| endpoint.node() == node)
                    })?;
                    Some(EhrAt {
                        endpoint: sent.as_str().to_owned(),
                        ehr_id: ehr_id.as_str().to_owned(),
                        patient: PatientLookup::RequestNamed,
                    })
                })
                .collect(),
        },
        Analysis::Unscoped(query) => DataSubject {
            patient: None,
            ehrs: query
                .ehr_scope()
                .filter(|_| {
                    answered.routed.is_none_or(|routed| {
                        answered.sent.iter().any(|sent| sent.as_str() == routed)
                    })
                })
                .map(|ehr_id| EhrAt {
                    endpoint: answered.routed.unwrap_or_default().to_owned(),
                    ehr_id: ehr_id.to_owned(),
                    // NOTE: no specification governs this: our own design; the record gate names
                    // the patient, and an ehr_id it never reaches stays marked unavailable.
                    patient: PatientLookup::Unavailable,
                })
                .into_iter()
                .collect(),
        },
    }
}
