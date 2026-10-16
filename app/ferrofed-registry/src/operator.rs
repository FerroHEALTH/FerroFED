// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bodies of the gateway's read-only operator surface.
//!
//! The gateway writes and the operator console reads the integrity
//! incidents, the `creating_system_id` routing table, pages of the
//! stored-query registry, each version marked read-only or not, the
//! deployment report, and the body of the
//! operator's clear of an `ehr_id` on the admin listener. Every report
//! carries routing ids, counts, stored definitions and the deployment
//! profile only. An `ehr_id` is named only when it is a bare
//! UUID, which cannot spell a patient identifier, and a stored definition
//! names no patient, because one that does is refused before it is held
//! (§5.4.1, N33, §12.7). Each report of the surface names the
//! [`Replica`] that answered it, because its state is that process's own. No specification governs the operator surface: our
//! own design.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Mutex, PoisonError};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::creating_system::LearnedMap;
use crate::id::{EhrId, NodeId};
use crate::incident::{Incident, Kind};
use crate::replica::Replica;
use crate::snapshot::RegistrySnapshot;

/// How many incidents of each kind the report keeps, newest last: a burst of
/// one kind never pushes another kind's incidents out.
pub const RECENT_PER_KIND: usize = 25;

/// The most a page of the operator surface holds, and the size of a page a
/// request does not size.
pub const MAX_PAGE: u64 = 100;

/// The incidents emitted since the process started, by kind, each kind's
/// newest last and at most [`RECENT_PER_KIND`] of them.
static RECENT: Mutex<BTreeMap<Kind, VecDeque<(Timestamp, Incident)>>> = Mutex::new(BTreeMap::new());

/// Keeps `incident`, emitted now, among the recent ones of its kind,
/// dropping that kind's oldest beyond [`RECENT_PER_KIND`].
pub(crate) fn record(incident: &Incident) {
    // NOTE: no specification governs this: our own design; a lock a panic
    // poisoned still holds whole entries, so recording goes on.
    let mut recent = RECENT.lock().unwrap_or_else(PoisonError::into_inner);
    let kind = recent.entry(incident.kind()).or_default();
    while kind.len() >= RECENT_PER_KIND {
        kind.pop_front();
    }
    kind.push_back((Timestamp::now(), incident.clone()));
}

/// The body of `GET {base}/operator/incidents`.
///
/// It is one page by construction: at most [`RECENT_PER_KIND`] incidents of
/// each of the four kinds, never more than [`MAX_PAGE`] in all, and at most
/// [`RECENT_PER_KIND`] cleared collisions beside them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IncidentReport {
    /// How many incidents of each kind were emitted since the process
    /// started, every kind listed.
    pub counts: BTreeMap<String, u64>,
    /// The most recent incidents of every kind, oldest first.
    pub recent: Vec<RecordedIncident>,
    /// The collisions the operator cleared most recently, oldest first: an
    /// incident is an event, so a clear is listed beside it and removes
    /// none.
    pub cleared: Vec<RecordedClear>,
    /// The replica whose incidents these are, which the operator surface
    /// names; absent from a report no surface answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replica: Option<String>,
}

impl IncidentReport {
    /// The report of this process: every kind's count and the incidents
    /// the process keeps of each kind.
    #[must_use]
    pub fn current() -> Self {
        let counts = Kind::ALL
            .iter()
            .map(|kind| (kind.as_str().to_owned(), kind.emitted()))
            .collect();
        let mut held: Vec<(Timestamp, Incident)> = RECENT
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .flatten()
            .cloned()
            .collect();
        held.sort_by_key(|(at, _)| *at);
        let recent = held
            .iter()
            .map(|(at, incident)| RecordedIncident::of(incident, *at))
            .collect();
        let cleared = CLEARED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect();
        Self {
            counts,
            recent,
            cleared,
            replica: None,
        }
    }
}

/// One incident as the operator surface names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedIncident {
    /// The incident's kind, a [`Kind`] name.
    pub kind: String,
    /// When it was emitted, RFC 3339.
    pub at: String,
    /// The incident's own description, which names an `ehr_id` only when it
    /// is a bare UUID.
    pub description: String,
    /// The `creating_system_id` it is about, for a routing-table conflict.
    pub creating_system_id: Option<String>,
    /// The `ehr_id` it is about, only when it is a bare UUID.
    pub ehr_id: Option<String>,
    /// How the gateway found it, for an `ehr_id` collision.
    pub detection: Option<String>,
    /// The endpoints involved.
    pub endpoints: Vec<String>,
    /// The nodes involved.
    pub nodes: Vec<String>,
}

