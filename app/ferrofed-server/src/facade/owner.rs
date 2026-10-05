// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which member owns a path `ehr_id` (§12.5, §12.5.1, N41).
//!
//! An `ehr_id` carries no system component, so a request under
//! `{base}/v1/ehr/{ehr_id}` names its node only through what surrounds it.
//! [`located`] takes the first three steps of §12.5.1 in order and never
//! moves past a step that names exactly one member:
//!
//! 1. the targeting headers, `openEHR-federation-endpoint` and
//!    `openEHR-federation-organisation` (§8.4);
//! 2. a resolution binding the client session holds (§5.2);
//! 3. the `ehr_id` to node index.
//!
//! A binding or an index entry naming several members is a collision: no
//! later step picks one of them, and the request, a read or a write, is
//! refused `409` listing the claimants (§12.5.2, N42). The fourth step, the
//! ask-all probe, is for reads only and is [`settled`] from what every member
//! answered: one member holding the `ehr_id` owns it, two are a `409`, none is
//! a `404`, and a member that gave no answer leaves the owner unknown, which
//! fails the read (§11.2, §11.5). Every collision raises the integrity
//! incident of N42 ([`collided`]).
//!
//! A new EHR under a path `ehr_id` goes only where the targeting headers
//! send it (§12.4, N23), and [`held_elsewhere`] reads steps 2 and 3 for it
//! too: an `ehr_id` either places at another member is refused before it is
//! sent, so the create never makes the collision of §12.5.2.
//!
//! The `ehr_id` in the path is the node-local identifier N33 locates a node
//! by, and it is the only value of the request any step reads.

use std::fmt;
use std::time::Instant;

use ferrofed_engine::single_node::forward::{ForwardError, Forwarded};
use ferrofed_engine::single_node::probe::Answer;
use ferrofed_identity::session::{Bound, ResolutionBindings, SessionKey};
use ferrofed_registry::ehr_index::{EhrIndex, Indexed};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::incident::{Detection, Incident};
use ferrofed_registry::snapshot::{Endpoint, RegistrySnapshot};
use http::{HeaderMap, StatusCode};

use crate::error::Code;
use crate::facade::target;

/// The step of §12.5.1 that named the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Step 1: the targeting headers.
    Target,
    /// Step 2: a resolution binding of the client session.
    Binding,
    /// Step 3: the `ehr_id` to node index.
    Index,
    /// Step 4: the ask-all probe.
    AskAll,
}

impl Step {
    /// The step's name as the log records it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Binding => "binding",
            Self::Index => "index",
            Self::AskAll => "ask-all",
        }
    }
}

/// What the first three steps of §12.5.1 say about a path `ehr_id`.
#[derive(Debug, Clone)]
pub enum Located<'a> {
    /// A step named one endpoint.
    ///
    /// The endpoint may be suspended, and then the request has no
    /// destination (§11.1, §11.2).
    At {
        /// The endpoint the request goes to.
        endpoint: &'a Endpoint,
        /// The step that named it.
        step: Step,
    },
    /// A step named one member that has no endpoint in the registry, so the
    /// request has no destination (§11.2).
    Unreachable {
        /// The step that named it.
        step: Step,
    },
    /// A binding or the index named several members of the registry: they
    /// claim one `ehr_id`, and no step picks one of them (§12.5.2, N42).
    Collision(Claimed),
    /// No step named exactly one member.
    Unknown,
}

/// The members a step of §12.5.1 found claiming one `ehr_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed {
    /// The endpoint each claimant is reached through, in `node_id` order.
    pub claimants: Vec<EndpointId>,
    /// The step that found them.
    pub detection: Detection,
}

/// The client session's bindings, when the request belongs to a session.
#[derive(Debug, Clone, Copy)]
pub struct Held<'a> {
    /// Every session's resolution bindings.
    pub bindings: &'a ResolutionBindings,
    /// The session of this request.
    pub session: &'a SessionKey,
    /// The instant the bindings are read at.
    pub now: Instant,
}

