// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The last state the gateway observed of each member endpoint and of the
//! resolver, which `GET /health/dependencies` reports.
//!
//! Nothing here sends a request: the states come from the requests the
//! gateway already makes for its clients, so a dependency nobody has asked
//! since boot is [`Observed::Unknown`]. The record holds one slot per
//! endpoint of the registry snapshot and one for the resolver, fixed when
//! the federation is built, so it never grows. A dependency's state never
//! gates readiness: under §11 a node outage is reported per query, and
//! readiness that followed it would turn one CDR outage into a total
//! outage. No specification governs health probes: our own design.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};

use ferrofed_engine::dispatch::definition::NodeCopy;
use ferrofed_engine::forward::{ForwardError, Forwarded};
use ferrofed_engine::probe::Answer;
use ferrofed_identity::resolver::Resolution;
use ferrofed_registry::id::{EndpointId, NodeId};
use openehr_federation::meta::FederationMeta;
use openehr_federation::status::EndpointStatus;
use serde::Serialize;

/// The last state observed of one dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Observed {
    /// No request has reached it since the federation was built.
    Unknown,
    /// It answered the last request.
    Up,
    /// It was reached and answered the last request with a failure.
    Failing,
    /// The last request could not reach it or got no answer in time.
    Down,
}

impl Observed {
    /// Returns the byte this state is stored as.
    const fn code(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Up => 1,
            Self::Failing => 2,
            Self::Down => 3,
        }
    }

    /// Returns the state stored as `code`.
    const fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Unknown,
            1 => Self::Up,
            2 => Self::Failing,
            _ => Self::Down,
        }
    }

    /// Returns what a §11.1 endpoint status says of the node, or `None` for
    /// a status no request to the node produced.
    #[must_use]
    pub const fn of_status(status: EndpointStatus) -> Option<Self> {
        match status {
            EndpointStatus::Active => Some(Self::Up),
            EndpointStatus::NodeError => Some(Self::Failing),
            EndpointStatus::Offline | EndpointStatus::TimeOut => Some(Self::Down),
            _ => None,
        }
    }

    /// Returns what a forwarded request's failure says of the node, or `None`
    /// when the gateway sent nothing.
    #[must_use]
    pub const fn of_forward_error(error: &ForwardError) -> Option<Self> {
        match error {
            ForwardError::TimeOut { .. } | ForwardError::Unreachable { .. } => Some(Self::Down),
            ForwardError::Refused { .. } => Some(Self::Failing),
            _ => None,
        }
    }

    /// Returns what a forwarded request's outcome says of the node, or `None`
    /// when the gateway sent nothing.
    #[must_use]
    pub fn of_forwarded(outcome: &Result<Forwarded, ForwardError>) -> Option<Self> {
        match outcome {
            Ok(answer) => Some(Self::of_answer(answer.status())),
            Err(error) => Self::of_forward_error(error),
        }
    }

    /// Returns what a drift check's read of a member's copy says of the
    /// member: a copy held or missing is an answer, and a read that got none
    /// is read from its §11.1 status.
    #[must_use]
    pub fn of_copy(copy: &NodeCopy) -> Option<Self> {
        match copy {
            NodeCopy::Held { .. } | NodeCopy::Missing { .. } => Some(Self::Up),
            NodeCopy::Failed { outcome } => Self::of_status(outcome.status()),
        }
    }

    /// Returns what a node's HTTP answer says of it: a server error is a
    /// failure, and any other answer is an answer.
    #[must_use]
    pub fn of_answer(status: http::StatusCode) -> Self {
        if status.is_server_error() {
            Self::Failing
        } else {
            Self::Up
        }
    }

    /// Returns what a resolution says of the resolver: down when it could not
    /// answer for some member, up when it answered for each, and `None` when
    /// it was not asked.
    #[must_use]
    pub fn of_resolutions(resolutions: &BTreeMap<NodeId, Resolution>) -> Option<Self> {
        if resolutions.is_empty() {
            None
        } else if resolutions
            .values()
            .any(|resolution| matches!(resolution, Resolution::Unavailable(_)))
        {
            Some(Self::Down)
        } else {
            Some(Self::Up)
        }
    }

    /// Returns what an ask-all probe answer says of the member, or `None`
    /// when the gateway sent nothing.
    #[must_use]
    pub fn of_probe(answer: &Answer) -> Option<Self> {
        match answer {
            Answer::Holds(forwarded) => Some(Self::of_answer(forwarded.status())),
            Answer::Absent => Some(Self::Up),
            Answer::Erred(status) => Some(Self::of_answer(*status)),
            Answer::Failed(error) => Self::of_forward_error(error),
            Answer::Abandoned => Some(Self::Down),
        }
    }
}

