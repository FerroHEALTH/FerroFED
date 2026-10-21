// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The usage report's metrics window.
//!
//! It holds a latency histogram and the server errors per request group and
//! the client refusals, recorded by the request log, and the outcome of each
//! request the gateway sent to a member node, recorded where the node
//! request metrics are.
//!
//! A recording carries the request's group, read from the route TEMPLATE it
//! matched (never the path a client sent), its status and its latency, or a
//! node request's §11.1 status alone; nothing of a request's parameters,
//! body, caller or node reaches it, so nothing of them can reach a report
//! (§5.4.1, N33). No specification governs this: our own design.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use http::StatusCode;
use openehr_federation::status::EndpointStatus;

use crate::base_path::BasePath;
use crate::request_log::UNMATCHED;
use crate::usage_report::payload::{
    BUCKET_BOUNDS_MS, BUCKETS, Counters, Group, GroupStats, MAX_COUNT, MAX_MS, Metrics,
    RequestsBucket, Window24h,
};

impl Group {
    /// Returns the group of a request from the route template it matched,
    /// under the deployment's `base` and the FHIR face's `fhir` base, or
    /// `None` for a probe the report does not count.
    ///
    /// The health routes and the JWK Set are probes. A template's parameter
    /// segments are matched by position and never read; [`UNMATCHED`] is
    /// `other`.
    #[must_use]
    pub fn of(base: &BasePath, fhir: Option<&str>, template: &str) -> Option<Self> {
        if template == UNMATCHED {
            return Some(Self::Other);
        }
        if let Some(fhir) = fhir
            && (template == fhir
                || template.starts_with(&format!("{}/", fhir.trim_end_matches('/'))))
        {
            return Some(Self::Fhir);
        }
        let relative = if base.is_root() {
            template
        } else {
            template.strip_prefix(base.as_str()).unwrap_or(template)
        };
        let segments: Vec<&str> = relative.split('/').filter(|s| !s.is_empty()).collect();
        let group = match segments.as_slice() {
            ["health" | ".well-known", ..] => return None,
            ["v1", "query", ..] => Self::Query,
            ["v1", "ehr", _, "composition", ..] => Self::Composition,
            ["v1", "ehr", ..] => Self::Ehr,
            ["v1", "definition", ..] => Self::Definition,
            ["operator", ..] => Self::Operator,
            _ => Self::Other,
        };
        Some(group)
    }
}

/// One group's counts over a window.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GroupCounts {
    /// The count per latency bucket.
    pub buckets: [u64; BUCKETS],
    /// Responses with a 5xx status.
    pub errors_5xx: u64,
    /// The largest latency recorded, in milliseconds.
    pub max_ms: f64,
}

impl GroupCounts {
    /// Returns the number of recordings.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.buckets
            .iter()
            .fold(0_u64, |sum, count| sum.saturating_add(*count))
    }

    /// Returns the `q` quantile in milliseconds, interpolated linearly inside
    /// its bucket; the `+Inf` bucket reaches up to the largest recording.
    #[must_use]
    pub fn quantile_ms(&self, q: f64) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let rank = q.clamp(0.0, 1.0) * to_f64(total);
        let mut cumulative = 0_u64;
        for (index, count) in self.buckets.iter().copied().enumerate() {
            if count == 0 {
                continue;
            }
            let before = cumulative;
            cumulative = cumulative.saturating_add(count);
            if to_f64(cumulative) >= rank {
                let lower = lower_bound_ms(index);
                let upper = upper_bound_ms(index, self.max_ms).max(lower);
                let fraction = ((rank - to_f64(before)) / to_f64(count)).clamp(0.0, 1.0);
                return report_ms((upper - lower).mul_add(fraction, lower));
            }
        }
        report_ms(self.max_ms)
    }

    /// Returns the group as the report carries it, or `None` without
    /// requests.
    #[must_use]
    pub fn stats(&self) -> Option<GroupStats> {
        (self.total() > 0).then(|| GroupStats {
            p50_ms: self.quantile_ms(0.50),
            p95_ms: self.quantile_ms(0.95),
            p99_ms: self.quantile_ms(0.99),
            errors_5xx: self.errors_5xx.min(MAX_COUNT),
            histogram: self.buckets.map(|count| count.min(MAX_COUNT)),
        })
    }

    /// Counts one recording that took `elapsed`.
    fn time(&mut self, elapsed: Duration) {
        let ms = elapsed.as_secs_f64() * 1000.0;
        if let Some(slot) = self.buckets.get_mut(bucket_of(ms)) {
            *slot = slot.saturating_add(1);
        }
        self.max_ms = self.max_ms.max(ms);
    }

    /// Adds `other` to these counts.
    fn absorb(&mut self, other: &Self) {
        for (mine, theirs) in self.buckets.iter_mut().zip(other.buckets) {
            *mine = mine.saturating_add(theirs);
        }
        self.errors_5xx = self.errors_5xx.saturating_add(other.errors_5xx);
        self.max_ms = self.max_ms.max(other.max_ms);
    }
}

