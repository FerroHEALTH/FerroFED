// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the report keeps of each live answer: the fields that hold states,
//! counts, kinds, times and routing ids, and nothing that is free text.
//!
//! Readiness keeps each indicator's state without its `detail`, which can
//! quote an upstream error. The incidents keep their kind, time, detection
//! and the endpoints and nodes involved, without the description, the
//! `ehr_id` or the `creating_system_id`. The metrics keep the value of a
//! label only when its name is one the gateway sets ([`METRIC_LABELS`]);
//! any other label's value is [`REDACTED`]. Each answer is read into these
//! types and written again, so a field the gateway may add later is left
//! out until it is listed here (§5.4.1, N33). No specification governs the
//! report: our own design.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ferrofed_registry::secret::REDACTED;
use serde::{Deserialize, Serialize};

use crate::health::State;
use crate::health::lifecycle::Phase;

/// The label names whose values the metrics keep: those the gateway's own
/// instruments set, which carry closed sets and endpoint ids, and those the
/// exposition format and the OpenTelemetry resource add.
pub const METRIC_LABELS: &[&str] = &[
    "endpoint",
    "event",
    "kind",
    "le",
    "limit",
    "otel_scope_name",
    "otel_scope_version",
    "outcome",
    "quantile",
    "reason",
    "result",
    "service_name",
    "status_class",
    "telemetry_sdk_language",
    "telemetry_sdk_name",
    "telemetry_sdk_version",
];

/// Readiness, as the report keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Readiness {
    /// The aggregate.
    pub state: State,
    /// The phase of the process.
    pub phase: Phase,
    /// Each indicator's state, by name.
    pub indicators: BTreeMap<String, Indicator>,
}

/// One indicator, as the report keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Indicator {
    /// Whether the subsystem answered.
    pub state: State,
}

/// The integrity incidents, as the report keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incidents {
    /// How many incidents of each kind the process emitted.
    pub counts: BTreeMap<String, u64>,
    /// The most recent incidents of every kind, oldest first.
    pub recent: Vec<Incident>,
}

/// One incident, as the report keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    /// The incident's kind.
    pub kind: String,
    /// When it was emitted, RFC 3339.
    pub at: String,
    /// How the gateway found it, for an `ehr_id` collision.
    pub detection: Option<String>,
    /// The endpoints involved.
    pub endpoints: Vec<String>,
    /// The nodes involved.
    pub nodes: Vec<String>,
}

/// Returns the Prometheus text `metrics` with unlisted label values redacted.
///
/// The value of every label [`METRIC_LABELS`] does not name becomes
/// [`REDACTED`], and a sample line whose labels do not read becomes a
/// comment saying so.
#[must_use]
pub fn metrics(metrics: &str) -> String {
    let mut out = String::with_capacity(metrics.len());
    for line in metrics.lines() {
        if line.starts_with('#') || !line.contains('{') {
            out.push_str(line);
        } else {
            match labelled(line) {
                Some(kept) => out.push_str(&kept),
                None => out.push_str("# a sample whose labels did not read was left out"),
            }
        }
        out.push('\n');
    }
    out
}

/// Returns the sample `line` with its labels redacted, or `None` when its
/// label set does not read.
fn labelled(line: &str) -> Option<String> {
    let (name, rest) = line.split_once('{')?;
    let mut out = format!("{name}{{");
    let mut chars = rest.chars();
    loop {
        let mut label = String::new();
        let closed = loop {
            match chars.next()? {
                '=' => break false,
                '}' if label.is_empty() => break true,
                ',' if label.is_empty() => {}
                c => label.push(c),
            }
        };
        if closed {
            break;
        }
        if chars.next()? != '"' {
            return None;
        }
        let mut value = String::new();
        loop {
            match chars.next()? {
                '\\' => {
                    value.push('\\');
                    value.push(chars.next()?);
                }
                '"' => break,
                c => value.push(c),
            }
        }
        if !out.ends_with('{') {
            out.push(',');
        }
        let shown = if METRIC_LABELS.contains(&label.trim()) {
            value.as_str()
        } else {
            REDACTED
        };
        // NOTE: no specification governs this: our own design; writing to a
        // String cannot fail.
        let _written: std::fmt::Result = write!(out, "{}=\"{shown}\"", label.trim());
    }
    out.push('}');
    out.extend(chars);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{Incidents, Readiness, metrics};

    #[test]
    fn a_label_the_gateway_does_not_set_is_redacted() {
        let text = "# HELP x A counter.\n# TYPE x counter\n\
            x_total{endpoint=\"node-a-pub\",subject=\"CANARY-PATIENT\",outcome=\"ok\"} 3\n\
            y 1\n\
            z{reason=\"a \\\"quoted\\\" value\"} 2\n\
            broken{endpoint=\"CANARY-UNCLOSED 1\n";
        let kept = metrics(text);
        assert!(!kept.contains("CANARY"), "{kept}");
        assert!(kept.contains("x_total{endpoint=\"node-a-pub\",subject=\"***\",outcome=\"ok\"} 3"));
        assert!(
            kept.contains("z{reason=\"a \\\"quoted\\\" value\"} 2"),
            "{kept}"
        );
        assert!(kept.contains("y 1"));
        assert!(kept.contains("# TYPE x counter"));
    }

    #[test]
    fn readiness_and_incidents_drop_their_free_text() {
        let readiness: Readiness = serde_json::from_str(
            r#"{"state":"down","phase":"serving","indicators":{"store":{"state":"down","detail":"CANARY-DETAIL"}}}"#,
        )
        .expect("a readiness body");
        let written = serde_json::to_string(&readiness).expect("it writes");
        assert!(!written.contains("CANARY"), "{written}");
        let incidents: Incidents = serde_json::from_str(
            r#"{"counts":{"EhrIdCollision":1},"recent":[{"kind":"EhrIdCollision","at":"2026-10-06T00:00:00Z","description":"ehr_id 7d44b88c-4199-4bad-97dc-d78268e01398 CANARY","creating_system_id":"CANARY-SYSTEM","ehr_id":"7d44b88c-4199-4bad-97dc-d78268e01398","detection":"ask-all","endpoints":["node-a-pub"],"nodes":[]}]}"#,
        )
        .expect("an incident report");
        let written = serde_json::to_string(&incidents).expect("it writes");
        assert!(!written.contains("CANARY"), "{written}");
        assert!(!written.contains("7d44b88c"), "{written}");
        assert!(written.contains("node-a-pub"), "{written}");
    }
}
