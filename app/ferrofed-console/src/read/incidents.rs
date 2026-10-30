// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The integrity incidents the replica keeps, split between the open and
//! the resolved ones.
//!
//! An incident is an event, and a clear removes none: an incident reads as
//! resolved once the operator cleared the `ehr_id` it is about after it was
//! emitted, and open otherwise. The report names an `ehr_id` only when it
//! is a bare UUID, a routing id and never a patient identifier (§5.4.1,
//! N33).

use std::collections::BTreeMap;

use ferrofed_registry::operator::{IncidentReport, RecordedClear, RecordedIncident};

use serde::{Deserialize, Serialize};

use crate::read::Stop;
use crate::read::signed_in;

/// One incident.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    /// Its kind, such as `EhrIdCollision`.
    pub kind: String,
    /// When it was emitted, RFC 3339.
    pub at: String,
    /// Its description.
    pub description: String,
    /// The `ehr_id` it is about, only when it is a bare UUID.
    pub ehr_id: Option<String>,
    /// The `creating_system_id` it is about, for a routing conflict.
    pub creating_system_id: Option<String>,
    /// The nodes involved.
    pub nodes: Vec<String>,
    /// The clear that resolved it, when one did.
    pub cleared: Option<RecordedClear>,
}

/// The incidents of one report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incidents {
    /// The open ones, newest first.
    pub open: Vec<Incident>,
    /// The resolved ones, newest first.
    pub resolved: Vec<Incident>,
    /// How many of each kind the replica saw since it started.
    pub counts: BTreeMap<String, u64>,
    /// The replica that answered, as its report names it.
    pub replica: Option<String>,
}

impl Incidents {
    /// The incidents of `report`.
    #[must_use]
    pub fn of(report: &IncidentReport) -> Self {
        let (mut open, mut resolved) = (Vec::new(), Vec::new());
        for recorded in report.recent.iter().rev() {
            let incident = Incident {
                kind: recorded.kind.clone(),
                at: recorded.at.clone(),
                description: recorded.description.clone(),
                ehr_id: recorded.ehr_id.clone(),
                creating_system_id: recorded.creating_system_id.clone(),
                nodes: recorded.nodes.clone(),
                cleared: clear_of(recorded, &report.cleared),
            };
            if incident.cleared.is_some() {
                resolved.push(incident);
            } else {
                open.push(incident);
            }
        }
        Self {
            open,
            resolved,
            counts: report.counts.clone(),
            replica: report.replica.clone(),
        }
    }

    /// The `ehr_id`s the open incidents hold, each once, in order.
    #[must_use]
    pub fn held(&self) -> Vec<String> {
        let mut held: Vec<String> = self
            .open
            .iter()
            .filter_map(|incident| incident.ehr_id.clone())
            .collect();
        held.sort();
        held.dedup();
        held
    }
}

/// The clear of the `ehr_id` `incident` is about, at or after the time it
/// was emitted, when one of `cleared` is.
fn clear_of(incident: &RecordedIncident, cleared: &[RecordedClear]) -> Option<RecordedClear> {
    let ehr_id = incident.ehr_id.as_deref()?;
    // NOTE: no specification governs this: our own design; a time that does not parse
    // proves no clear came after, so the incident stays open.
    let emitted = incident.at.parse::<jiff::Timestamp>().ok()?;
    cleared
        .iter()
        .find(|clear| {
            clear.ehr_id.as_deref() == Some(ehr_id)
                && clear
                    .at
                    .parse::<jiff::Timestamp>()
                    .is_ok_and(|at| at >= emitted)
        })
        .cloned()
}

/// Reads the incidents as the operator of the request.
///
/// # Errors
/// Returns the [`Stop`] of a missing session, or of a read the gateway
/// refused or did not answer.
pub async fn read() -> Result<Incidents, Stop> {
    let (state, token) = signed_in()?;
    let report = state.gateway().incidents(&token).await?;
    Ok(Incidents::of(&report))
}

#[cfg(test)]
mod tests {
    use ferrofed_registry::operator::{IncidentReport, RecordedClear, RecordedIncident};

    use super::Incidents;

    fn incident(at: &str, ehr_id: Option<&str>) -> RecordedIncident {
        RecordedIncident {
            kind: String::from("EhrIdCollision"),
            at: at.to_owned(),
            description: String::from("two nodes claim one EHR id"),
            creating_system_id: None,
            ehr_id: ehr_id.map(str::to_owned),
            detection: None,
            endpoints: Vec::new(),
            nodes: vec![String::from("node-a"), String::from("node-b")],
        }
    }

    #[test]
    fn an_incident_is_resolved_only_by_a_clear_of_its_ehr_id_after_it() {
        let id = "8c3e8f3a-1b2c-4d5e-8f90-123456789abc";
        let report = IncidentReport {
            counts: std::collections::BTreeMap::new(),
            recent: vec![
                incident("2026-10-21T08:00:00Z", Some(id)),
                incident("2026-10-21T10:00:00Z", Some(id)),
                incident("2026-10-21T08:30:00Z", None),
            ],
            cleared: vec![RecordedClear {
                at: String::from("2026-10-21T09:00:00Z"),
                ehr_id: Some(id.to_owned()),
                claimants: vec![String::from("node-a")],
            }],
            replica: None,
        };
        let incidents = Incidents::of(&report);
        assert_eq!(1, incidents.resolved.len());
        assert_eq!(2, incidents.open.len());
        assert_eq!(vec![id.to_owned()], incidents.held());
    }
}