/// One slot per member endpoint and one for the resolver, each holding the
/// last state observed.
#[derive(Debug)]
pub struct Dependencies {
    /// The endpoints of the registry snapshot, fixed at construction.
    endpoints: BTreeMap<EndpointId, AtomicU8>,
    /// The resolver's slot, when one is configured.
    resolver: Option<AtomicU8>,
}

impl Dependencies {
    /// Returns a record of `endpoints` and, when `resolver` is `true`, of the
    /// resolver, every state [`Observed::Unknown`].
    #[must_use]
    pub fn new<'a>(endpoints: impl IntoIterator<Item = &'a EndpointId>, resolver: bool) -> Self {
        Self {
            endpoints: endpoints
                .into_iter()
                .map(|endpoint| (endpoint.clone(), AtomicU8::new(Observed::Unknown.code())))
                .collect(),
            resolver: resolver.then(|| AtomicU8::new(Observed::Unknown.code())),
        }
    }

    /// Records `observed` as the last state of `endpoint`.
    ///
    /// An endpoint outside the record is ignored, so the record never grows.
    pub fn endpoint(&self, endpoint: &EndpointId, observed: Observed) {
        if let Some(slot) = self.endpoints.get(endpoint) {
            slot.store(observed.code(), Ordering::Relaxed);
        }
    }

    /// Records what a request forwarded to `endpoint` showed of it, when the
    /// gateway sent one.
    pub fn forwarded(&self, endpoint: &EndpointId, outcome: &Result<Forwarded, ForwardError>) {
        if let Some(observed) = Observed::of_forwarded(outcome) {
            self.endpoint(endpoint, observed);
        }
    }

    /// Records what an ask-all probe answer showed of `endpoint`, when the
    /// gateway sent the probe.
    pub fn probed(&self, endpoint: &EndpointId, answer: &Answer) {
        if let Some(observed) = Observed::of_probe(answer) {
            self.endpoint(endpoint, observed);
        }
    }

    /// Records the state of every endpoint a fan-out sent a request to, from
    /// the §11.1 status `meta.federation` reports for it.
    pub fn fan_out(&self, federation: &FederationMeta) {
        for outcome in federation.endpoints() {
            let Some(observed) = Observed::of_status(outcome.status()) else {
                continue;
            };
            // NOTE: no specification governs this: our own design; an id the
            // registry would refuse names no slot, so there is nothing to record.
            if let Ok(endpoint) = EndpointId::new(outcome.id().as_str()) {
                self.endpoint(&endpoint, observed);
            }
        }
    }

    /// Records `observed` as the resolver's last state, when one is
    /// configured.
    pub fn resolver(&self, observed: Observed) {
        if let Some(slot) = &self.resolver {
            slot.store(observed.code(), Ordering::Relaxed);
        }
    }

    /// Returns the report `GET /health/dependencies` answers with.
    #[must_use]
    pub fn report(&self) -> Report {
        Report {
            endpoints: self
                .endpoints
                .iter()
                .map(|(endpoint, slot)| {
                    (
                        endpoint.as_str().to_owned(),
                        Observed::from_code(slot.load(Ordering::Relaxed)),
                    )
                })
                .collect(),
            resolver: self
                .resolver
                .as_ref()
                .map(|slot| Observed::from_code(slot.load(Ordering::Relaxed))),
        }
    }
}

/// What `GET /health/dependencies` answers with: endpoint ids and states,
/// and nothing else.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    /// Every member endpoint by id, in id order.
    pub endpoints: BTreeMap<String, Observed>,
    /// The resolver's state, absent when no resolver is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver: Option<Observed>,
}

