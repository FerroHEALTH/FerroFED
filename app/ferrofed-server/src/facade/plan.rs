// SPDX-FileCopyrightText: Cadasto B.V.
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

use ferrofed_identity::behalf::OnBehalfOf;
use ferrofed_identity::consent::Requester;
use ferrofed_identity::localizer::OnFailure;
use ferrofed_identity::patient::{IdentifierNamespace, PatientRef, PatientRefError};
use ferrofed_identity::resolver::{Resolution, Resolver};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::{EndpointStatus, RegistrySnapshot};
use openehr_federation::aql::subject::Subject;
use openehr_federation::aql::{ColumnSource, PatientQuery, UnscopedQuery};
use openehr_federation::error::WireError;
use openehr_federation::outcome::{ConsentRefusal, ErrorDetail, Outcome};
use secrecy::SecretString;
use tracing::Instrument as _;
use tracing::field::Empty;

use crate::facade::consent;
use crate::facade::demographics::{self, Identified};
use crate::facade::localize::{Localized, localize};
use crate::federation::Federation;
use ferrofed_registry::health::Observed;

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
    /// The members the Step-1 consent pre-filter denied, whose cached
    /// `ehr_id`s the session drops (N27a).
    pub denied: BTreeSet<NodeId>,
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
/// A patient named in a namespace the demographics step handles is taken to
/// it first, and the master identity it finds is localized and resolved in
/// its place, both identifiers withheld from every request (Annex A §A.2,
/// §5.4.1, N33). No match leaves every member `not-resolved`, which fails
/// nothing (N6); an answer naming no one master identity leaves every
/// member `not-resolved` with the error, which fails the query; a service
/// that could not answer is handled as `identified` describes.
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
/// With a consent pre-filter configured, it is asked next about every
/// candidate, and each candidate it denies is `consent-denied` with no
/// `latency_ms`, never resolved and never sent a request (N27a, N40). A
/// candidate it does not deny is not cleared by that, and neither is one
/// localization named: its node checks consent itself (N26, N27, §14.3).
///
/// A deployment that does not disclose consent exclusions
/// ([`Federation::discloses_consent`]) still never sends a denied candidate a
/// request, but resolves it with the others and reports it as a member the
/// cross-reference does not know the patient at: `not-resolved`, with the
/// one error text such a member carries there, or the resolver's own failure
/// where it could not answer. That status clears `complete` and fails
/// nothing, as `consent-denied` does (§11.1, §11.3), so the answer never
/// presents itself as whole; `not-localized` would leave `complete` true,
/// which N37 requires of a member that was not in scope.
///
/// The patient is then resolved at every remaining candidate before
/// `deadline`. Every identity exchange is made on behalf of `on_behalf`, the
/// verified caller, whom an audited exchange's record names as its user
/// (PIXm §2:3.83.5.2.1). A member that knows the patient is asked its node
/// query; one that does not is `not-resolved`, which fails nothing (N6); one
/// the resolver could not answer for is `not-resolved` with the resolver's
/// error, which fails the query (§11.3 covers only an answered lookup; no
/// specification governs this: our own design). Without a resolver, every
/// candidate is the last case: the gateway fails closed.
///
/// # Errors
/// Returns a [`TargetsError`] when the subject is not a patient reference or
/// a status cannot be described.
pub async fn patient(
    federation: &Federation,
    selection: Selection<'_>,
    (query, requester, on_behalf): (&PatientQuery, Option<&Requester>, &OnBehalfOf),
    deadline: Instant,
) -> Result<Targets, TargetsError> {
    let resolver = federation.resolver();
    let mut membership = membership(federation.snapshot(), selection);
    let members: Vec<NodeId> = membership.asked.keys().cloned().collect();
    // NOTE: §8, a directive selects the node set by itself, so only an undirected query is localized.
    let undirected = matches!(selection, Selection::Undirected);
    let (patient, unidentified, withheld) = named(
        federation,
        (query, on_behalf),
        (members.is_empty(), undirected),
        deadline,
    )
    .await?;
    let mut plan = exclude(Plan::new().withholding(withheld), &membership.excluded)?;
    let located = match (&unidentified, &patient) {
        (Some(Unidentified::ClosedOut(failure)), _) => Localized::closed(failure.clone()),
        (None, Some(patient)) if undirected => {
            localize(federation, (patient, on_behalf), &members, deadline).await
        }
        _ => Localized::everyone(),
    };
    plan = settle_alternates(&located, plan, membership.alternates)?;
    let mut candidates: Vec<NodeId> = members
        .into_iter()
        .filter(|member| located.admits(member))
        .collect();
    let consented = match (&patient, &unidentified) {
        (Some(patient), None) => {
            consent::prefilter(federation, (patient, requester), &candidates, deadline).await
        }
        _ => consent::Prefiltered::default(),
    };
    let disclosed = federation.discloses_consent();
    plan = settle_denied(plan, &mut membership.asked, &consented, disclosed)?;
    if disclosed {
        candidates.retain(|member| !consented.denied.contains(member));
    }
    let unknown = if disclosed { UNKNOWN } else { UNAVAILABLE };
    let resolutions = match (resolver, &patient, &unidentified) {
        (Some(resolver), Some(patient), None) if !candidates.is_empty() => {
            resolve(resolver, (patient, on_behalf), &candidates, deadline).await
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
        if let Some((error, failed)) = unidentified.as_ref().and_then(Unidentified::unresolved) {
            resolution_failed |= failed;
            plan = settle(plan, endpoint, Outcome::NotResolved { error })?;
            continue;
        }
        // NOTE: Regulation (EU) 2025/327 Art 8, §11.1: `not-resolved` clears `complete` and fails
        // nothing, as `consent-denied` does, so a hidden exclusion reads as a member without the patient.
        let hidden = !disclosed && consented.denied.contains(&member);
        match resolutions.get(&member) {
            Some(Resolution::Resolved(ehr_id)) if !hidden => {
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
            answer => {
                let (error, failed) = unresolved(answer, resolver.is_some(), unknown)?;
                resolution_failed |= failed;
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
        denied: consented.denied,
        resolver: crate::health::dependencies::of_resolutions(&resolutions),
    })
}

/// The `not-resolved` error of a member the plan does not ask after the
/// cross-reference gave `answer`, with whether it fails the query: a member
/// it does not know the patient at carries `unknown` and fails nothing (N6);
/// one it could not answer for, or where no cross-reference service is
/// `configured`, fails the query (§11.3 covers only an answered lookup; no
/// specification governs this: our own design).
fn unresolved(
    answer: Option<&Resolution>,
    configured: bool,
    unknown: &str,
) -> Result<(ErrorDetail, bool), TargetsError> {
    match answer {
        Some(Resolution::Resolved(_) | Resolution::Unknown) => Ok((detail(unknown)?, false)),
        Some(Resolution::Unavailable(failure)) => Ok((
            detail(&format!("the cross-reference could not answer: {failure}"))?,
            true,
        )),
        None if configured => Ok((
            detail("the cross-reference gave no answer for this member")?,
            true,
        )),
        None => Ok((detail("no cross-reference service is configured")?, true)),
    }
}

/// Asks `resolver` for `patient`'s `ehr_id` at each of `members` on behalf
/// of `on_behalf` before `deadline`, inside the `resolve` span, which names
/// how many members were asked and how many resolved, never the patient and
/// never the caller.
async fn resolve(
    resolver: &dyn Resolver,
    (patient, on_behalf): (&PatientRef, &OnBehalfOf),
    members: &[NodeId],
    deadline: Instant,
) -> BTreeMap<NodeId, Resolution> {
    let span = tracing::info_span!("resolve", members = members.len(), resolved = Empty);
    let resolutions = resolver
        .resolve(patient, members, on_behalf, deadline)
        .instrument(span.clone())
        .await;
    let count = resolutions
        .values()
        .filter(|resolution| matches!(resolution, Resolution::Resolved(_)))
        .count();
    span.record("resolved", count);
    resolutions
}

/// Why the demographics step left a patient query without one master
/// identity to localize and resolve (Annex A §A.2).
#[derive(Debug)]
enum Unidentified {
    /// The service knows no patient: every member is `not-resolved`, which
    /// fails nothing (N6).
    NoMatch(ErrorDetail),
    /// The answer names no one master identity, or the service failed where
    /// no localization policy applies: every member is `not-resolved` with
    /// the error, which fails the query under all-or-nothing.
    Failed(ErrorDetail),
    /// The service failed on an undirected query that fails closed: every
    /// member is `not-localized` with the error, as a localizer outage
    /// leaves it (§14.1, N4).
    ClosedOut(ErrorDetail),
}

impl Unidentified {
    /// The `not-resolved` error every member localization admitted reports,
    /// with whether it fails the query, or `None` when the members are
    /// `not-localized` instead.
    fn unresolved(&self) -> Option<(ErrorDetail, bool)> {
        match self {
            Self::NoMatch(error) => Some((error.clone(), false)),
            Self::Failed(error) => Some((error.clone(), true)),
            Self::ClosedOut(_) => None,
        }
    }
}

/// The patient the query names, as it is localized and resolved, with why
/// the demographics step found no master identity for it and the values the
/// outbound gate withholds; no patient when there is no member to ask or no
/// service to consult.
///
/// # Errors
/// Returns [`TargetsError::Patient`] when the subject is not a patient
/// reference.
async fn named(
    federation: &Federation,
    (query, on_behalf): (&PatientQuery, &OnBehalfOf),
    (no_member, undirected): (bool, bool),
    deadline: Instant,
) -> Result<(Option<PatientRef>, Option<Unidentified>, Withheld), TargetsError> {
    let consulted = undirected && federation.localization().localizer().is_some()
        || federation.consent_prefilter().is_some()
        || federation.resolver().is_some();
    // NOTE: §5.4.1, the identifier resolution consumes is withheld from every
    // request the plan sends, and so is a master identity found for it.
    let mut withheld = vec![SecretString::from(query.subject().value())];
    if no_member || !consulted {
        return Ok((None, None, Withheld::new(withheld)));
    }
    let named = patient_ref(query.subject())?;
    let (master, unidentified) =
        identified(federation, (&named, on_behalf), undirected, deadline).await;
    withheld.extend(master.as_ref().map(PatientRef::withheld));
    Ok((
        Some(master.unwrap_or(named)),
        unidentified,
        Withheld::new(withheld),
    ))
}

/// The patient `named` is localized and resolved as, after the demographics
/// step: the master identity it found, or `None` for the patient as named;
/// with why it found none, when it did not.
///
/// A service that could not answer on an `undirected` query is the outage
/// of a step that feeds localization, so the deployment's localization
/// failure policy decides; an exchange that could not be audited fails
/// closed under every policy, as a localizer's does. On a directed query it
/// is a resolution that could not answer (§14.1; Annex A §A.2 places the
/// step ahead of localization; no specification governs the directed case:
/// our own design).
async fn identified(
    federation: &Federation,
    (named, on_behalf): (&PatientRef, &OnBehalfOf),
    undirected: bool,
    deadline: Instant,
) -> (Option<PatientRef>, Option<Unidentified>) {
    match demographics::identify(federation, (named, on_behalf), deadline).await {
        Identified::AsNamed => (None, None),
        Identified::Master(master) => (Some(master), None),
        Identified::NoMatch(error) => (None, Some(Unidentified::NoMatch(ErrorDetail::Text(error)))),
        Identified::Ambiguous(error) => {
            (None, Some(Unidentified::Failed(ErrorDetail::Text(error))))
        }
        Identified::Unavailable {
            failure,
            audit_failed,
        } => {
            let closed = undirected
                && (audit_failed || federation.localization().on_failure() == OnFailure::Closed);
            let failure = ErrorDetail::Text(failure);
            if closed {
                (None, Some(Unidentified::ClosedOut(failure)))
            } else {
                (None, Some(Unidentified::Failed(failure)))
            }
        }
    }
}

/// The `not-resolved` error of a member the cross-reference does not know
/// the patient at, in a deployment that discloses consent exclusions.
const UNKNOWN: &str = "the patient is not known at this member";

/// The `not-resolved` error of a member the cross-reference does not know
/// the patient at, of a member the consent pre-filter excluded, and of a node
/// that refused on consent grounds, in a deployment that does not disclose
/// consent exclusions: one text, true of each (Regulation (EU) 2025/327 Art 8).
pub(crate) const UNAVAILABLE: &str =
    "no record of the patient at this member is available to this request";

/// `plan` with every member `consented` denies settled `consent-denied` when
/// the deployment discloses consent exclusions, each leaving `asked`, and the
/// pre-filter's failure, if it could not answer, carried as
/// `meta.federation.consent.error` (N27a, N40).
///
/// A denied member is never resolved and never sent a request, so its record
/// carries no `latency_ms`. A candidate the pre-filter did not deny stays,
/// and its node checks consent itself (N27, §14.3). When the deployment does
/// not disclose them, a denied member stays in `asked`, to be resolved and
/// reported as any member the cross-reference answered for, and is never
/// sent a request either.
fn settle_denied(
    mut plan: Plan,
    asked: &mut BTreeMap<NodeId, EndpointId>,
    consented: &consent::Prefiltered,
    disclosed: bool,
) -> Result<Plan, TargetsError> {
    let denied = consented.denied.iter().filter(|_| disclosed);
    for member in denied {
        let Some(endpoint) = asked.remove(member) else {
            continue;
        };
        let error = Some(detail(
            "the consent pre-filter does not permit asking this member (N27a)",
        )?);
        let outcome = Outcome::ConsentDenied {
            refused_by: ConsentRefusal::PreFilter,
            error,
        };
        plan = settle(plan, endpoint, outcome)?;
    }
    if let Some(error) = &consented.unavailable {
        plan = plan.consent_unavailable(error.clone());
    }
    Ok(plan)
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
    // NOTE: §7.1, N29: an ehr_id that is no HIER_OBJECT_ID is no node's own, so the query
    // is scoped to none and the outbound gate reads its literal like any other text.
    let scope = query.ehr_scope().and_then(|value| EhrId::new(value).ok());
    for endpoint in membership.asked.into_values() {
        let mut sent = NodeQuery::new(node.aql()).with_width(super::cells::width(node.columns()));
        if let Some(ehr_id) = &scope {
            sent = sent.with_scope(ehr_id.hier_object_id());
        }
        plan = plan.dispatch(endpoint, sent).map_err(TargetsError::Plan)?;
    }
    Ok(Targets {
        plan,
        sources: query.node_query().columns().to_vec(),
        resolution_failed: false,
        resolved: Vec::new(),
        denied: BTreeSet::new(),
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

/// `plan` with every endpoint of `alternates` settled: `excluded` for a member
/// localization admitted, which is asked through another endpoint, and
/// `not-localized` for one it did not (§11.1).
fn settle_alternates(
    located: &Localized,
    mut plan: Plan,
    alternates: Vec<(NodeId, EndpointId, String)>,
) -> Result<Plan, TargetsError> {
    for (member, endpoint, reason) in alternates {
        let outcome = if located.admits(&member) {
            Outcome::Excluded {
                error: Some(detail(&reason)?),
            }
        } else {
            located.not_localized()
        };
        plan = settle(plan, endpoint, outcome)?;
    }
    Ok(plan)
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
pub(crate) fn patient_ref(subject: &Subject) -> Result<PatientRef, TargetsError> {
    let namespace = IdentifierNamespace::new(subject.namespace()).map_err(TargetsError::Patient)?;
    PatientRef::new(namespace, SecretString::from(subject.value())).map_err(TargetsError::Patient)
}
