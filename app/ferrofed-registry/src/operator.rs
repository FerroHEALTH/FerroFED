// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bodies of the gateway's read-only operator surface.
//!
//! The gateway writes and the operator console reads three JSON reports:
//! the integrity incidents, the `creating_system_id` routing table, and the
//! stored-query registry. Every report carries routing ids, counts and stored definitions only. An
//! `ehr_id` is named only when it is a bare UUID, which cannot spell a
//! patient identifier, and a stored definition names no patient, because one
//! that does is refused before it is held (§5.4.1, N33, §12.7). No
//! specification governs the operator surface: our own design.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::creating_system::LearnedMap;
use crate::definition::StoredDefinition;
use crate::incident::{Incident, Kind};
use crate::snapshot::RegistrySnapshot;

/// How many incidents the operator report keeps, newest last.
pub const RECENT_INCIDENTS: usize = 100;

/// The incidents emitted since the process started, newest last, at most
/// [`RECENT_INCIDENTS`] of them.
static RECENT: Mutex<VecDeque<(Timestamp, Incident)>> = Mutex::new(VecDeque::new());

/// Keeps `incident`, emitted now, among the recent ones, dropping the oldest
/// beyond [`RECENT_INCIDENTS`].
pub(crate) fn record(incident: &Incident) {
    // NOTE: no specification governs this: our own design; a store unusable
    // after a panic keeps nothing more, while the count and the event still go out.
    if let Ok(mut recent) = RECENT.lock() {
        while recent.len() >= RECENT_INCIDENTS {
            recent.pop_front();
        }
        recent.push_back((Timestamp::now(), incident.clone()));
    }
}

/// The body of `GET {base}/operator/incidents`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentReport {
    /// How many incidents of each kind were emitted since the process
    /// started, every kind listed.
    pub counts: BTreeMap<String, u64>,
    /// The most recent incidents, newest last.
    pub recent: Vec<RecordedIncident>,
}

impl IncidentReport {
    /// The report of this process: every kind's count and the incidents
    /// the process keeps.
    #[must_use]
    pub fn current() -> Self {
        let counts = Kind::ALL
            .iter()
            .map(|kind| (kind.as_str().to_owned(), kind.emitted()))
            .collect();
        let recent = RECENT
            .lock()
            .map(|recent| {
                recent
                    .iter()
                    .map(|(at, incident)| RecordedIncident::of(incident, *at))
                    .collect()
            })
            .unwrap_or_default();
        Self { counts, recent }
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

/// The body of `GET {base}/operator/creating-systems`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatingSystemReport {
    /// Every route, the members' own `system_id`s first, then the registry
    /// document's mappings, then the learned ones, each ordered by id.
    pub entries: Vec<CreatingSystemEntry>,
}

impl CreatingSystemReport {
    /// The routing table `snapshot` and `learned` hold together.
    #[must_use]
    pub fn of(snapshot: &RegistrySnapshot, learned: &LearnedMap) -> Self {
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
        Self {
            entries: members.chain(registered).chain(learned).collect(),
        }
    }
}

/// One held version of a stored query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredQueryEntry {
    /// The qualified name.
    pub name: String,
    /// The `major.minor.patch` version.
    pub version: String,
    /// When it was stored, RFC 3339.
    pub saved: String,
    /// The AQL text, which names no patient.
    pub aql: String,
}

impl From<&StoredDefinition> for StoredQueryEntry {
    fn from(definition: &StoredDefinition) -> Self {
        Self {
            name: definition.name().to_string(),
            version: definition.version().to_string(),
            saved: definition.saved().to_string(),
            aql: definition.aql().to_owned(),
        }
    }
}

/// The body of `GET {base}/operator/stored-queries`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredQueryReport {
    /// Every held version, by name and then by version.
    pub definitions: Vec<StoredQueryEntry>,
}

#[cfg(test)]
mod tests {
    use super::{IncidentReport, RecordedIncident, record};
    use crate::incident::{Detection, Incident};
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
    fn the_report_counts_every_kind_and_keeps_the_recent_ones() {
        let incident = Incident::LearnedCreatingSystemConflict {
            creating_system_id: "legacy-x.example.org".parse().expect("a system id"),
            first: "node-a-pub".parse().expect("an endpoint"),
            second: "node-b-pub".parse().expect("an endpoint"),
        };
        record(&incident);
        let report = IncidentReport::current();
        assert_eq!(4, report.counts.len());
        assert!(
            report
                .recent
                .iter()
                .any(|recorded| recorded.creating_system_id.as_deref()
                    == Some("legacy-x.example.org")),
            "{report:?}"
        );
    }
}