#[cfg(test)]
mod tests {
    use super::{Dependencies, Observed};
    use ferrofed_engine::dispatch::definition::NodeCopy;
    use ferrofed_registry::id::EndpointId;
    use http::StatusCode;
    use openehr_federation::outcome::{ErrorDetail, Outcome};
    use openehr_federation::status::EndpointStatus;

    #[test]
    fn every_slot_starts_unknown_and_keeps_the_last_state() {
        let a = EndpointId::new("node-a-pub").expect("a valid id");
        let b = EndpointId::new("node-b-pub").expect("a valid id");
        let record = Dependencies::new([&a, &b], true);
        let report = record.report();
        assert_eq!(Some(&Observed::Unknown), report.endpoints.get("node-a-pub"));
        assert_eq!(Some(Observed::Unknown), report.resolver);
        record.endpoint(&a, Observed::Down);
        record.endpoint(&a, Observed::Up);
        record.endpoint(&b, Observed::Failing);
        record.resolver(Observed::Down);
        let report = record.report();
        assert_eq!(Some(&Observed::Up), report.endpoints.get("node-a-pub"));
        assert_eq!(Some(&Observed::Failing), report.endpoints.get("node-b-pub"));
        assert_eq!(Some(Observed::Down), report.resolver);
    }

    #[test]
    fn an_endpoint_outside_the_record_is_never_added() {
        let a = EndpointId::new("node-a-pub").expect("a valid id");
        let stranger = EndpointId::new("node-z-pub").expect("a valid id");
        let record = Dependencies::new([&a], false);
        record.endpoint(&stranger, Observed::Down);
        record.resolver(Observed::Up);
        let report = record.report();
        assert_eq!(1, report.endpoints.len());
        assert_eq!(None, report.resolver, "no resolver, no slot");
    }

    #[test]
    fn only_the_statuses_a_request_produced_are_observations() {
        assert_eq!(
            Some(Observed::Up),
            Observed::of_status(EndpointStatus::Active)
        );
        assert_eq!(
            Some(Observed::Down),
            Observed::of_status(EndpointStatus::Offline)
        );
        assert_eq!(
            Some(Observed::Down),
            Observed::of_status(EndpointStatus::TimeOut)
        );
        assert_eq!(
            Some(Observed::Failing),
            Observed::of_status(EndpointStatus::NodeError)
        );
        for settled in [
            EndpointStatus::NotResolved,
            EndpointStatus::ConsentDenied,
            EndpointStatus::Excluded,
            EndpointStatus::NotLocalized,
        ] {
            assert_eq!(None, Observed::of_status(settled), "{settled:?}");
        }
        assert_eq!(Observed::Up, Observed::of_answer(StatusCode::NOT_FOUND));
        assert_eq!(
            Observed::Failing,
            Observed::of_answer(StatusCode::BAD_GATEWAY)
        );
    }

    #[test]
    fn a_copy_held_or_missing_is_an_answer_whether_or_not_it_matches() {
        let held = NodeCopy::Held {
            aql: "SELECT c FROM EHR e CONTAINS COMPOSITION c".to_owned(),
            latency_ms: 3,
        };
        assert_eq!(Some(Observed::Up), Observed::of_copy(&held));
        let missing = NodeCopy::Missing { latency_ms: 3 };
        assert_eq!(Some(Observed::Up), Observed::of_copy(&missing));
        let error = || ErrorDetail::Text("synthetic".to_owned());
        for (outcome, observed) in [
            (
                Outcome::NodeError {
                    latency_ms: 3,
                    error: error(),
                },
                Observed::Failing,
            ),
            (
                Outcome::TimeOut {
                    latency_ms: 3,
                    error: error(),
                },
                Observed::Down,
            ),
            (
                Outcome::Offline {
                    latency_ms: 3,
                    error: error(),
                },
                Observed::Down,
            ),
        ] {
            let failed = NodeCopy::Failed { outcome };
            assert_eq!(Some(observed), Observed::of_copy(&failed), "{failed:?}");
        }
    }
}
