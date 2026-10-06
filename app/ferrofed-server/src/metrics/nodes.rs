// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node request counter and duration histogram, recorded from what the
//! gateway already observes of each request it sends to a member.
//!
//! A request's `outcome` is the §11.1 status the per-endpoint report gives
//! it: `active`, `offline`, `time-out`, `node-error` or `consent-denied`, the
//! statuses that carry a `latency_ms` because a request was sent (§9.5). A
//! request routed to one node and an ask-all probe have no per-endpoint
//! report, so their outcome is read the same way from the node's answer: a
//! server error or a refusal of the gateway's onward credentials is
//! `node-error`, and a consent refusal in a code the registry lists is
//! `consent-denied`, also where the client's answer withholds it. Every request counted is timed, and a request that never
//! left the gateway is neither counted nor timed, whatever its §11.1 record
//! says of it. The `endpoint` label is an endpoint id of the registry snapshot the
//! request ran on, never anything a request carries.

use std::collections::BTreeSet;
use std::time::Duration;

use ferrofed_engine::dispatch::Contact;
use ferrofed_engine::single_node::forward::{ForwardError, Forwarded};
use ferrofed_engine::single_node::probe::{Answer, Probed};
use ferrofed_identity::role::consent::ConsentDecision;
use ferrofed_identity::role::demographics::{DemographicsError, Identification};
use ferrofed_identity::role::header::HeaderAnswer;
use ferrofed_identity::role::localizer::{Localization, LocalizerError};
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::outcome::Outcome;
use openehr_federation::status::EndpointStatus;
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram, Meter};

use crate::metrics::{
    CONSENT_PREFILTER_REQUESTS, DEMOGRAPHICS_REQUEST_DURATION, DEMOGRAPHICS_REQUESTS,
    LOCALIZER_REQUEST_DURATION, LOCALIZER_REQUESTS, NODE_DURATION_BUCKETS, NODE_REQUEST_DURATION,
    NODE_REQUESTS, OVERLOAD_REFUSALS, RESOLVER_REQUEST_DURATION, RESOLVER_REQUESTS,
};

/// The node request instruments of one meter provider, shared by every
/// federation a reload builds.
#[derive(Debug, Clone)]
pub struct Instruments {
    requests: Counter<u64>,
    duration: Histogram<f64>,
    prefilter: Counter<u64>,
    localizer: Counter<u64>,
    localizer_duration: Histogram<f64>,
    demographics: Counter<u64>,
    demographics_duration: Histogram<f64>,
    resolver: Counter<u64>,
    resolver_duration: Histogram<f64>,
    overload: Counter<u64>,
}

impl Instruments {
    /// Creates the instruments on `meter`.
    #[must_use]
    pub fn new(meter: &Meter) -> Self {
        Self {
            requests: meter
                .u64_counter(NODE_REQUESTS)
                .with_description("Requests sent to a member node, by endpoint and outcome")
                .build(),
            duration: meter
                .f64_histogram(NODE_REQUEST_DURATION)
                .with_description("The time a member node took to answer a request")
                .with_unit("s")
                .with_boundaries(NODE_DURATION_BUCKETS.to_vec())
                .build(),
            prefilter: meter
                .u64_counter(CONSENT_PREFILTER_REQUESTS)
                .with_description("Calls to the consent pre-filter, by outcome")
                .build(),
            localizer: meter
                .u64_counter(LOCALIZER_REQUESTS)
                .with_description("Calls to the localizer, by outcome")
                .build(),
            demographics: meter
                .u64_counter(DEMOGRAPHICS_REQUESTS)
                .with_description("Calls to the demographics service, by outcome")
                .build(),
            demographics_duration: duration(
                meter,
                DEMOGRAPHICS_REQUEST_DURATION,
                "The time the demographics service took to answer a call",
            ),
            localizer_duration: duration(
                meter,
                LOCALIZER_REQUEST_DURATION,
                "The time the localizer took to answer a call",
            ),
            resolver: meter
                .u64_counter(RESOLVER_REQUESTS)
                .with_description("Calls to the cross-reference resolver, by outcome")
                .build(),
            resolver_duration: duration(
                meter,
                RESOLVER_REQUEST_DURATION,
                "The time the cross-reference resolver took to answer a call",
            ),
            overload: overload(meter),
        }
    }

