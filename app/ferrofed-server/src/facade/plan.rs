// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fan-out plan of one federated query: which endpoint of each member is
//! asked, and the status of every endpoint that is not (§11.1, N16, N40).
//!
//! Every registry member appears in the plan, so `meta.federation` reports the
//! whole federation (§11.1, N16, CP-11). A member is asked through one
//! endpoint: the first active one in endpoint id order
//! ([`RegistrySnapshot::asked_through_among`]). Its other active
//! endpoints are `excluded`, because asking one node twice returns its rows
//! twice; a suspended endpoint is `excluded` by operator policy (§11.1);
//! and under a [`Selection::Directed`]
//! request, every endpoint the request did not name is `excluded` by that
//! decision (§8, §11.1). No specification governs the one-endpoint rule: our
//! own design.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use ferrofed_engine::dispatch::NodeQuery;
use ferrofed_engine::fanout::{Plan, PlanError};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_identity::localizer::{Localization, Localizer, LocalizerError, OnFailure};
use ferrofed_identity::patient::{IdentifierNamespace, PatientRef, PatientRefError};
use ferrofed_identity::resolver::Resolution;
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::{EndpointStatus, RegistrySnapshot};
use openehr_federation::aql::subject::Subject;
use openehr_federation::aql::{ColumnSource, PatientQuery, UnscopedQuery};
use openehr_federation::error::WireError;
use openehr_federation::outcome::{ErrorDetail, Outcome};
use secrecy::SecretString;

use crate::federation::Federation;
use crate::health::dependencies::Observed;
use crate::localization::LocalizationPolicy;

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
    /// query under all-or-nothing (§11.3 covers only an answered lookup; no
    /// specification governs this: our own design).
    pub resolution_failed: bool,
    /// The `{node, ehr_id}` set the resolution produced, for the session's
    /// resolution bindings (§12.5.1 step 2).
    pub resolved: Vec<(NodeId, EhrId)>,
    /// What the resolver showed of itself, when it was asked: up when it
    /// answered for every member, down when it could not answer for one.
    pub resolver: Option<Observed>,
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
    /// The query is scoped to one `ehr_id`, and the order of §12.5.1 routes
    /// it to the member this endpoint reaches (N29, N41). Every other
    /// endpoint is `excluded`, because the routing decision ruled it out
    /// (§11.1).
    Owner(&'a EndpointId),
}

impl<'a> Selection<'a> {
    /// The selection of a request whose directive or headers name `named`,
    /// or whose `ehr_id` routes it to `owner`: the explicit target first
    /// (§8, §12.5.1 step 1).
    #[must_use]
    pub fn of(named: Option<&'a BTreeSet<EndpointId>>, owner: Option<&'a EndpointId>) -> Self {
        match (named, owner) {
            (Some(named), _) => Self::Directed(named),
            (None, Some(owner)) => Self::Owner(owner),
            (None, None) => Self::Undirected,
        }
    }
}

impl Selection<'_> {
    /// Whether the request lets the plan ask `endpoint`.
    fn admits(self, endpoint: &EndpointId) -> bool {
        match self {
            Self::Undirected => true,
            Self::Directed(named) => named.contains(endpoint),
            Self::Owner(owner) => owner == endpoint,
        }
    }

    /// Why an endpoint the selection does not admit is `excluded`.
    fn reason(self) -> &'static str {
        match self {
            Self::Undirected | Self::Directed(_) => "not named by the request's endpoint directive",
            Self::Owner(_) => "the query's ehr_id is routed to another member (§12.5.1, N29)",
        }
    }
}

/// How each member's endpoints take part in the query.
struct Membership {
    /// The endpoint each member is asked through, when it has an active one.
    asked: BTreeMap<NodeId, EndpointId>,
    /// The endpoints that are never asked, with why.
    excluded: Vec<(EndpointId, String)>,
    /// The other active endpoints of a member asked through one endpoint,
    /// each with its member and why it is not asked.
    alternates: Vec<(NodeId, EndpointId, String)>,
}