/// Locates the owner of `ehr_id` by the first three steps of §12.5.1, in
/// order (N41).
///
/// A binding or an index entry naming a member the snapshot does not hold is
/// stale whole: it is dropped, and the next step is taken, so routing
/// re-learns the owner from the nodes. A step naming two members the snapshot
/// holds is a [`Located::Collision`], and no later step is taken: no step
/// picks one of two claimants (§12.5.2, N42).
///
/// # Errors
///
/// Returns [`Untargeted`] when the targeting headers name what the registry
/// does not know, disagree, or select other than one endpoint: an explicit
/// target that cannot be followed is refused, never passed over (§8.4.1).
pub fn located<'a>(
    snapshot: &'a RegistrySnapshot,
    headers: &HeaderMap,
    held: Option<Held<'_>>,
    index: &EhrIndex,
    ehr_id: &EhrId,
) -> Result<Located<'a>, Untargeted> {
    if let Some(endpoint) = targeted(snapshot, headers)? {
        return Ok(Located::At {
            endpoint,
            step: Step::Target,
        });
    }
    if let Some(held) = held {
        let bound = bound(snapshot, held, ehr_id);
        if let Some(located) = among(snapshot, &bound, Step::Binding, Detection::Binding) {
            return Ok(located);
        }
    }
    let indexed = indexed(snapshot, index, ehr_id);
    Ok(among(snapshot, &indexed, Step::Index, Detection::Index).unwrap_or(Located::Unknown))
}

/// Locates the owner of a path `ehr_id` for a caller confined to one patient.
///
/// The targeting headers come first, then `holders`, the members at which
/// that patient's own `ehr_id` is the path `ehr_id`. The confinement is a resolution of the caller's patient, so it stands as
/// step 2 of §12.5.1, and no later step is taken: neither the index nor the
/// ask-all probe can name a member outside the patient's own pairs.
///
/// # Errors
///
/// Returns [`Untargeted`] as [`located`] does.
pub fn located_within<'a>(
    snapshot: &'a RegistrySnapshot,
    headers: &HeaderMap,
    holders: &[NodeId],
) -> Result<Located<'a>, Untargeted> {
    if let Some(endpoint) = targeted(snapshot, headers)? {
        return Ok(Located::At {
            endpoint,
            step: Step::Target,
        });
    }
    Ok(among(snapshot, holders, Step::Binding, Detection::Binding).unwrap_or(Located::Unknown))
}

/// The members a binding of the `held` session names for `ehr_id` (§12.5.1
/// step 2), or none when the binding names a member the snapshot does not
/// hold, which is dropped whole.
fn bound(snapshot: &RegistrySnapshot, held: Held<'_>, ehr_id: &EhrId) -> Vec<NodeId> {
    let present = |node: &NodeId| snapshot.node(node).is_some();
    let nodes = match held.bindings.lookup(held.session, held.now, ehr_id) {
        Bound::One(node) => vec![node],
        Bound::Several(nodes) => nodes,
        Bound::None => Vec::new(),
    };
    if nodes.iter().all(present) {
        return nodes;
    }
    if held.bindings.forget_absent(held.session, ehr_id, present) {
        stale(Step::Binding);
    }
    Vec::new()
}

/// The members the index holds `ehr_id` at (§12.5.1 step 3), or none when
/// its entry names a member the snapshot does not hold, which is dropped
/// whole.
fn indexed(snapshot: &RegistrySnapshot, index: &EhrIndex, ehr_id: &EhrId) -> Vec<NodeId> {
    let present = |node: &NodeId| snapshot.node(node).is_some();
    let nodes = match index.lookup(ehr_id) {
        Indexed::One(node) => vec![node],
        Indexed::Several(nodes) => nodes,
        Indexed::None => Vec::new(),
    };
    if nodes.iter().all(present) {
        return nodes;
    }
    if index.forget_absent(ehr_id, present) {
        stale(Step::Index);
    }
    Vec::new()
}