    /// Counts one call to the cross-reference resolver that ended as
    /// `outcome` after `elapsed`.
    pub fn resolved(&self, outcome: crate::metrics::resolver::Outcome, elapsed: Duration) {
        self.resolver
            .add(1, &[KeyValue::new("outcome", outcome.as_str())]);
        self.resolver_duration.record(elapsed.as_secs_f64(), &[]);
    }

    /// Counts one request a limit of the gateway refused.
    pub fn shed(&self, limit: Limit) {
        self.overload
            .add(1, &[KeyValue::new("limit", limit.as_str())]);
    }
}

/// A limit of the gateway that refuses a request past it, the `limit` label
/// of [`OVERLOAD_REFUSALS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// The gateway serves as many requests as `server.max_concurrent_requests`
    /// allows: `503`.
    Concurrency,
    /// The caller sent more than `[server.caller_rate]` allows: `429`.
    CallerRate,
    /// The endpoint's `federation.max_in_flight_per_node` cap stayed full
    /// until the request's deadline: `time-out` (§11.5).
    NodeInFlight,
}

impl Limit {
    /// Every limit, in declaration order.
    pub const ALL: [Self; 3] = [Self::Concurrency, Self::CallerRate, Self::NodeInFlight];

    /// The label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Concurrency => "concurrency",
            Self::CallerRate => "caller-rate",
            Self::NodeInFlight => "node-in-flight",
        }
    }
}

/// A duration histogram named `name`, in seconds, over the node request
/// buckets.
fn duration(meter: &Meter, name: &'static str, description: &'static str) -> Histogram<f64> {
    meter
        .f64_histogram(name)
        .with_description(description)
        .with_unit("s")
        .with_boundaries(NODE_DURATION_BUCKETS.to_vec())
        .build()
}

/// The refusal counter of every limit, the two the listener applies at `0`
/// from the start, so an alert on their increase works from the first
/// scrape; a per-node refusal also names its `endpoint`.
fn overload(meter: &Meter) -> Counter<u64> {
    let counter = meter
        .u64_counter(OVERLOAD_REFUSALS)
        .with_description("Requests a limit of the gateway refused, by limit")
        .build();
    for limit in [Limit::Concurrency, Limit::CallerRate] {
        counter.add(0, &[KeyValue::new("limit", limit.as_str())]);
    }
    counter
}

/// What one federation records of its node requests: the endpoints of its
/// registry snapshot, and the instruments, when the server meters it.
#[derive(Debug)]
pub struct NodeRequests {
    endpoints: BTreeSet<EndpointId>,
    instruments: Option<Instruments>,
}