impl RecordedIncident {
    /// The record of `incident`, emitted `at`.
    #[must_use]
    pub fn of(incident: &Incident, at: Timestamp) -> Self {
        let (detection, endpoints, nodes) = match incident {
            Incident::LearnedCreatingSystemConflict { first, second, .. } => (
                None,
                vec![first.to_string(), second.to_string()],
                Vec::new(),
            ),
            Incident::RegisteredCreatingSystemConflict {
                registered,
                learned,
                ..
            } => (
                None,
                vec![learned.to_string()],
                vec![registered.to_string()],
            ),
            Incident::EhrIdCollision {
                detection,
                claimants,
                ..
            } => (
                Some(detection.as_str().to_owned()),
                claimants.iter().map(ToString::to_string).collect(),
                Vec::new(),
            ),
            Incident::IndexInsertCollision { claimants, .. } => (
                None,
                Vec::new(),
                claimants.iter().map(ToString::to_string).collect(),
            ),
        };
        Self {
            kind: incident.kind().as_str().to_owned(),
            at: at.to_string(),
            description: incident.to_string(),
            creating_system_id: incident.creating_system_id().map(ToString::to_string),
            ehr_id: incident
                .ehr_id()
                .filter(|ehr_id| ehr_id.is_uuid())
                .map(ToString::to_string),
            detection,
            endpoints,
            nodes,
        }
    }
}

/// What this process held for an `ehr_id` before the operator cleared it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeldBefore {
    /// The index or the session bindings named two members or more: a
    /// collision (§12.5.2, N42).
    Collision,
    /// They named one member only.
    Owner,
    /// Neither held the `ehr_id`.
    Nothing,
}

/// Where a clear took effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClearScope {
    /// This process alone: each replica holds its own index and bindings,
    /// so a deployment of several replicas is cleared at each of them.
    Replica,
}

/// The body of the operator's clear of an `ehr_id` on the admin listener:
/// what this process forgot.
///
/// It names the `ehr_id` only when it is a bare UUID, and members by
/// `node_id`, never a patient identifier (§5.4.1, N33).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearedEhrId {
    /// The `ehr_id` cleared, only when it is a bare UUID.
    pub ehr_id: Option<String>,
    /// What the process held for it before the clear.
    pub held: HeldBefore,
    /// Every member the index or a binding named, in `node_id` order.
    pub claimants: Vec<String>,
    /// Whether the `ehr_id` index held an entry for it.
    pub indexed: bool,
    /// How many client sessions held a binding of it.
    pub sessions: usize,
    /// Where the clear took effect.
    pub scope: ClearScope,
}

impl ClearedEhrId {
    /// The clear of `ehr_id`, which forgot the index entry naming `indexed`
    /// and bindings in `sessions` sessions naming `bound`.
    #[must_use]
    pub fn of(
        ehr_id: &EhrId,
        indexed: &[NodeId],
        (sessions, bound): (usize, &BTreeSet<NodeId>),
    ) -> Self {
        let claimants: BTreeSet<&NodeId> = indexed.iter().chain(bound).collect();
        let held = match claimants.len() {
            0 if sessions == 0 => HeldBefore::Nothing,
            0 | 1 => HeldBefore::Owner,
            _ => HeldBefore::Collision,
        };
        Self {
            ehr_id: ehr_id.is_uuid().then(|| ehr_id.to_string()),
            held,
            claimants: claimants.into_iter().map(ToString::to_string).collect(),
            indexed: !indexed.is_empty(),
            sessions,
            scope: ClearScope::Replica,
        }
    }

    /// Keeps the clear, made now, among the recent clears the incident
    /// report lists, when it cleared a collision; dropping the oldest beyond
    /// [`RECENT_PER_KIND`].
    pub fn record(&self) {
        if self.held != HeldBefore::Collision {
            return;
        }
        let mut cleared = CLEARED.lock().unwrap_or_else(PoisonError::into_inner);
        while cleared.len() >= RECENT_PER_KIND {
            cleared.pop_front();
        }
        cleared.push_back(RecordedClear {
            at: Timestamp::now().to_string(),
            ehr_id: self.ehr_id.clone(),
            claimants: self.claimants.clone(),
        });
    }
}