/// Refuses a new EHR whose `ehr_id` a held binding or the index places at a
/// member other than `at`'s.
///
/// `at` is the endpoint the targeting headers name, and `None` means neither
/// step places the `ehr_id` elsewhere. Only the steps after the explicit
/// target are read, never the ask-all probe, because a write is never probed
/// for (§12.5.1, N41). A step that places the `ehr_id` at `at` alone lets the
/// create through: that node answers its own ITS-REST `409` for an `ehr_id`
/// it holds. An entry naming a member the snapshot does not hold is dropped
/// whole, as [`located`] drops it, and places the `ehr_id` nowhere.
#[must_use]
pub fn held_elsewhere(
    snapshot: &RegistrySnapshot,
    held: Option<Held<'_>>,
    index: &EhrIndex,
    ehr_id: &EhrId,
    at: &Endpoint,
) -> Option<HeldElsewhere> {
    let elsewhere = |nodes: Vec<NodeId>, detection| {
        let others: Vec<NodeId> = nodes.into_iter().filter(|node| node != at.node()).collect();
        (!others.is_empty()).then(|| HeldElsewhere {
            holders: others
                .iter()
                .filter_map(|node| endpoint_of(snapshot, node))
                .map(|endpoint| endpoint.id().clone())
                .collect(),
            detection,
            at: at.id().clone(),
        })
    };
    if let Some(held) = held
        && let Some(refused) = elsewhere(bound(snapshot, held, ehr_id), Detection::Binding)
    {
        return Some(refused);
    }
    elsewhere(indexed(snapshot, index, ehr_id), Detection::Index)
}

/// Why a new EHR is not created under its path `ehr_id` at the endpoint the
/// targeting headers name (§12.4, §12.5.2; ITS-REST 1.1.0
/// `ehr_create_with_id`).
///
/// The message names registry endpoint ids only, never the `ehr_id` or
/// another value of the request (§5.4.3).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "a new EHR under this ehr_id is not created at endpoint {at}: {detection} already places the ehr_id at endpoints {}, and one ehr_id at two members is the collision the gateway never lets arise (ITS-REST 1.1.0 ehr_create_with_id; §12.4, §12.5.2, N42)",
    Listed(.holders)
)]
pub struct HeldElsewhere {
    /// The endpoint each other member holding the `ehr_id` is reached
    /// through, in `node_id` order.
    pub holders: Vec<EndpointId>,
    /// The step of §12.5.1 that places the `ehr_id` there.
    pub detection: Detection,
    /// The endpoint the targeting headers name.
    pub at: EndpointId,
}

/// Logs that `step` held an entry naming a member the registry does not
/// hold, which was dropped.
fn stale(step: Step) {
    // NOTE: §12.5.2, N42: an entry naming a departed claimant is dropped whole
    // and never narrowed to the claimant that remains, a pick of one N42 forbids.
    tracing::info!(
        step = step.as_str(),
        "a routing entry named a member the registry does not hold, and was dropped"
    );
}

/// What a step naming `nodes`, every one a member of the snapshot, says:
/// `None` when it names none.
fn among<'a>(
    snapshot: &'a RegistrySnapshot,
    nodes: &[NodeId],
    step: Step,
    detection: Detection,
) -> Option<Located<'a>> {
    let mut held = nodes.iter().filter_map(|node| member(snapshot, node, step));
    let (first, second) = (held.next()?, held.next());
    let Some(second) = second else {
        return Some(first);
    };
    // NOTE: §12.5.2, N42: two claimants are a collision on a read as on a
    // write, so neither a later step nor the probe settles it.
    let claimants = [first, second]
        .into_iter()
        .chain(held)
        .filter_map(|located| match located {
            Located::At { endpoint, .. } => Some(endpoint.id().clone()),
            Located::Unreachable { .. } | Located::Collision(_) | Located::Unknown => None,
        })
        .collect();
    Some(Located::Collision(Claimed {
        claimants,
        detection,
    }))
}

/// Where `node` is reached, as `step` named it, or `None` when the snapshot
/// holds no such member.
fn member<'a>(snapshot: &'a RegistrySnapshot, node: &NodeId, step: Step) -> Option<Located<'a>> {
    snapshot.node(node)?;
    Some(match endpoint_of(snapshot, node) {
        Some(endpoint) => Located::At { endpoint, step },
        None => Located::Unreachable { step },
    })
}