impl NodeRequests {
    /// Returns a record of `endpoints` that records nothing until
    /// [`NodeRequests::metered`] gives it instruments.
    #[must_use]
    pub fn new<'a>(endpoints: impl IntoIterator<Item = &'a EndpointId>) -> Self {
        Self {
            endpoints: endpoints.into_iter().cloned().collect(),
            instruments: None,
        }
    }

    /// Records through `instruments` from now on.
    pub fn metered(&mut self, instruments: Instruments) {
        self.instruments = Some(instruments);
    }

    /// Returns the instruments this record writes to, when it is metered.
    #[must_use]
    pub fn instruments(&self) -> Option<&Instruments> {
        self.instruments.as_ref()
    }

    /// Records the request to `endpoint` that ended as `outcome` in a
    /// per-member record, when `contact` says it left the gateway.
    ///
    /// A member record reports a request the gateway could not send as
    /// `offline` with a `latency_ms`, so the record alone cannot tell it
    /// from a node that was not reached; `contact` can, and such a request
    /// is not counted.
    pub fn settled(&self, endpoint: &EndpointId, outcome: &Outcome, contact: Contact) {
        if contact == Contact::Capped {
            self.capped(endpoint);
        }
        if !contact.sent() {
            return;
        }
        if let Some(latency_ms) = outcome.latency_ms() {
            self.count(
                endpoint,
                outcome.status(),
                Duration::from_millis(latency_ms),
            );
        }
    }

    /// Records a request forwarded to `endpoint` that took `elapsed`, when
    /// the gateway sent it; an answer the node `refused` on consent grounds
    /// is `consent-denied`, whether or not the client is told (N27).
    pub fn forwarded(
        &self,
        endpoint: &EndpointId,
        (outcome, refused): (&Result<Forwarded, ForwardError>, bool),
        elapsed: Duration,
    ) {
        let contact = Contact::of_forwarded(outcome);
        if contact == Contact::Capped {
            self.capped(endpoint);
        }
        if !contact.sent() {
            return;
        }
        let status = match outcome {
            Ok(_) if refused => Some(EndpointStatus::ConsentDenied),
            Ok(answer) => Some(answered(answer.status())),
            Err(error) => failed(error),
        };
        if let Some(status) = status {
            self.count(endpoint, status, elapsed);
        }
    }

    /// Records an ask-all probe of `endpoint` and its latency, when the
    /// gateway sent it.
    ///
    /// An abandoned probe is a `time-out`, timed to the moment the overall
    /// budget ran out, as an abandoned fan-out request is (§11.1, §11.5).
    pub fn probed(&self, endpoint: &EndpointId, probed: &Probed) {
        if probed.contact() == Contact::Capped {
            self.capped(endpoint);
        }
        if !probed.contact().sent() {
            return;
        }
        let status = match &probed.answer {
            Answer::Holds(forwarded) => Some(answered(forwarded.status())),
            Answer::Absent => Some(EndpointStatus::Active),
            Answer::Erred(status) => Some(answered(*status)),
            Answer::ConsentRefused => Some(EndpointStatus::ConsentDenied),
            Answer::Failed(error) => failed(error),
            Answer::Abandoned => Some(EndpointStatus::TimeOut),
        };
        if let Some(status) = status {
            self.count(endpoint, status, probed.latency);
        }
    }

    /// Counts one call to the consent pre-filter that ended in `decision`, in
    /// a series of its own: the pre-filter is no member, so it has no
    /// `endpoint` and no §11.1 outcome. A call that did not ask the service
    /// is `not-asked`, labelled with the closed reason.
    pub fn prefiltered(&self, decision: &ConsentDecision) {
        let Some(instruments) = &self.instruments else {
            return;
        };
        let (outcome, reason) = match decision {
            ConsentDecision::Denied(_) => ("denied", None),
            ConsentDecision::NoSignal => ("no-signal", None),
            ConsentDecision::NotAsked(reason) => ("not-asked", Some(reason.as_str())),
            ConsentDecision::Unavailable(_) => ("unavailable", None),
            ConsentDecision::Partial { .. } => ("partial", None),
        };
        // NOTE: §5.4.1, N33: the reason is a closed enum name, never a value of the
        // request, so no identifier reaches the label.
        let mut labels = vec![KeyValue::new("outcome", outcome)];
        labels.extend(reason.map(|reason| KeyValue::new("reason", reason)));
        instruments.prefilter.add(1, &labels);
    }

    /// Counts one call to the localizer that ended in `localization`, in a
    /// series of its own: the localizer is no member, so it has no
    /// `endpoint` and no §11.1 outcome. A call that was made is timed
    /// over `elapsed`; one the localizer was never asked, `not-configured`,
    /// is not.
    pub fn localized(&self, localization: &Localization, elapsed: Duration) {
        let Some(instruments) = &self.instruments else {
            return;
        };
        let outcome = match localization {
            Localization::Candidates(_) => "candidates",
            Localization::NoRecords => "no-records",
            Localization::NotConfigured => "not-configured",
            Localization::Unavailable(LocalizerError::AuditFailed(_)) => "audit-failed",
            Localization::Unavailable(_) => "unavailable",
        };
        instruments
            .localizer
            .add(1, &[KeyValue::new("outcome", outcome)]);
        if !matches!(localization, Localization::NotConfigured) {
            instruments
                .localizer_duration
                .record(elapsed.as_secs_f64(), &[]);
        }
    }

    /// Counts one call to the demographics service that ended in
    /// `identification`, in a series of its own: the service is no member,
    /// so it has no `endpoint` and no §11.1 outcome, timed over `elapsed`.
    pub fn identified(&self, identification: &Identification, elapsed: Duration) {
        let outcome = match identification {
            Identification::Identified(_) => "identified",
            Identification::NoMatch => "no-match",
            Identification::Ambiguous(_) => "ambiguous",
            Identification::Unavailable(DemographicsError::AuditFailed(_)) => "audit-failed",
            _ => "unavailable",
        };
        self.demographics_call(outcome, elapsed);
    }

    /// Counts one call to the demographics service for a summary header
    /// that ended in `answer`, in the series of [`Self::identified`], the
    /// patient found counted as `identified`, timed over `elapsed`.
    pub fn headed(&self, answer: &HeaderAnswer, elapsed: Duration) {
        let outcome = match answer {
            HeaderAnswer::Found(_) => "identified",
            HeaderAnswer::NoMatch => "no-match",
            HeaderAnswer::Ambiguous(_) => "ambiguous",
            HeaderAnswer::Unavailable(DemographicsError::AuditFailed(_)) => "audit-failed",
            _ => "unavailable",
        };
        self.demographics_call(outcome, elapsed);
    }

    /// Counts one call to the demographics service that ended in `outcome`,
    /// timed over `elapsed`.
    fn demographics_call(&self, outcome: &'static str, elapsed: Duration) {
        let Some(instruments) = &self.instruments else {
            return;
        };
        instruments
            .demographics
            .add(1, &[KeyValue::new("outcome", outcome)]);
        instruments
            .demographics_duration
            .record(elapsed.as_secs_f64(), &[]);
    }

    /// Counts one request to `endpoint` the endpoint's in-flight cap refused;
    /// an endpoint outside the snapshot is not recorded.
    fn capped(&self, endpoint: &EndpointId) {
        let Some(instruments) = &self.instruments else {
            return;
        };
        let Some(endpoint) = self.endpoints.get(endpoint) else {
            return;
        };
        instruments.overload.add(
            1,
            &[
                KeyValue::new("limit", Limit::NodeInFlight.as_str()),
                KeyValue::new("endpoint", endpoint.as_str().to_owned()),
            ],
        );
    }

    /// Counts one request to `endpoint` that ended as `status` after
    /// `elapsed`; an endpoint outside the snapshot is not recorded.
    fn count(&self, endpoint: &EndpointId, status: EndpointStatus, elapsed: Duration) {
        let Some(instruments) = &self.instruments else {
            return;
        };
        // NOTE: §5.4.1, N33: a label value is a registry endpoint id or an enum
        // name, never request text, so no identifier reaches the surface.
        let Some(endpoint) = self.endpoints.get(endpoint) else {
            return;
        };
        let label = KeyValue::new("endpoint", endpoint.as_str().to_owned());
        instruments.requests.add(
            1,
            &[label.clone(), KeyValue::new("outcome", status.as_str())],
        );
        instruments.duration.record(elapsed.as_secs_f64(), &[label]);
    }
}

/// The outcome of a request the node answered with `status`: a server error
/// is `node-error`, and any other answer is an answer.
fn answered(status: StatusCode) -> EndpointStatus {
    if status.is_server_error() {
        EndpointStatus::NodeError
    } else {
        EndpointStatus::Active
    }
}

/// The outcome of a request that got no answer of the node's, or `None` when
/// the gateway sent nothing.
fn failed(error: &ForwardError) -> Option<EndpointStatus> {
    match error {
        ForwardError::TimeOut { .. } => Some(EndpointStatus::TimeOut),
        ForwardError::Unreachable { .. } => Some(EndpointStatus::Offline),
        ForwardError::Refused { .. } | ForwardError::Oversized { .. } => {
            Some(EndpointStatus::NodeError)
        }
        _ => None,
    }
}
