// SPDX-FileCopyrightText: Vernum Projecten B.V.
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
//! incident of N42 ([`collided`]). The `ehr_id` in the path is the node-local
//! identifier N33 locates a node by, and it is the only value of the request
//! any step reads.

use std::fmt;
use std::time::Instant;

use ferrofed_engine::forward::{ForwardError, Forwarded};
use ferrofed_engine::probe::Answer;
use ferrofed_identity::binding::{Bound, ResolutionBindings, SessionKey};
use ferrofed_registry::ehr_index::{EhrIndex, Indexed};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::incident::{Detection, Incident};
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus, RegistrySnapshot};
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
/// A binding or an index entry naming a member the snapshot no longer holds
/// names nothing, and the next step is taken. A step naming two members the
/// snapshot holds is a [`Located::Collision`], and no later step is taken: no
/// step picks one of two claimants (§12.5.2, N42).
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
    if let Some(endpoint) = target(snapshot, headers)? {
        return Ok(Located::At {
            endpoint,
            step: Step::Target,
        });
    }
    if let Some(held) = held {
        let bound = match held.bindings.lookup(held.session, held.now, ehr_id) {
            Bound::One(node) => vec![node],
            Bound::Several(nodes) => nodes,
            Bound::None => Vec::new(),
        };
        if let Some(located) = among(snapshot, &bound, Step::Binding, Detection::Binding) {
            return Ok(located);
        }
    }
    let indexed = match index.lookup(ehr_id) {
        Indexed::One(node) => vec![node],
        Indexed::Several(nodes) => nodes,
        Indexed::None => Vec::new(),
    };
    Ok(among(snapshot, &indexed, Step::Index, Detection::Index).unwrap_or(Located::Unknown))
}

/// What a step naming `nodes` says, counting only the members the snapshot
/// holds: `None` when it holds none of them.
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
    let reached = reached_through(snapshot, node).or_else(|| {
        snapshot
            .endpoints()
            .find(|endpoint| endpoint.node() == node)
    });
    Some(match reached {
        Some(endpoint) => Located::At { endpoint, step },
        None => Located::Unreachable { step },
    })
}

/// The endpoint a member is asked through: its first active endpoint in
/// `endpoint_id` order, as a federated query asks it.
#[must_use]
pub fn reached_through<'a>(snapshot: &'a RegistrySnapshot, node: &NodeId) -> Option<&'a Endpoint> {
    snapshot
        .endpoints()
        .find(|endpoint| endpoint.node() == node && endpoint.status() == EndpointStatus::Active)
}

/// The endpoint each member is probed through in step 4, in `node_id` order;
/// a member with no active endpoint is never contacted (§11.1).
#[must_use]
pub fn probed(snapshot: &RegistrySnapshot) -> Vec<EndpointId> {
    snapshot
        .nodes()
        .filter_map(|node| reached_through(snapshot, node.id()))
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
/// they select exactly one endpoint (§7a.1, §12.4).
fn target<'a>(
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
    /// ran out.
    TimedOut,
    /// It could not be reached.
    Unreachable,
    /// It refused the gateway's onward credentials.
    Refused,
    /// The probe was never sent, for a reason on the gateway's side.
    Unsent(ForwardError),
}

impl Silence {
    /// The code the silence answers with on its own.
    #[must_use]
    pub fn code(&self) -> Code {
        match self {
            Self::Erred(_) => Code::NodeError,
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
            Self::Unsent(_) => f.write_str("was not sent the probe"),
        }
    }
}

/// What every member's answer to the probe settles.
///
/// Two members holding the `ehr_id` are a collision whatever the others
/// answered. Otherwise every member must have answered: one that did not may
/// hold it too, so neither one claimant nor none is an answer then.
#[must_use]
pub fn settled(answers: Vec<(EndpointId, Answer)>) -> Settled {
    let mut holders = Vec::new();
    let mut silent = Vec::new();
    for (endpoint, answer) in answers {
        let silence = match answer {
            Answer::Holds(forwarded) => {
                holders.push((endpoint, forwarded));
                continue;
            }
            Answer::Absent => continue,
            Answer::Erred(status) => Silence::Erred(status),
            Answer::Abandoned | Answer::Failed(ForwardError::TimeOut { .. }) => Silence::TimedOut,
            Answer::Failed(ForwardError::Unreachable { .. }) => Silence::Unreachable,
            Answer::Failed(ForwardError::Refused { .. }) => Silence::Refused,
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
        None => Settled::Failed(Unsettled::Nowhere),
    }
}

/// Endpoint ids as a message lists them: `[a, b]`.
///
/// Every id is the registry's own, so the message quotes no client text
/// (§5.4.3).
struct Listed<'a>(&'a [EndpointId]);

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