/// The collisions the operator cleared since the process started, newest
/// last and at most [`RECENT_PER_KIND`] of them.
static CLEARED: Mutex<VecDeque<RecordedClear>> = Mutex::new(VecDeque::new());

/// One collision the operator cleared, as the incident report lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedClear {
    /// When it was cleared, RFC 3339.
    pub at: String,
    /// The `ehr_id` cleared, only when it is a bare UUID.
    pub ehr_id: Option<String>,
    /// The members the process held it at, in `node_id` order.
    pub claimants: Vec<String>,
}

/// Where a `creating_system_id` routes, and on what ground (N21, §12.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteSource {
    /// A member's own `system_id`.
    Member,
    /// A `[[creating_system]]` mapping of the registry document.
    Registered,
    /// A mapping learned from answers.
    Learned,
    /// A learned mapping withdrawn by an incident, which routes nowhere.
    Withdrawn,
}

/// One row of the `creating_system_id` routing table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatingSystemEntry {
    /// The `creating_system_id`, as the registry or the answer spells it.
    pub creating_system_id: String,
    /// Where the route comes from.
    pub source: RouteSource,
    /// The node it routes to, absent for a withdrawn mapping.
    pub node: Option<String>,
    /// The endpoint it routes through, absent for a member's own `system_id`
    /// and a withdrawn mapping.
    pub endpoint: Option<String>,
}

/// The routing table `snapshot` and `learned` hold together: the members'
/// own `system_id`s first, then the registry document's mappings, then the
/// learned ones, each ordered by id.
#[must_use]
pub fn routing_table(
    snapshot: &RegistrySnapshot,
    learned: &LearnedMap,
) -> Vec<CreatingSystemEntry> {
    let members = snapshot.nodes().map(|node| CreatingSystemEntry {
        creating_system_id: node.system_id().to_string(),
        source: RouteSource::Member,
        node: Some(node.id().to_string()),
        endpoint: None,
    });
    let registered = snapshot
        .creating_systems()
        .map(|(creating_system_id, endpoint)| CreatingSystemEntry {
            creating_system_id: creating_system_id.to_string(),
            source: RouteSource::Registered,
            node: snapshot
                .endpoint(endpoint)
                .map(|declared| declared.node().to_string()),
            endpoint: Some(endpoint.to_string()),
        });
    let learned = learned
        .entries()
        .map(|(creating_system_id, holder)| match holder {
            Some(endpoint) => CreatingSystemEntry {
                creating_system_id: creating_system_id.to_string(),
                source: RouteSource::Learned,
                node: snapshot
                    .endpoint(endpoint)
                    .map(|declared| declared.node().to_string()),
                endpoint: Some(endpoint.to_string()),
            },
            None => CreatingSystemEntry {
                creating_system_id: creating_system_id.to_string(),
                source: RouteSource::Withdrawn,
                node: None,
                endpoint: None,
            },
        });
    members.chain(registered).chain(learned).collect()
}

/// Which page of a listing a request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageRequest {
    /// How many items to skip.
    pub offset: u64,
    /// How many items to answer, at most [`MAX_PAGE`].
    pub limit: u64,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: MAX_PAGE,
        }
    }
}

impl PageRequest {
    /// Whether the request asks for a page the surface answers: at least one
    /// item and at most [`MAX_PAGE`].
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.limit >= 1 && self.limit <= MAX_PAGE
    }
}

/// The deployment profile a gateway runs under, as the operator surface
/// names it.
// NOTE: no specification governs this: our own design; the set is the configuration's
// `profile`, and a deployment marked for nothing else is never reported as one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeploymentMode {
    /// A deployment that serves real requests, `profile = "production"`.
    #[default]
    Production,
    /// A deployment marked for development and testing,
    /// `profile = "development"`, which admits what production refuses,
    /// such as the static cross-reference and cleartext credentials.
    Development,
}

/// The body of `GET {base}/operator/deployment`: how this process is
/// deployed, so a console can warn that it reads a development gateway.
///
/// It names the profile, whether patients resolve through the static
/// development cross-reference, the product and its version, and the
/// replica. It carries no setting value, URL, credential or identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentReport {
    /// The deployment profile the process started with, which only a
    /// restart changes.
    pub mode: DeploymentMode,
    /// Whether the federation resolves patients through the static
    /// development cross-reference (`[dev]`), which only the development
    /// profile admits.
    pub development_crossref: bool,
    /// The product name, as `GET {base}/` reports it.
    pub product: String,
    /// The product version, as `GET {base}/` reports it.
    pub version: String,
    /// The replica that answered.
    pub replica: String,
}