/// A window's counts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Counts {
    /// The counts of each group with a recording.
    pub groups: BTreeMap<Group, GroupCounts>,
    /// Responses with a 4xx status, over every counted group.
    pub responses_4xx: u64,
    /// Requests the gateway sent to a member node.
    pub node_requests: u64,
    /// Node requests reported `time-out`.
    pub node_timeouts: u64,
    /// Node requests reported `node-error` or `offline`.
    pub node_errors: u64,
}

impl Counts {
    /// Returns the number of counted requests.
    #[must_use]
    pub fn requests(&self) -> u64 {
        self.groups
            .values()
            .fold(0_u64, |sum, group| sum.saturating_add(group.total()))
    }

    /// Renders the report's `metrics` object.
    #[must_use]
    pub fn to_metrics(&self) -> Metrics {
        Metrics {
            window_24h: Window24h {
                requests_bucket: RequestsBucket::of(self.requests()),
                groups: self
                    .groups
                    .iter()
                    .filter_map(|(group, counts)| counts.stats().map(|stats| (*group, stats)))
                    .collect(),
                counters: Counters {
                    node_requests: self.node_requests.min(MAX_COUNT),
                    node_timeouts: self.node_timeouts.min(MAX_COUNT),
                    node_errors: self.node_errors.min(MAX_COUNT),
                    responses_4xx: self.responses_4xx.min(MAX_COUNT),
                },
            },
        }
    }

    /// Adds `other` to these counts.
    fn absorb(&mut self, other: &Self) {
        for (group, counts) in &other.groups {
            self.groups.entry(*group).or_default().absorb(counts);
        }
        self.responses_4xx = self.responses_4xx.saturating_add(other.responses_4xx);
        self.node_requests = self.node_requests.saturating_add(other.node_requests);
        self.node_timeouts = self.node_timeouts.saturating_add(other.node_timeouts);
        self.node_errors = self.node_errors.saturating_add(other.node_errors);
    }
}

/// The in-process window the request log and the node requests record into
/// and the daily report takes.
#[derive(Debug, Default)]
pub struct Window {
    counts: Mutex<Counts>,
}

impl Window {
    /// Records one request of `group` that ended in `status` after
    /// `elapsed`; a request with no group, a probe, is dropped.
    pub fn record(&self, group: Option<Group>, status: StatusCode, elapsed: Duration) {
        let Some(group) = group else {
            return;
        };
        let mut counts = self.lock();
        if status.is_client_error() {
            counts.responses_4xx = counts.responses_4xx.saturating_add(1);
        }
        let entry = counts.groups.entry(group).or_default();
        entry.time(elapsed);
        if status.is_server_error() {
            entry.errors_5xx = entry.errors_5xx.saturating_add(1);
        }
    }

    /// Records one request the gateway sent to a member node, which ended as
    /// `status` (§11.1).
    pub fn node(&self, status: EndpointStatus) {
        let mut counts = self.lock();
        counts.node_requests = counts.node_requests.saturating_add(1);
        match status {
            EndpointStatus::TimeOut => {
                counts.node_timeouts = counts.node_timeouts.saturating_add(1);
            }
            EndpointStatus::NodeError | EndpointStatus::Offline => {
                counts.node_errors = counts.node_errors.saturating_add(1);
            }
            _ => {}
        }
    }

    /// Takes the counts recorded so far and starts a fresh window.
    #[must_use]
    pub fn take(&self) -> Counts {
        std::mem::take(&mut *self.lock())
    }

    /// Puts `counts` back into the window, for a daily report that did not
    /// reach the collector.
    pub fn restore(&self, counts: &Counts) {
        self.lock().absorb(counts);
    }

    /// Returns the guard of the counts.
    fn lock(&self) -> std::sync::MutexGuard<'_, Counts> {
        // NOTE: no specification governs this: our own design; every update
        // leaves the counts consistent, so a poisoned lock still holds them.
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Returns the index of the bucket holding `ms`; an upper bound includes its
/// own value.
fn bucket_of(ms: f64) -> usize {
    BUCKET_BOUNDS_MS
        .iter()
        .position(|bound| ms <= f64::from(*bound))
        .unwrap_or(BUCKETS - 1)
}

/// Returns the lower bound of bucket `index`, in milliseconds.
fn lower_bound_ms(index: usize) -> f64 {
    index
        .checked_sub(1)
        .and_then(|previous| BUCKET_BOUNDS_MS.get(previous))
        .map_or(0.0, |bound| f64::from(*bound))
}

/// Returns the upper bound of bucket `index`, in milliseconds; the `+Inf`
/// bucket reaches up to `max_ms`.
fn upper_bound_ms(index: usize, max_ms: f64) -> f64 {
    BUCKET_BOUNDS_MS
        .get(index)
        .map_or(max_ms, |bound| f64::from(*bound))
}

/// Returns `ms` within the contract's bounds, rounded to a microsecond.
fn report_ms(ms: f64) -> f64 {
    (ms.clamp(0.0, MAX_MS) * 1000.0).round() / 1000.0
}

/// Returns `count` as a float, saturating at `u32::MAX`, which a report's
/// counts (at most [`MAX_COUNT`]) never reach.
fn to_f64(count: u64) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use http::StatusCode;
    use openehr_federation::status::EndpointStatus;