/// The endpoint a member is named by: the one it is asked through, or its
/// first endpoint in `endpoint_id` order when every one is suspended.
fn endpoint_of<'a>(snapshot: &'a RegistrySnapshot, node: &NodeId) -> Option<&'a Endpoint> {
    snapshot.asked_through(node).or_else(|| {
        snapshot
            .endpoints()
            .find(|endpoint| endpoint.node() == node)
    })
}

/// The endpoint each member is probed through in step 4, in `node_id` order;
/// a member with no active endpoint is never contacted (§11.1).
#[must_use]
pub fn probed(snapshot: &RegistrySnapshot) -> Vec<EndpointId> {
    snapshot
        .nodes()
        .filter_map(|node| snapshot.asked_through(node.id()))
        .map(|endpoint| endpoint.id().clone())
        .collect()
}

/// Records that `node` holds `ehr_id`, from a resolution or a member's
/// successful answer under it.
///
/// An `ehr_id` the index holds at another member raises the index-insert
/// alarm of §12b.2, which the index emits itself, once.
pub fn learn(index: &EhrIndex, ehr_id: &EhrId, node: &NodeId) {
    index.learn(ehr_id, node);
}

/// Raises the integrity incident of a request refused because `claimants`
/// claim its `ehr_id`, as `detection` found them (§12.5.2, N42).
///
/// The incident is emitted once, here, and names routing ids only.
pub fn collided(ehr_id: &EhrId, detection: Detection, claimants: &[EndpointId]) {
    Incident::EhrIdCollision {
        ehr_id: ehr_id.clone(),
        detection,
        claimants: claimants.to_vec(),
    }
    .emit();
}

/// The endpoint the targeting headers name, `None` without either header
/// (§8.4, §12.5.1 step 1).
///
/// The `openEHR-federation-endpoint` and `openEHR-federation-organisation`
/// headers both apply to a routed request, read as [`target::requested`]
/// reads them for a query; a routed request reaches one node, so together
/// they select exactly one endpoint (§7a.1, §12.4). A new EHR is routed by
/// this step alone, since it has no owner for a later step to find (§12.4).
///
/// # Errors
///
/// Returns [`Untargeted`] when the headers name what the registry does not
/// know, disagree, or select other than one endpoint (§8.4.1).
pub fn targeted<'a>(
    snapshot: &'a RegistrySnapshot,
    headers: &HeaderMap,
) -> Result<Option<&'a Endpoint>, Untargeted> {
    let Some(selected) = target::requested(snapshot, None, headers)? else {
        return Ok(None);
    };
    let mut selected = selected.into_iter();
    let id = match (selected.next(), selected.next()) {
        (Some(id), None) => id,
        (Some(_), Some(_)) => return Err(Untargeted::Several),
        (None, _) => return Err(Untargeted::Nothing),
    };
    Ok(Some(registered(snapshot, &id)))
}

/// The endpoint `id` of `snapshot`, which selected it.
#[expect(
    clippy::expect_used,
    reason = "target::requested selects only endpoints it found in this same snapshot"
)]
fn registered<'a>(snapshot: &'a RegistrySnapshot, id: &EndpointId) -> &'a Endpoint {
    snapshot
        .endpoint(id)
        .expect("a selected endpoint should be in the snapshot that selected it")
}

/// Why the targeting headers of a routed request name no one endpoint.
#[derive(Debug, thiserror::Error)]
pub enum Untargeted {
    /// A header names what the registry does not know, or the two headers
    /// select different node sets (§8.4.1).
    #[error(transparent)]
    Target(#[from] target::TargetError),
    /// The headers select more than one endpoint.
    #[error(
        "the targeting headers select more than one endpoint, and a request routed to one node selects exactly one (§7a.1, §12.4)"
    )]
    Several,
    /// The headers select no endpoint: the organisation manages none.
    #[error("the targeting headers select no endpoint, so the request has no destination (§11.2)")]
    Nothing,
}

