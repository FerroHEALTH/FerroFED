// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The store's readiness indicator: up while the database answers a
//! statement within the store's budget.
//!
//! Every readiness probe asks the database, but the log says only what
//! changed: one warning when the store goes down, a repeat at most once per
//! [`REPEAT`] while it stays down, so an operator sees the outage is still
//! there, and one line when it answers again, with how long it was down.
//! No specification governs the log: our own design.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ferrohealth_health::registry::{Check, HealthIndicator, Outcome};

use super::{INDICATOR, Store};

/// The least time between two warnings of a store that stays down.
pub const REPEAT: Duration = Duration::from_secs(300);

/// What the readiness check says of one probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// The store went down.
    Down,
    /// The store is still down, as it has been for `for_`.
    StillDown {
        /// How long it has been down.
        for_: Duration,
    },
    /// The store answers again, after `for_` down.
    Back {
        /// How long it was down.
        for_: Duration,
    },
}

/// When the store went down and when the check last said so.
#[derive(Debug, Clone, Copy)]
struct Outage {
    since: Instant,
    said: Instant,
}

/// The readiness check's memory of the store's answers: up until a probe
/// finds it down.
#[derive(Debug, Default)]
pub struct Watch(Mutex<Option<Outage>>);

impl Watch {
    /// What to say of a probe at `now` that found the store `up` or not:
    /// [`Said::Down`] on the change to down, [`Said::StillDown`] when it
    /// stays down [`REPEAT`] past the last thing said, [`Said::Back`] on the
    /// change to up, and nothing otherwise.
    pub fn observe(&self, up: bool, now: Instant) -> Option<Said> {
        let mut outage = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match (*outage, up) {
            (None, false) => {
                *outage = Some(Outage {
                    since: now,
                    said: now,
                });
                Some(Said::Down)
            }
            (Some(down), true) => {
                *outage = None;
                Some(Said::Back {
                    for_: now.saturating_duration_since(down.since),
                })
            }
            (Some(down), false) if now.saturating_duration_since(down.said) >= REPEAT => {
                *outage = Some(Outage {
                    since: down.since,
                    said: now,
                });
                Some(Said::StillDown {
                    for_: now.saturating_duration_since(down.since),
                })
            }
            (None, true) | (Some(_), false) => None,
        }
    }
}

/// The store's readiness indicator.
#[derive(Debug)]
pub struct Ready {
    store: Arc<Store>,
    watch: Watch,
}

impl Ready {
    /// The indicator of `store`, which has not yet been found down.
    #[must_use]
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            watch: Watch::default(),
        }
    }
}

impl HealthIndicator for Ready {
    fn name(&self) -> &'static str {
        INDICATOR
    }

    fn check(&self) -> Check<'_> {
        Box::pin(async move {
            let answered = self.store.ping().await;
            match self.watch.observe(answered.is_ok(), Instant::now()) {
                Some(Said::Down) => {
                    let cause = answered.as_ref().err().map(|error| crate::chain(error));
                    tracing::warn!(error = cause, "the store does not answer");
                }
                Some(Said::StillDown { for_ }) => {
                    let cause = answered.as_ref().err().map(|error| crate::chain(error));
                    tracing::warn!(
                        error = cause,
                        down_for_s = for_.as_secs(),
                        "the store still does not answer"
                    );
                }
                Some(Said::Back { for_ }) => {
                    tracing::info!(down_for_s = for_.as_secs(), "the store answers again");
                }
                None => {}
            }
            match answered {
                Ok(()) => Outcome::up(),
                Err(_unanswered) => Outcome::down("the store does not answer"),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{REPEAT, Said, Watch};

    /// The probe interval of the tests, well below [`REPEAT`].
    const PROBE: Duration = Duration::from_secs(10);

    /// What `watch` says of failed probes from `start`, one every [`PROBE`]
    /// for `span`, and the instant of the first probe after them.
    fn down_for(watch: &Watch, start: Instant, span: Duration) -> (Vec<Said>, Instant) {
        let mut said = Vec::new();
        let mut now = start;
        while now.saturating_duration_since(start) < span {
            said.extend(watch.observe(false, now));
            now += PROBE;
        }
        (said, now)
    }

    #[test]
    fn a_store_that_answers_says_nothing() {
        let watch = Watch::default();
        let start = Instant::now();
        for probe in 0..100_u32 {
            assert_eq!(None, watch.observe(true, start + PROBE * probe));
        }
    }

    #[test]
    fn a_store_that_goes_down_says_so_once_then_stays_quiet_within_the_repeat() {
        let watch = Watch::default();
        let start = Instant::now();
        let (said, _) = down_for(&watch, start, REPEAT);
        assert_eq!(
            vec![Said::Down],
            said,
            "one line for the transition to down"
        );
    }

    #[test]
    fn a_store_that_stays_down_repeats_at_most_once_per_repeat_interval() {
        let watch = Watch::default();
        let start = Instant::now();
        let (said, _) = down_for(&watch, start, REPEAT * 3);
        assert_eq!(Some(&Said::Down), said.first());
        let repeats: Vec<Duration> = said
            .iter()
            .filter_map(|said| match said {
                Said::StillDown { for_ } => Some(*for_),
                _ => None,
            })
            .collect();
        assert_eq!(2, repeats.len(), "a bounded repeat: {said:?}");
        for pair in repeats.windows(2) {
            if let [earlier, later] = pair {
                assert!(later.saturating_sub(*earlier) >= REPEAT, "{repeats:?}");
            }
        }
        assert_eq!(said.len(), 1 + repeats.len(), "{said:?}");
    }

    #[test]
    fn a_store_that_comes_back_says_so_once_with_how_long_it_was_down() {
        let watch = Watch::default();
        let start = Instant::now();
        let (_, now) = down_for(&watch, start, PROBE * 6);
        assert_eq!(
            Some(Said::Back { for_: PROBE * 6 }),
            watch.observe(true, now)
        );
        assert_eq!(None, watch.observe(true, now + PROBE), "once");
        assert_eq!(
            Some(Said::Down),
            watch.observe(false, now + PROBE * 2),
            "down again is new"
        );
    }
}
