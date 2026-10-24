// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The recent latency of each member endpoint.
//!
//! The median and the 95th percentile over the last [`WINDOW`] requests the
//! gateway sent an endpoint are what `OPTIONS {base}/` reports as
//! `latency_ms_p50` and `latency_ms_p95` (§7a.2, N30).
//!
//! Every request [`NodeRequests`](crate::metrics::nodes::NodeRequests) times
//! is recorded here too, the ask-all probes among them, whether or not the
//! metrics surface is on, so the self-description never depends on
//! `[metrics]`. A sample is a duration and nothing else: no request, caller
//! or answer reaches it (§5.4.1, N33). The record lives as long as the
//! process, across registry reloads, and holds at most [`WINDOW`] samples per
//! endpoint id. No specification governs the window or the percentile
//! method: our own design.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use ferrofed_registry::id::EndpointId;

/// How many of an endpoint's most recent requests its percentiles are
/// computed over.
pub const WINDOW: usize = 100;

/// The median and the 95th percentile of an endpoint's recent requests, in
/// whole milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Percentiles {
    /// The median, `latency_ms_p50`.
    pub p50: u64,
    /// The 95th percentile, `latency_ms_p95`.
    pub p95: u64,
}

/// The last [`WINDOW`] latencies of each member endpoint.
#[derive(Debug, Default)]
pub struct Latencies {
    recent: Mutex<BTreeMap<EndpointId, VecDeque<u64>>>,
}

impl Latencies {
    /// Records one request to `endpoint` that took `elapsed`, dropping the
    /// oldest sample once the window is full.
    pub fn record(&self, endpoint: &EndpointId, elapsed: Duration) {
        let ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        let mut recent = self.recent.lock().unwrap_or_else(PoisonError::into_inner);
        let samples = recent
            .entry(endpoint.clone())
            .or_insert_with(|| VecDeque::with_capacity(WINDOW));
        if samples.len() >= WINDOW {
            samples.pop_front();
        }
        samples.push_back(ms);
    }

    /// Returns the percentiles of `endpoint`'s recent requests, or `None`
    /// when the gateway has sent it none since the process started.
    #[must_use]
    pub fn of(&self, endpoint: &EndpointId) -> Option<Percentiles> {
        let mut sorted: Vec<u64> = {
            let recent = self.recent.lock().unwrap_or_else(PoisonError::into_inner);
            recent.get(endpoint)?.iter().copied().collect()
        };
        sorted.sort_unstable();
        Some(Percentiles {
            p50: nearest_rank(&sorted, 50)?,
            p95: nearest_rank(&sorted, 95)?,
        })
    }
}

/// The `percent` percentile of `sorted` by the nearest-rank method: the
/// smallest sample at least `percent` per cent of the samples do not
/// exceed, or `None` for no samples.
// NOTE: no specification governs this: our own design; nearest rank reports a latency that was
// observed, never one interpolated between two requests.
fn nearest_rank(sorted: &[u64], percent: usize) -> Option<u64> {
    let rank = sorted.len().checked_mul(percent)?.div_ceil(100).max(1);
    sorted.get(rank.checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(id: &str) -> EndpointId {
        EndpointId::new(id).expect("a valid endpoint id")
    }

    #[test]
    fn an_endpoint_never_asked_has_no_percentiles() {
        let latencies = Latencies::default();
        latencies.record(&endpoint("node_b"), Duration::from_millis(40));
        assert_eq!(None, latencies.of(&endpoint("node_a")));
    }

    #[test]
    fn one_sample_is_both_percentiles() {
        let latencies = Latencies::default();
        latencies.record(&endpoint("node_a"), Duration::from_millis(42));
        assert_eq!(
            Some(Percentiles { p50: 42, p95: 42 }),
            latencies.of(&endpoint("node_a"))
        );
    }

    #[test]
    fn a_slow_tail_raises_the_95th_percentile_and_not_the_median() {
        let latencies = Latencies::default();
        let node = endpoint("node_a");
        for _ in 0..90 {
            latencies.record(&node, Duration::from_millis(20));
        }
        for _ in 0..10 {
            latencies.record(&node, Duration::from_millis(3000));
        }
        assert_eq!(
            Some(Percentiles { p50: 20, p95: 3000 }),
            latencies.of(&node)
        );
    }

    #[test]
    fn the_window_keeps_only_the_most_recent_requests() {
        let latencies = Latencies::default();
        let node = endpoint("node_a");
        for _ in 0..WINDOW {
            latencies.record(&node, Duration::from_millis(5000));
        }
        for _ in 0..WINDOW {
            latencies.record(&node, Duration::from_millis(30));
        }
        assert_eq!(Some(Percentiles { p50: 30, p95: 30 }), latencies.of(&node));
    }

    #[test]
    fn nearest_rank_picks_an_observed_sample() {
        let sorted: Vec<u64> = (1..=20).collect();
        assert_eq!(Some(10), nearest_rank(&sorted, 50));
        assert_eq!(Some(19), nearest_rank(&sorted, 95));
        assert_eq!(None, nearest_rank(&[], 50));
    }
}