impl Untargeted {
    /// The stable code the error body names.
    #[must_use]
    pub fn code(&self) -> Code {
        match self {
            Self::Target(error) => error.code(),
            Self::Several => Code::EndpointSeveral,
            Self::Nothing => Code::NoDestination,
        }
    }
}

/// What the ask-all probe settled (§12.5.1 step 4).
#[derive(Debug)]
pub enum Settled {
    /// Exactly one member holds the `ehr_id`, and every other member said it
    /// does not.
    Owner {
        /// The endpoint the owner answered through.
        endpoint: EndpointId,
        /// Its answer to the probe.
        answer: Forwarded,
    },
    /// No member owns the `ehr_id`, or the owner is unknown: the read fails.
    Failed(Unsettled),
}

/// Why the ask-all probe names no owner.
#[derive(Debug, thiserror::Error)]
pub enum Unsettled {
    /// Every member said it does not hold the `ehr_id` (§11.2).
    #[error(
        "no member holds the ehr_id: every member asked answered 404, so the request can be routed to no destination (§11.2, §12.5.1)"
    )]
    Nowhere,
    /// No member the gateway may read holds the `ehr_id`, in a deployment
    /// that does not disclose consent exclusions: the one answer for an
    /// `ehr_id` no member holds and for one only a member refusing on
    /// consent grounds would serve (Regulation (EU) 2025/327 Art 8; RFC 9110
    /// §15.5.5).
    #[error("{}", Code::SubjectUnavailable.message())]
    Unavailable,
    /// More than one member holds the `ehr_id`, and the gateway never
    /// chooses between them (§12.5.2, N42).
    #[error(
        "the ehr_id is claimed by endpoints {}, and the gateway never chooses between them (§12.5.2, N42)",
        Listed(.0)
    )]
    Claimed(Vec<EndpointId>),
    /// A member gave no answer that says whether it holds the `ehr_id`, so
    /// the owner is unknown and never taken to be the one that answered
    /// (§11.5: a `time-out` means unknown, never no data).
    #[error(
        "{} gave no answer to the ask-all probe that says whether it holds the ehr_id, so its owner is unknown (§11.5, §12.5.1)",
        Unanswered(.silent)
    )]
    Unknown {
        /// Each member that gave no answer, and how, in `node_id` order.
        silent: Vec<(EndpointId, Silence)>,
        /// The code the worst of them answers with.
        code: Code,
    },
}

impl Unsettled {
    /// The stable code the error body names.
    ///
    /// Of the members that gave no answer, a failure on the gateway's own
    /// side answers `500`, a time-out or an unreachable member `504`, and a
    /// member's error `424`, in that precedence, the order N37 sets for a
    /// failed query between `504` and `424`.
    #[must_use]
    pub fn code(&self) -> Code {
        match self {
            Self::Nowhere => Code::NoDestination,
            Self::Unavailable => Code::SubjectUnavailable,
            Self::Claimed(_) => Code::EhrIdCollision,
            Self::Unknown { code, .. } => *code,
        }
    }
}

/// The rank of a code among the failures of [`Unsettled::Unknown`], lowest
/// first.
fn precedence(code: Code) -> u8 {
    match code {
        Code::Internal => 0,
        Code::NodeTimeout => 1,
        Code::NodeUnreachable => 2,
        Code::NodeRefused => 3,
        _ => 4,
    }
}

/// How a member failed to answer the probe.
#[derive(Debug)]
pub enum Silence {
    /// It answered a status that is neither a success nor `404`.
    Erred(StatusCode),
    /// It did not answer before its deadline, or before the overall budget
    /// ran out, or the deadline passed before the probe was sent: whether
    /// it holds the EHR is unknown (§11.5).
    TimedOut,
    /// It could not be reached.
    Unreachable,
    /// It refused the gateway's onward credentials.
    Refused,
    /// No onward credential could be obtained for it, so it was sent
    /// nothing (§13.1, N25).
    Unauthenticated,
    /// The probe was never sent, for a reason on the gateway's side.
    Unsent(ForwardError),
}

