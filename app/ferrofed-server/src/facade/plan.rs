// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fan-out plan of one federated query: which endpoint of each member is
//! asked, and the status of every endpoint that is not (§11.1, N16, N40).
//!
//! Every registry member appears in the plan, so `meta.federation` reports the
//! whole federation (§11.1, N16, CP-11). A member is asked through one
//! endpoint: the first active one in endpoint id order. Its other active
//! endpoints are `excluded`, because asking one node twice returns its rows
//! twice; a suspended endpoint is `excluded` by operator policy
//! (`docs/architecture.md` section 8); and under a [`Selection::Directed`]
//! request, every endpoint the request did not name is `excluded` by that
//! decision (§8, §11.1). No specification governs the one-endpoint rule: our
//! own design.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use ferrofed_engine::dispatch::NodeQuery;
use ferrofed_engine::fanout::{Plan, PlanError};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_identity::patient::{IdentifierNamespace, PatientRef, PatientRefError};
use ferrofed_identity::resolver::{Resolution, Resolver};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::{EndpointStatus, RegistrySnapshot};
use openehr_federation::aql::subject::Subject;
use openehr_federation::aql::{ColumnSource, PatientQuery, UnscopedQuery};
use openehr_federation::error::WireError;
use openehr_federation::outcome::{ErrorDetail, Outcome};
use secrecy::SecretString;

/// The plan of one query, and where each façade column of a node row comes
/// from.
#[derive(Debug)]
pub struct Targets {
    /// The fan-out plan.
    pub plan: Plan,
    /// The column sources of every node query, which the rewrite makes the
    /// same for every node.
    pub sources: Vec<ColumnSource>,
    /// Whether the resolver could not answer for some member, which fails the
    /// query under all-or-nothing (decision A17).
    pub resolution_failed: bool,
    /// The `{node, ehr_id}` set the resolution produced, for the session's
    /// resolution bindings (§12.5.1 step 2).
    pub resolved: Vec<(NodeId, EhrId)>,
}

