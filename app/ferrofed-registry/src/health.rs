// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The body of `GET /health/dependencies`, which the gateway writes and the
//! operator console reads.
//!
//! The report names each member endpoint by id and every other dependency by
//! key, each with the last state the gateway observed of it, and nothing
//! else: no request, no patient identifier and no credential. No
//! specification governs health probes: our own design.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The last state observed of one dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// It is reachable, and work it has not taken yet waits for it: the
    /// audit repository while its spool holds messages, and the care
    /// services directory while the change it answered with is refused.
    Degraded,
}

impl Observed {
    /// Returns the state as the report names it: `unknown`, `up`,
    /// `failing`, `down` or `degraded`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Up => "up",
            Self::Failing => "failing",
            Self::Down => "down",
            Self::Degraded => "degraded",
        }
    }
}

/// What one indication of `GET /health/dependencies` says: a state, or the
/// class of a fault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Indication {
    /// The last state observed.
    State(Observed),
    /// Why the dependency is not up, by class.
    Fault(String),
}

impl Indication {
    /// Returns the indication as the report names it: the state, or the
    /// fault's class.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::State(observed) => observed.as_str(),
            Self::Fault(class) => class,
        }
    }
}

/// The body of `GET /health/dependencies`: endpoint ids, dependency keys and
/// their states, and nothing else.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyReport {
    /// Every member endpoint by id, in id order.
    pub endpoints: BTreeMap<String, Observed>,
    /// The resolver's state, absent when no resolver is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver: Option<Observed>,
    /// The consent pre-filter's state, absent when no pre-filter is
    /// configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent: Option<Observed>,
    /// The localizer's state, absent when no localizer is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub localizer: Option<Observed>,
    /// The state of the demographics service the gateway asks for a master
    /// identity, absent when none is configured (`[pdqm]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demographics: Option<Observed>,
    /// What each binding indicates of the services it runs or records
    /// through, by key, each absent when the binding configures none.
    ///
    /// The IHE binding indicates `directory`, the care services directory the
    /// registry is read from (`up` after an answer the gateway accepted,
    /// `degraded` after an answer whose change it refused, `failing` after an
    /// HTTP error or an answer that breaks ITI-90 or ITI-91, `down` when it
    /// did not answer), with `directory_fault` (`registry-invalid`,
    /// `configuration-mismatch` or `refused-credentials`); `identity_registry`,
    /// the PMIR Patient Identity Registry (`up` while it holds the
    /// subscription, `failing` after a refusal, a broken answer or a
    /// subscription in `error` or `off`, `down` when it did not answer), with
    /// `identity_registry_fault` (`unreachable`, `refused`, `malformed`,
    /// `unmanageable` or `audit-failed`); `audit_repository`, the ATNA
    /// repository the ITI-55 audit messages go to; and `audit_feed`, the
    /// repository the PIXm, PDQm, mCSD and PMIR records are posted to.
    #[serde(flatten)]
    pub bindings: BTreeMap<String, Indication>,
}

impl DependencyReport {
    /// Returns every dependency but the member endpoints, by key in key
    /// order, each with its state or its fault's class as the report names
    /// it.
    #[must_use]
    pub fn services(&self) -> BTreeMap<&str, &str> {
        [
            ("resolver", self.resolver),
            ("consent", self.consent),
            ("localizer", self.localizer),
            ("demographics", self.demographics),
        ]
        .into_iter()
        .filter_map(|(key, observed)| observed.map(|observed| (key, observed.as_str())))
        .chain(
            self.bindings
                .iter()
                .map(|(key, indication)| (key.as_str(), indication.as_str())),
        )
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyReport, Indication, Observed};

    #[test]
    fn every_state_is_named_on_the_wire_as_it_reads() {
        for observed in [
            Observed::Unknown,
            Observed::Up,
            Observed::Failing,
            Observed::Down,
            Observed::Degraded,
        ] {
            assert_eq!(
                format!("\"{}\"", observed.as_str()),
                serde_json::to_string(&observed).expect("a state serializes"),
            );
        }
    }

    #[test]
    fn a_report_reads_back_as_it_was_written() {
        let mut report = DependencyReport {
            resolver: Some(Observed::Up),
            consent: Some(Observed::Down),
            ..DependencyReport::default()
        };
        report
            .endpoints
            .insert("node-a-pub".to_owned(), Observed::Failing);
        report.bindings.insert(
            "directory".to_owned(),
            Indication::State(Observed::Degraded),
        );
        report.bindings.insert(
            "directory_fault".to_owned(),
            Indication::Fault("registry-invalid".to_owned()),
        );
        let text = serde_json::to_string(&report).expect("a report serializes");
        assert_eq!(
            r#"{"endpoints":{"node-a-pub":"failing"},"resolver":"up","consent":"down","directory":"degraded","directory_fault":"registry-invalid"}"#,
            text
        );
        let read: DependencyReport = serde_json::from_str(&text).expect("a report reads");
        assert_eq!(report, read);
        assert_eq!(
            vec![
                ("consent", "down"),
                ("directory", "degraded"),
                ("directory_fault", "registry-invalid"),
                ("resolver", "up"),
            ],
            read.services().into_iter().collect::<Vec<_>>()
        );
    }
}