impl Silence {
    /// The code the silence answers with on its own.
    #[must_use]
    pub fn code(&self) -> Code {
        match self {
            Self::Erred(_) | Self::Unauthenticated => Code::NodeError,
            Self::TimedOut => Code::NodeTimeout,
            Self::Unreachable => Code::NodeUnreachable,
            Self::Refused => Code::NodeRefused,
            Self::Unsent(_) => Code::Internal,
        }
    }
}

impl fmt::Display for Silence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Erred(status) => write!(f, "answered {}", status.as_u16()),
            Self::TimedOut => f.write_str("timed out"),
            Self::Unreachable => f.write_str("could not be reached"),
            Self::Refused => f.write_str("refused the onward credentials"),
            Self::Unauthenticated => {
                f.write_str("was sent nothing: no onward credential could be obtained")
            }
            Self::Unsent(_) => f.write_str("was not sent the probe"),
        }
    }
}

/// What every member's answer to the probe settles.
///
/// Two members holding the `ehr_id` are a collision whatever the others
/// answered. Otherwise every member must have answered: one that did not may
/// hold it too, so neither one claimant nor none is an answer then.
///
/// A member's consent refusal is its `403` where the deployment discloses
/// consent exclusions. Where it does not, the member is read as one that does
/// not hold the `ehr_id`, and a probe no member answers with the EHR is
/// [`Unsettled::Unavailable`] whatever the reason, so the answer never shows
/// a restriction (Regulation (EU) 2025/327 Art 8).
#[must_use]
pub fn settled(answers: Vec<(EndpointId, Answer)>, disclosed: bool) -> Settled {
    let mut holders = Vec::new();
    let mut silent = Vec::new();
    for (endpoint, answer) in answers {
        let silence = match answer {
            Answer::Holds(forwarded) => {
                holders.push((endpoint, forwarded));
                continue;
            }
            Answer::Absent => continue,
            Answer::ConsentRefused if !disclosed => continue,
            Answer::ConsentRefused => Silence::Erred(StatusCode::FORBIDDEN),
            Answer::Erred(status) => Silence::Erred(status),
            Answer::Abandoned
            | Answer::Failed(ForwardError::TimeOut { .. } | ForwardError::Expired { .. }) => {
                Silence::TimedOut
            }
            Answer::Failed(ForwardError::Unreachable { .. }) => Silence::Unreachable,
            Answer::Failed(ForwardError::Refused { .. }) => Silence::Refused,
            Answer::Failed(ForwardError::Credentials { .. }) => Silence::Unauthenticated,
            Answer::Failed(failure) => Silence::Unsent(failure),
        };
        silent.push((endpoint, silence));
    }
    if holders.len() > 1 {
        return Settled::Failed(Unsettled::Claimed(
            holders.into_iter().map(|(endpoint, _)| endpoint).collect(),
        ));
    }
    if let Some(code) = silent
        .iter()
        .map(|(_, silence)| silence.code())
        .min_by_key(|code| precedence(*code))
    {
        return Settled::Failed(Unsettled::Unknown { silent, code });
    }
    match holders.pop() {
        Some((endpoint, answer)) => Settled::Owner { endpoint, answer },
        None if disclosed => Settled::Failed(Unsettled::Nowhere),
        None => Settled::Failed(Unsettled::Unavailable),
    }
}

/// Endpoint ids as a message lists them: `[a, b]`.
///
/// Every id is the registry's own, so the message quotes no client text
/// (§5.4.3).
pub(crate) struct Listed<'a>(pub(crate) &'a [EndpointId]);

impl fmt::Display for Listed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (index, endpoint) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            f.write_str(endpoint.as_str())?;
        }
        f.write_str("]")
    }
}

/// The silent members as a message names them: `endpoint a (timed out),
/// endpoint b (answered 503)`.
struct Unanswered<'a>(&'a [(EndpointId, Silence)]);

impl fmt::Display for Unanswered<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, (endpoint, silence)) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "endpoint {endpoint} ({silence})")?;
        }
        Ok(())
    }
}