/// A plan that cannot be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TargetsError {
    /// The patient reference could not be built from the query's subject.
    #[error("the patient reference could not be built")]
    Patient(#[source] PatientRefError),
    /// An endpoint status could not be described.
    #[error("an endpoint status could not be described")]
    Detail(#[source] WireError),
    /// The plan refused an endpoint.
    #[error("the fan-out plan refused an endpoint")]
    Plan(#[source] PlanError),
}

/// Which endpoints the request lets the plan ask (§8, §11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection<'a> {
    /// The request names no endpoint: every member is a candidate.
    Undirected,
    /// The request names these endpoints, through the `FROM ENDPOINT`
    /// directive or the `openEHR-federation-endpoint` header (§8). Every other
    /// endpoint is `excluded`, because a decision ruled it out, and stays out
    /// of scope: it neither clears `complete` nor fails the query (§11.1).
    Directed(&'a BTreeSet<EndpointId>),
}

impl Selection<'_> {
    /// Whether the request lets the plan ask `endpoint`.
    fn admits(self, endpoint: &EndpointId) -> bool {
        match self {
            Self::Undirected => true,
            Self::Directed(named) => named.contains(endpoint),
        }
    }
}

/// How each member's endpoints take part in the query.
struct Membership {
    /// The endpoint each member is asked through, when it has an active one.
    asked: BTreeMap<NodeId, EndpointId>,
    /// The endpoints that are never asked, with why.
    excluded: Vec<(EndpointId, String)>,
}

/// The plan of a query that names a patient (§5.2, N3, N6).
///
/// The patient is resolved at every member that has an active endpoint, before
/// `deadline`. A member that knows the patient is asked its node query; one
/// that does not is `not-resolved`, which fails nothing (N6); one the
/// resolver could not answer for is `not-resolved` with the resolver's error,
/// which fails the query (decision A17). Without a resolver, every such member
/// is the last case: the gateway fails closed.
///
/// # Errors
/// Returns a [`TargetsError`] when the subject is not a patient reference or
/// a status cannot be described.
pub async fn patient(
    snapshot: &RegistrySnapshot,
    selection: Selection<'_>,
    resolver: Option<&dyn Resolver>,
    query: &PatientQuery,
    deadline: Instant,
) -> Result<Targets, TargetsError> {
    let membership = membership(snapshot, selection);
    // NOTE: §5.4.1, the identifier resolution consumes is withheld from every
    // request the plan sends, the outbound gate's second layer.
    let withheld = Withheld::new([SecretString::from(query.subject().value())]);
    let mut plan = exclude(Plan::new().withholding(withheld), &membership.excluded)?;
    let members: Vec<NodeId> = membership.asked.keys().cloned().collect();
    let resolutions = match resolver {
        Some(resolver) if !members.is_empty() => {
            let patient = patient_ref(query.subject())?;
            resolver.resolve(&patient, &members, deadline).await
        }
        Some(_) | None => BTreeMap::new(),
    };
    let mut sources = None;
    let mut resolution_failed = false;
    let mut bound = Vec::new();
    for (member, endpoint) in membership.asked {
        match resolutions.get(&member) {
            Some(Resolution::Resolved(ehr_id)) => {
                let node = query.for_node(ehr_id.hier_object_id());
                sources.get_or_insert_with(|| node.columns().to_vec());
                plan = plan
                    .dispatch(
                        endpoint,
                        NodeQuery::new(node.aql()).with_scope(ehr_id.hier_object_id()),
                    )
                    .map_err(TargetsError::Plan)?;
                bound.push((member.clone(), ehr_id.clone()));
            }
            Some(Resolution::Unknown) => {
                let error = detail("the patient is not known at this member")?;
                plan = settle(plan, endpoint, Outcome::NotResolved { error })?;
            }
            Some(Resolution::Unavailable(failure)) => {
                resolution_failed = true;
                let error = detail(&format!("the cross-reference could not answer: {failure}"))?;
                plan = settle(plan, endpoint, Outcome::NotResolved { error })?;
            }
            None => {
                resolution_failed = true;
                let error = if resolver.is_some() {
                    detail("the cross-reference gave no answer for this member")?
                } else {
                    detail("no cross-reference service is configured")?
                };
                plan = settle(plan, endpoint, Outcome::NotResolved { error })?;
            }
        }
    }
    // NOTE: §7.1, with no member asked there are no rows, so the column
    // sources are never read and none is a correct answer.
    let sources = sources.unwrap_or_default();
    Ok(Targets {
        plan,
        sources,
        resolution_failed,
        resolved: bound,
    })
}

/// The plan of a query that names no patient, dispatched as written.
///
/// It goes to every member `selection` admits: every member in a deployment
/// with no localizer (N4, last sentence; decision A8), or the endpoints a
/// directed request names (§8).
///
/// # Errors
/// Returns a [`TargetsError`] when a status cannot be described.
pub fn unscoped(
    snapshot: &RegistrySnapshot,
    selection: Selection<'_>,
    query: &UnscopedQuery,
) -> Result<Targets, TargetsError> {
    let membership = membership(snapshot, selection);
    let mut plan = exclude(Plan::new(), &membership.excluded)?;
    for endpoint in membership.asked.into_values() {
        plan = plan
            .dispatch(endpoint, NodeQuery::new(query.node_query().aql()))
            .map_err(TargetsError::Plan)?;
    }
    Ok(Targets {
        plan,
        sources: query.node_query().columns().to_vec(),
        resolution_failed: false,
        resolved: Vec::new(),
    })
}

/// The endpoint each member is asked through, and the endpoints never asked.
fn membership(snapshot: &RegistrySnapshot, selection: Selection<'_>) -> Membership {
    let mut asked = BTreeMap::new();
    let mut excluded = Vec::new();
    for node in snapshot.nodes() {
        let mut endpoints: Vec<_> = snapshot.endpoints_of(node.id()).collect();
        endpoints.sort_by(|left, right| left.id().cmp(right.id()));
        let mut chosen: Option<EndpointId> = None;
        for endpoint in endpoints {
            if !selection.admits(endpoint.id()) {
                excluded.push((
                    endpoint.id().clone(),
                    String::from("not named by the request's endpoint directive"),
                ));
                continue;
            }
            match (endpoint.status(), &chosen) {
                (EndpointStatus::Suspended, _) => excluded.push((
                    endpoint.id().clone(),
                    String::from("suspended by the federation operator"),
                )),
                (EndpointStatus::Active, Some(first)) => excluded.push((
                    endpoint.id().clone(),
                    format!("the member is asked through endpoint {first}"),
                )),
                (EndpointStatus::Active, None) => chosen = Some(endpoint.id().clone()),
            }
        }
        if let Some(endpoint) = chosen {
            asked.insert(node.id().clone(), endpoint);
        }
    }
    Membership { asked, excluded }
}

/// `plan` with every endpoint of `excluded` settled as `excluded`.
fn exclude(mut plan: Plan, excluded: &[(EndpointId, String)]) -> Result<Plan, TargetsError> {
    for (endpoint, reason) in excluded {
        let error = Some(detail(reason)?);
        plan = settle(plan, endpoint.clone(), Outcome::Excluded { error })?;
    }
    Ok(plan)
}

fn settle(plan: Plan, endpoint: EndpointId, outcome: Outcome) -> Result<Plan, TargetsError> {
    plan.settle(endpoint, outcome).map_err(TargetsError::Plan)
}

fn detail(message: &str) -> Result<ErrorDetail, TargetsError> {
    ErrorDetail::text(message).map_err(TargetsError::Detail)
}

/// The patient reference the resolver is asked about.
fn patient_ref(subject: &Subject) -> Result<PatientRef, TargetsError> {
    let namespace = IdentifierNamespace::new(subject.namespace()).map_err(TargetsError::Patient)?;
    PatientRef::new(namespace, SecretString::from(subject.value())).map_err(TargetsError::Patient)
}
