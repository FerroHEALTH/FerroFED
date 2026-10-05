// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission report: one finding per identifier-integrity condition of
//! §12b.2, each with its verdict and the evidence behind it.
//!
//! The report prints the endpoint id, the node id, `system_id`s and the
//! `ehr_id`s the node created, and never a synthetic subject: the evidence
//! is redacted against every subject of the run before it is kept.

use std::fmt;

use ferrofed_registry::id::{EndpointId, NodeId};
use secrecy::{ExposeSecret, SecretString};

use super::subject::NAMESPACE;

/// What the check concluded about one condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The evidence shows the node meets the condition.
    Pass,
    /// The evidence shows the node does not meet it, or the check could not
    /// reach the node at all.
    Fail,
    /// The check cannot decide the condition; the reason says why.
    CannotCheck,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::CannotCheck => "cannot-check",
        })
    }
}

/// One identifier-integrity condition of §12b.2, in the order of its table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// `ehr_id`s are version-4 UUIDs, or another scheme with equivalent
    /// collision resistance and no coordination requirement.
    EhrIdGeneration,
    /// No `ehr_id` is re-issued or reused, across restores, migrations and
    /// test-data resets included.
    NoReuse,
    /// No foreign `ehr_id` is adopted on import.
    NoForeignAdoption,
    /// The node's `system_id` is unique across the federation.
    SystemIdUniqueness,
    /// The node's environment resolves a patient identifier to the node's
    /// local `ehr_id`.
    EhrIdExchange,
}

impl Condition {
    /// The condition's name, as the §12b.2 table names it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::EhrIdGeneration => "ehr_id generation",
            Self::NoReuse => "no reuse",
            Self::NoForeignAdoption => "no adoption of foreign ehr_ids",
            Self::SystemIdUniqueness => "system_id uniqueness",
            Self::EhrIdExchange => "ehr_id exchange",
        }
    }
}

/// The verdict on one condition and the evidence for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    condition: Condition,
    verdict: Verdict,
    evidence: Vec<String>,
}

impl Finding {
    /// A finding on `condition`.
    #[must_use]
    pub fn new(condition: Condition, verdict: Verdict, evidence: Vec<String>) -> Self {
        Self {
            condition,
            verdict,
            evidence,
        }
    }

    /// The condition.
    #[must_use]
    pub fn condition(&self) -> Condition {
        self.condition
    }

    /// The verdict.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// The evidence, one line each.
    #[must_use]
    pub fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

/// What one admission check found on one member's endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    endpoint: EndpointId,
    node: NodeId,
    created: Vec<String>,
    findings: Vec<Finding>,
}

impl Report {
    /// The report on `endpoint` of `node`: the `ehr_id`s the node created
    /// and the findings, each line redacted against `subjects`.
    pub(super) fn new(
        endpoint: EndpointId,
        node: NodeId,
        created: Vec<String>,
        findings: Vec<Finding>,
        subjects: &[&SecretString],
    ) -> Self {
        let redacted = |line: String| redact(line, subjects);
        Self {
            endpoint,
            node,
            created: created.into_iter().map(redacted).collect(),
            findings: findings
                .into_iter()
                .map(|finding| Finding {
                    evidence: finding.evidence.into_iter().map(redacted).collect(),
                    ..finding
                })
                .collect(),
        }
    }

    /// The endpoint the check ran against.
    #[must_use]
    pub fn endpoint(&self) -> &EndpointId {
        &self.endpoint
    }

    /// The node the endpoint belongs to.
    #[must_use]
    pub fn node(&self) -> &NodeId {
        &self.node
    }

    /// The `ehr_id`s of the test EHRs the node created, as it named them.
    #[must_use]
    pub fn created(&self) -> &[String] {
        &self.created
    }

    /// One finding per condition of §12b.2, in the order of its table.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// The finding on `condition`.
    #[must_use]
    pub fn finding(&self, condition: Condition) -> Option<&Finding> {
        self.findings
            .iter()
            .find(|finding| finding.condition == condition)
    }

    /// Whether any condition failed.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.verdict == Verdict::Fail)
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "ferrofed admission check: endpoint {} of node {}",
            self.endpoint, self.node
        )?;
        writeln!(
            f,
            "This check creates test EHRs on the node, each with a synthetic EHR_STATUS subject in namespace {NAMESPACE}, and the node keeps every one it created."
        )?;
        if self.created.is_empty() {
            writeln!(f, "EHRs created: none")?;
        } else {
            writeln!(
                f,
                "EHRs created ({}): {}",
                self.created.len(),
                self.created.join(", ")
            )?;
        }
        for finding in &self.findings {
            writeln!(f)?;
            writeln!(
                f,
                "[{}] {} (§12b.2, N42a)",
                finding.verdict,
                finding.condition.name()
            )?;
            for line in &finding.evidence {
                writeln!(f, "  - {line}")?;
            }
        }
        let count = |verdict: Verdict| {
            self.findings
                .iter()
                .filter(|finding| finding.verdict == verdict)
                .count()
        };
        writeln!(f)?;
        write!(
            f,
            "Result: {} passed, {} failed, {} cannot be checked. The tool supplies evidence; the federation operator decides admission (§12b.1, N42a, CP-33a).",
            count(Verdict::Pass),
            count(Verdict::Fail),
            count(Verdict::CannotCheck)
        )
    }
}

/// `line` with every occurrence of each of `subjects` replaced by a marker.
fn redact(line: String, subjects: &[&SecretString]) -> String {
    subjects.iter().fold(line, |line, subject| {
        let value = subject.expose_secret();
        if value.is_empty() {
            line
        } else {
            line.replace(value, "[synthetic subject]")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::redact;
    use secrecy::SecretString;

    #[test]
    fn a_subject_in_a_line_is_replaced() {
        let subject = SecretString::from("ffd-admission-0f");
        assert_eq!(
            "the node echoed [synthetic subject] back",
            redact(
                "the node echoed ffd-admission-0f back".to_owned(),
                &[&subject]
            )
        );
    }
}
