// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[anomaly]` table: the thresholds of the requests national contact
//! points relay, past which the gateway raises an anomaly warning.
//!
//! The `NCPeH` Architecture Specifications 10.0.0 §5.1.9 give the contact
//! point an anomaly detection "based on a configurable rule", with three
//! warnings: too many requests for one patient
//! (`EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS_FOR_UNIQUE_PATIENT`),
//! from one point of care (`..._FOR_UNIQUE_POINT_OF_CARE`), and in all
//! (`EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS`), each "triggered if
//! the event happens in a specified time interval". A breach is a warning
//! in the audit record and "does not block traffic". Counting behind the
//! contact point is our own design, as the emergency limit of
//! [`crate::config::emergency`] is.
//!
//! Each rule is its own table, `[anomaly.patient]`,
//! `[anomaly.point_of_care]` and `[anomaly.requests]`, with a `threshold`
//! and a `window`; a rule left out is not counted. A threshold or a window
//! of zero is refused.

use ferrohealth_config::units;
use std::num::NonZeroU32;
use std::time::Duration;

use serde::Deserialize;

use crate::config::error::Error;

/// `[anomaly]` as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Anomaly {
    /// The requests for one patient, by the patient the treatment
    /// relationship confirmation names.
    pub patient: Option<RuleTable>,
    /// The requests from one point of care, as the identity assertion names
    /// it.
    pub point_of_care: Option<RuleTable>,
    /// Every request of every contact point.
    pub requests: Option<RuleTable>,
}

/// One `[anomaly.*]` rule as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleTable {
    /// How many requests the window holds before the next one is a breach.
    pub threshold: u32,
    /// The window the requests are counted in.
    pub window: units::Duration,
}

/// One rule, resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// How many requests one window holds before the next one is a breach.
    pub threshold: NonZeroU32,
    /// The window, which opens at the first request counted in it.
    pub window: Duration,
}

/// `[anomaly]`, resolved: the rules in effect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnomalyRules {
    /// The rule of the requests for one patient.
    pub patient: Option<Rule>,
    /// The rule of the requests from one point of care.
    pub point_of_care: Option<Rule>,
    /// The rule of every request.
    pub requests: Option<Rule>,
}

impl AnomalyRules {
    /// Whether any rule is in effect.
    #[must_use]
    pub fn any(&self) -> bool {
        self.patient.is_some() || self.point_of_care.is_some() || self.requests.is_some()
    }
}

/// Resolves `[anomaly]`.
///
/// # Errors
///
/// [`Error::Zero`] for a rule whose threshold or window is zero, naming its
/// key.
pub fn resolve(table: &Anomaly) -> Result<AnomalyRules, Error> {
    Ok(AnomalyRules {
        patient: rule("anomaly.patient", table.patient.as_ref())?,
        point_of_care: rule("anomaly.point_of_care", table.point_of_care.as_ref())?,
        requests: rule("anomaly.requests", table.requests.as_ref())?,
    })
}

/// The rule `written` under `key`, or `None` when it is left out.
fn rule(key: &str, written: Option<&RuleTable>) -> Result<Option<Rule>, Error> {
    let Some(written) = written else {
        return Ok(None);
    };
    let threshold = NonZeroU32::new(written.threshold).ok_or_else(|| Error::Zero {
        key: format!("{key}.threshold"),
    })?;
    if written.window.as_secs() == 0 {
        return Err(Error::Zero {
            key: format!("{key}.window"),
        });
    }
    Ok(Some(Rule {
        threshold,
        window: Duration::from_secs(written.window.as_secs()),
    }))
}

#[cfg(test)]
mod tests {
    use super::{Anomaly, AnomalyRules, resolve};

    /// `[anomaly]` read from `text`.
    fn read(text: &str) -> Anomaly {
        match toml::from_str(text) {
            Ok(table) => table,
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn no_rule_is_in_effect_by_default() {
        let rules = resolve(&Anomaly::default()).map_err(|error| error.to_string());
        assert_eq!(Ok(AnomalyRules::default()), rules);
        assert!(!AnomalyRules::default().any());
    }

    #[test]
    fn each_rule_resolves_with_its_threshold_and_window() {
        let table = read(
            "[patient]\nthreshold = 20\nwindow = '1h'\n\n[requests]\nthreshold = 1000\nwindow = '10min'\n",
        );
        let rules = resolve(&table).map_err(|error| error.to_string());
        let Ok(rules) = rules else {
            panic!("{rules:?}");
        };
        assert_eq!(
            Some((20, 3600)),
            rules
                .patient
                .map(|rule| (rule.threshold.get(), rule.window.as_secs()))
        );
        assert_eq!(None, rules.point_of_care);
        assert_eq!(
            Some((1000, 600)),
            rules
                .requests
                .map(|rule| (rule.threshold.get(), rule.window.as_secs()))
        );
        assert!(rules.any());
    }

    #[test]
    fn a_zero_threshold_or_window_is_refused_naming_its_key() {
        for (text, key) in [
            (
                "[point_of_care]\nthreshold = 0\nwindow = '1h'\n",
                "anomaly.point_of_care.threshold",
            ),
            (
                "[patient]\nthreshold = 3\nwindow = '0s'\n",
                "anomaly.patient.window",
            ),
        ] {
            let refused = resolve(&read(text)).map_err(|error| error.to_string());
            let Err(message) = refused else {
                panic!("{text}: {refused:?}");
            };
            assert!(message.contains(key), "{message}");
        }
    }
}