/// One stored-query version as the operator surface lists it, with whether
/// the gateway takes a write under its name.
///
/// `Q` is the ITS-REST `StoredQuery` of `openehr-its`, flattened, so a
/// reader of the plain ITS-REST shape reads `read_only` as one more member.
// NOTE: no specification governs this: our own design; the flag spares a reader
// a copy of the gateway's reserved-namespace and read-only-store rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredVersion<Q> {
    /// The version as ITS-REST names it.
    #[serde(flatten)]
    pub definition: Q,
    /// Whether the gateway refuses a write under this name: it sits in the
    /// gateway's reserved namespace, or the store refuses every insert, as
    /// the read-only definition files do.
    pub read_only: bool,
}

/// One page of a listing, with where it starts and how many there are in
/// all, so a reader always knows when it holds only part of the listing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    /// The items of this page, in the listing's order.
    pub items: Vec<T>,
    /// Where this page starts in the listing.
    pub offset: u64,
    /// How many items the whole listing holds.
    pub total: u64,
    /// The replica that answered the page, for a listing of per-process
    /// state or one read through a replica; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replica: Option<String>,
}

impl<T> Page<T> {
    /// The page `request` selects of `all`.
    #[must_use]
    pub fn of(all: Vec<T>, request: PageRequest) -> Self {
        let total = u64::try_from(all.len()).unwrap_or(u64::MAX);
        let skip = usize::try_from(request.offset).unwrap_or(usize::MAX);
        let take = usize::try_from(request.limit).unwrap_or(usize::MAX);
        Self {
            items: all.into_iter().skip(skip).take(take).collect(),
            offset: request.offset,
            total,
            replica: None,
        }
    }