    use super::{Group, GroupCounts, Window, bucket_of};
    use crate::base_path::BasePath;
    use crate::request_log::UNMATCHED;

    fn group(base: &str, template: &str) -> Option<Group> {
        let base: BasePath = base.parse().expect("a base path");
        Group::of(&base, Some("/fhir"), template)
    }

    #[test]
    fn every_route_template_has_its_group() {
        for base in ["/", "/fed"] {
            let prefix = if base == "/" { "" } else { base };
            let cases = [
                ("/v1/query/aql", Group::Query),
                ("/v1/query/{qualified_query_name}", Group::Query),
                ("/v1/query/{qualified_query_name}/{version}", Group::Query),
                ("/v1/ehr", Group::Ehr),
                ("/v1/ehr/{ehr_id}", Group::Ehr),
                ("/v1/ehr/{ehr_id}/ehr_status", Group::Ehr),
                ("/v1/ehr/{ehr_id}/composition", Group::Composition),
                (
                    "/v1/ehr/{ehr_id}/composition/{uid_based_id}",
                    Group::Composition,
                ),
                ("/v1/definition/template/adl1.4", Group::Definition),
                ("/operator/incidents", Group::Operator),
                ("/", Group::Other),
            ];
            for (template, expected) in cases {
                let template = format!("{prefix}{template}");
                assert_eq!(group(base, &template), Some(expected), "{template}");
            }
        }
        assert_eq!(group("/", "/fhir/Patient/$summary"), Some(Group::Fhir));
        assert_eq!(group("/", "/fhir"), Some(Group::Fhir));
        assert_eq!(group("/", "/fhirx"), Some(Group::Other));
        assert_eq!(group("/", UNMATCHED), Some(Group::Other));
    }

    #[test]
    fn the_probes_are_not_counted() {
        for template in ["/health", "/health/readiness", "/.well-known/jwks.json"] {
            assert_eq!(group("/", template), None, "{template}");
        }
        assert_eq!(group("/fed", "/fed/health/readiness"), None);
    }

    #[test]
    fn an_upper_bound_includes_its_own_value() {
        assert_eq!(bucket_of(0.0), 0);
        assert_eq!(bucket_of(5.0), 0);
        assert_eq!(bucket_of(5.001), 1);
        assert_eq!(bucket_of(5000.0), 9);
        assert_eq!(bucket_of(5000.5), 10);
    }

    #[test]
    fn a_quantile_interpolates_inside_its_bucket() {
        let counts = GroupCounts {
            buckets: [0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            errors_5xx: 0,
            max_ms: 9.0,
        };
        assert!((counts.quantile_ms(0.5) - 7.5).abs() < 1e-9);
        let tail = GroupCounts {
            buckets: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            errors_5xx: 0,
            max_ms: 8000.0,
        };
        assert!((tail.quantile_ms(0.99) - 7970.0).abs() < 1e-6);
        assert!(GroupCounts::default().stats().is_none());
    }

    #[test]
    fn a_restored_window_carries_its_counts_into_the_next() {
        let window = Window::default();
        let query = Some(Group::Query);
        window.record(query, StatusCode::OK, Duration::from_millis(3));
        window.record(query, StatusCode::BAD_GATEWAY, Duration::from_millis(30));
        window.record(Some(Group::Other), StatusCode::NOT_FOUND, Duration::ZERO);
        window.record(None, StatusCode::OK, Duration::ZERO);
        window.node(EndpointStatus::Active);
        window.node(EndpointStatus::TimeOut);
        window.node(EndpointStatus::NodeError);
        window.node(EndpointStatus::Offline);
        let taken = window.take();
        assert_eq!(taken.requests(), 3);
        assert_eq!(taken.responses_4xx, 1);
        assert_eq!(
            (taken.node_requests, taken.node_timeouts, taken.node_errors),
            (4, 1, 2)
        );
        assert_eq!(window.take().requests(), 0, "taking starts a fresh window");
        window.restore(&taken);
        window.record(query, StatusCode::OK, Duration::from_millis(3));
        window.node(EndpointStatus::Active);
        let next = window.take();
        let counted = next.groups.get(&Group::Query).expect("recorded");
        assert_eq!(counted.total(), 3);
        assert_eq!(counted.errors_5xx, 1);
        assert_eq!(next.responses_4xx, 1);
        let counters = next.to_metrics().window_24h.counters;
        assert_eq!(
            (
                counters.node_requests,
                counters.node_timeouts,
                counters.node_errors
            ),
            (5, 1, 2)
        );
    }
}