/// The plan of a query that names a patient (§5.2, §14.1, N3, N4, N6).
///
/// An undirected query in a deployment with a localizer asks it first which
/// members might hold the patient's data, within the localizer's budget: a
/// member it does not name is `not-localized` and never asked (§11.1). A
/// localizer that does not answer leaves no candidate, every member
/// `not-localized` with its error and the error in `meta.federation`, unless
/// the deployment declared `ask-all`, under which every member is a
/// candidate (§14.1, N4). A directed query is never localized, since the
/// directive selects its node set (§8).
///
/// The patient is then resolved at every candidate before `deadline`. A
/// member that knows the patient is asked its node query; one that does not
/// is `not-resolved`, which fails nothing (N6); one the resolver could not
/// answer for is `not-resolved` with the resolver's error, which fails the
/// query (§11.3 covers only an answered lookup; no specification governs
/// this: our own design). Without a resolver, every candidate is the last
/// case: the gateway fails closed.
///
/// # Errors
/// Returns a [`TargetsError`] when the subject is not a patient reference or
/// a status cannot be described.
pub async fn patient(
    federation: &Federation,
    selection: Selection<'_>,
    query: &PatientQuery,
    deadline: Instant,
) -> Result<Targets, TargetsError> {
    let resolver = federation.resolver();
    let membership = membership(federation.snapshot(), selection);
    // NOTE: §5.4.1, the identifier resolution consumes is withheld from every
    // request the plan sends, the outbound gate's second layer.
    let withheld = Withheld::new([SecretString::from(query.subject().value())]);
    let mut plan = exclude(Plan::new().withholding(withheld), &membership.excluded)?;
    let members: Vec<NodeId> = membership.asked.keys().cloned().collect();
    // NOTE: §8, a directive selects the node set by itself, so only an undirected query is localized.
    let localizer = match selection {
        Selection::Undirected => federation.localization().localizer(),
        Selection::Directed(_) | Selection::Owner(_) => None,
    };
    let patient = if !members.is_empty() && (localizer.is_some() || resolver.is_some()) {
        Some(patient_ref(query.subject())?)
    } else {
        None
    };
    let located = match (localizer, &patient) {
        (Some(localizer), Some(patient)) => {
            let policy = federation.localization();
            localize(policy, localizer, patient, &members, deadline).await?
        }
        _ => Localized::everyone(),
    };
    for (member, endpoint, reason) in membership.alternates {
        let outcome = if located.admits(&member) {
            Outcome::Excluded {
                error: Some(detail(&reason)?),
            }
        } else {
            located.not_localized()
        };
        plan = settle(plan, endpoint, outcome)?;
    }
    let candidates: Vec<NodeId> = members
        .into_iter()
        .filter(|member| located.admits(member))
        .collect();
    let resolutions = match (resolver, &patient) {
        (Some(resolver), Some(patient)) if !candidates.is_empty() => {
            resolver.resolve(patient, &candidates, deadline).await
        }
        _ => BTreeMap::new(),
    };
    if let Some(failure) = located.failure.clone() {
        plan = plan.localization_failed(failure);
    }
    let mut sources = None;
    let mut resolution_failed = false;
    let mut bound = Vec::new();
    for (member, endpoint) in membership.asked {
        if !located.admits(&member) {
            plan = settle(plan, endpoint, located.not_localized())?;
            continue;
        }
        match resolutions.get(&member) {
            Some(Resolution::Resolved(ehr_id)) => {
                let node = query.for_node(ehr_id.hier_object_id());
                sources.get_or_insert_with(|| node.columns().to_vec());
                plan = plan
                    .dispatch(
                        endpoint,
                        NodeQuery::new(node.aql())
                            .with_scope(ehr_id.hier_object_id())
                            .with_width(super::cells::width(node.columns())),
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
        resolver: Observed::of_resolutions(&resolutions),
    })
}

/// The plan of a query that names no patient, dispatched as written.
///
/// It goes to every member `selection` admits: every member in a deployment
/// with no localizer (N4, last sentence), the endpoints a
/// directed request names (§8), or the owner of the one `ehr_id` the query
/// is scoped to (§12.5.1, N29).
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
    for (_, endpoint, reason) in membership.alternates {
        let error = Some(detail(&reason)?);
        plan = settle(plan, endpoint, Outcome::Excluded { error })?;
    }
    let node = query.node_query();
    for endpoint in membership.asked.into_values() {
        plan = plan
            .dispatch(
                endpoint,
                NodeQuery::new(node.aql()).with_width(super::cells::width(node.columns())),
            )
            .map_err(TargetsError::Plan)?;
    }
    Ok(Targets {
        plan,
        sources: query.node_query().columns().to_vec(),
        resolution_failed: false,
        resolved: Vec::new(),
        resolver: None,
    })
}

/// The endpoint each member is asked through, and the endpoints never asked.
fn membership(snapshot: &RegistrySnapshot, selection: Selection<'_>) -> Membership {
    let mut asked = BTreeMap::new();
    let mut excluded = Vec::new();
    let mut alternates = Vec::new();
    for node in snapshot.nodes() {
        let chosen = snapshot.asked_through_among(node.id(), |endpoint| selection.admits(endpoint));
        for endpoint in snapshot.endpoints_of(node.id()) {
            let reason = match (selection.admits(endpoint.id()), endpoint.status(), chosen) {
                (false, _, _) => String::from(selection.reason()),
                (true, EndpointStatus::Suspended, _) => {
                    String::from("suspended by the federation operator")
                }
                (true, EndpointStatus::Active, Some(first)) if first.id() != endpoint.id() => {
                    let reason = format!("the member is asked through endpoint {}", first.id());
                    alternates.push((node.id().clone(), endpoint.id().clone(), reason));
                    continue;
                }
                (true, EndpointStatus::Active, _) => continue,
            };
            excluded.push((endpoint.id().clone(), reason));
        }
        if let Some(endpoint) = chosen {
            asked.insert(node.id().clone(), endpoint.id().clone());
        }
    }
    Membership {
        asked,
        excluded,
        alternates,
    }
}

/// What localization left of the members asked (§14.1).
struct Localized {
    /// The members localization named, or `None` when every member is a
    /// candidate.
    candidates: Option<BTreeSet<NodeId>>,
    /// The error every member it did not name carries: the localizer's
    /// failure, under fail-closed.
    error: Option<ErrorDetail>,
    /// The localizer's failure, carried in `meta.federation` too.
    failure: Option<ErrorDetail>,
}

impl Localized {
    /// Every member a candidate: no localizer, or one that is not consulted.
    fn everyone() -> Self {
        Self {
            candidates: None,
            error: None,
            failure: None,
        }
    }

    /// No member a candidate, each carrying `error` when there is one.
    fn nobody(error: Option<ErrorDetail>) -> Self {
        Self {
            candidates: Some(BTreeSet::new()),
            failure: error.clone(),
            error,
        }
    }

    /// Whether `member` is a candidate.
    fn admits(&self, member: &NodeId) -> bool {
        self.candidates
            .as_ref()
            .is_none_or(|candidates| candidates.contains(member))
    }

    /// The status of a member that is not a candidate (§11.1).
    fn not_localized(&self) -> Outcome {
        Outcome::NotLocalized {
            error: self.error.clone(),
        }
    }
}

/// Asks `localizer` which of `members` might hold `patient`'s data, within
/// the budget `policy` gives it and before `deadline` (§14.1, N4).
///
/// A localizer still silent at the end of its budget did not answer, and the
/// failure policy applies as to any other failure.
async fn localize(
    policy: &LocalizationPolicy,
    localizer: &dyn Localizer,
    patient: &PatientRef,
    members: &[NodeId],
    deadline: Instant,
) -> Result<Localized, TargetsError> {
    let until = Instant::now()
        .checked_add(policy.timeout())
        .map_or(deadline, |at| at.min(deadline));
    let answer = tokio::time::timeout_at(
        tokio::time::Instant::from_std(until),
        localizer.localize(patient, members, until),
    )
    .await
    .unwrap_or(Localization::Unavailable(LocalizerError::DeadlineExceeded));
    match answer {
        Localization::NotConfigured => Ok(Localized::everyone()),
        Localization::Candidates(named) => Ok(Localized {
            candidates: Some(named),
            error: None,
            failure: None,
        }),
        Localization::NoRecords => Ok(Localized::nobody(None)),
        Localization::Unavailable(error) => {
            let cause = crate::chain(&error);
            tracing::warn!(
                error = %cause,
                on_failure = %policy.on_failure(),
                "the localizer did not answer"
            );
            let failure = detail(&format!("the localizer could not answer: {cause}"))?;
            Ok(match policy.on_failure() {
                OnFailure::Closed => Localized::nobody(Some(failure)),
                OnFailure::AskAll => Localized {
                    candidates: None,
                    error: None,
                    failure: Some(failure),
                },
            })
        }
    }
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