    /// Returns the page naming `replica` as the process that answered it.
    #[must_use]
    pub fn answered_by(mut self, replica: &Replica) -> Self {
        self.replica = Some(replica.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        ClearScope, ClearedEhrId, HeldBefore, IncidentReport, MAX_PAGE, Page, PageRequest,
        RECENT_PER_KIND, RecordedClear, RecordedIncident, record,
    };
    use crate::id::{EhrId, NodeId};
    use crate::incident::{Detection, Incident, Kind};
    use jiff::Timestamp;

    #[test]
    fn an_ehr_id_that_is_no_uuid_is_never_named() {
        let incident = Incident::IndexInsertCollision {
            ehr_id: "synthetic-patient-9999".parse().expect("an ehr_id"),
            claimants: vec!["node-a".parse().expect("a node")],
        };
        let recorded = RecordedIncident::of(&incident, Timestamp::UNIX_EPOCH);
        assert_eq!(None, recorded.ehr_id);
        assert!(
            !recorded.description.contains("synthetic-patient-9999"),
            "{}",
            recorded.description
        );
    }

    #[test]
    fn a_recorded_incident_names_its_uuid_ehr_id_and_its_claimants() {
        let incident = Incident::EhrIdCollision {
            ehr_id: "7d44b88c-4199-4bad-97dc-d78268e01398"
                .parse()
                .expect("an ehr_id"),
            detection: Detection::AskAll,
            claimants: vec!["node-a-pub".parse().expect("an endpoint")],
        };
        let recorded = RecordedIncident::of(&incident, Timestamp::UNIX_EPOCH);
        assert_eq!(
            Some("7d44b88c-4199-4bad-97dc-d78268e01398"),
            recorded.ehr_id.as_deref()
        );
        assert_eq!(Some("ask-all"), recorded.detection.as_deref());
        assert_eq!(vec!["node-a-pub".to_owned()], recorded.endpoints);
    }

    #[test]
    fn a_burst_of_one_kind_keeps_the_other_kinds_and_stays_bounded() {
        record(&Incident::LearnedCreatingSystemConflict {
            creating_system_id: "legacy-x.example.org".parse().expect("a system id"),
            first: "node-a-pub".parse().expect("an endpoint"),
            second: "node-b-pub".parse().expect("an endpoint"),
        });
        let collision = Incident::EhrIdCollision {
            ehr_id: "7d44b88c-4199-4bad-97dc-d78268e01398"
                .parse()
                .expect("an ehr_id"),
            detection: Detection::Index,
            claimants: vec!["node-a-pub".parse().expect("an endpoint")],
        };
        for _ in 0..1000 {
            record(&collision);
        }
        let report = IncidentReport::current();
        assert_eq!(4, report.counts.len());
        let collisions = report
            .recent
            .iter()
            .filter(|recorded| recorded.kind == "EhrIdCollision")
            .count();
        assert_eq!(RECENT_PER_KIND, collisions);
        assert!(
            report
                .recent
                .iter()
                .any(|recorded| recorded.creating_system_id.as_deref()
                    == Some("legacy-x.example.org")),
            "{report:?}"
        );
    }

    #[test]
    fn a_clear_says_what_was_held_and_only_a_cleared_collision_is_listed() {
        let ehr_id: EhrId = "6f0e5b1a-2c3d-4e5f-8a9b-0c1d2e3f4a5b"
            .parse()
            .expect("an ehr_id");
        let node = |id: &str| id.parse::<NodeId>().expect("a node");
        let nothing = ClearedEhrId::of(&ehr_id, &[], (0, &BTreeSet::new()));
        assert_eq!(
            (HeldBefore::Nothing, false, 0),
            (nothing.held, nothing.indexed, nothing.sessions)
        );
        let owner = ClearedEhrId::of(&ehr_id, &[node("node-a")], (1, &BTreeSet::new()));
        assert_eq!(HeldBefore::Owner, owner.held);
        let collision = ClearedEhrId::of(
            &ehr_id,
            &[node("node-b")],
            (1, &BTreeSet::from([node("node-a")])),
        );
        assert_eq!(HeldBefore::Collision, collision.held);
        assert_eq!(vec!["node-a", "node-b"], collision.claimants);
        assert_eq!(ClearScope::Replica, collision.scope);
        for clear in [&nothing, &owner, &collision] {
            clear.record();
        }
        let listed: Vec<RecordedClear> = IncidentReport::current()
            .cleared
            .into_iter()
            .filter(|cleared| cleared.ehr_id.as_deref() == Some(ehr_id.as_str()))
            .collect();
        assert_eq!(1, listed.len(), "{listed:?}");
        let unnamed: EhrId = "synthetic-patient-9999".parse().expect("an ehr_id");
        let hidden = ClearedEhrId::of(&unnamed, &[node("node-a")], (0, &BTreeSet::new()));
        assert_eq!(None, hidden.ehr_id, "only a bare UUID is named");
    }

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Definition {
        name: String,
    }

    #[test]
    fn a_stored_version_adds_read_only_beside_the_definitions_own_members() {
        let listed = super::StoredVersion {
            definition: Definition {
                name: "org.example::q".to_owned(),
            },
            read_only: true,
        };
        let text = serde_json::to_string(&listed).expect("a body");
        assert_eq!(r#"{"name":"org.example::q","read_only":true}"#, text);
        let read: super::StoredVersion<Definition> =
            serde_json::from_str(&text).expect("read back");
        assert_eq!(listed, read);
    }

    #[test]
    fn a_report_without_clears_still_reads() {
        let report: IncidentReport =
            serde_json::from_str(r#"{"counts":{},"recent":[]}"#).expect("a report");
        assert!(report.cleared.is_empty());
    }

    #[test]
    fn the_incident_report_never_holds_more_than_a_page() {
        let most = RECENT_PER_KIND * Kind::ALL.len();
        let page = usize::try_from(MAX_PAGE).expect("a page fits a usize");
        assert!(
            most <= page,
            "{most} recent incidents over a page of {page}"
        );
    }

    #[test]
    fn a_page_says_where_it_starts_and_how_many_there_are() {
        let page = Page::of(
            (0..250).collect::<Vec<u32>>(),
            PageRequest {
                offset: 200,
                limit: MAX_PAGE,
            },
        );
        assert_eq!(50, page.items.len());
        assert_eq!(Some(&200), page.items.first());
        assert_eq!(200, page.offset);
        assert_eq!(250, page.total);
        assert!(PageRequest::default().is_valid());
        assert!(
            !PageRequest {
                offset: 0,
                limit: MAX_PAGE + 1
            }
            .is_valid()
        );
        assert!(
            !PageRequest {
                offset: 0,
                limit: 0
            }
            .is_valid()
        );
    }
}
