// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission report: one finding per identifier-integrity condition of
//! §12b.2, each with its verdict and the evidence behind it.
//!
//! The report prints the endpoint id, the node id, `system_id`s and the
//! `ehr_id`s the node created or the run read, and never a synthetic
//! subject: the evidence is redacted against every subject of the run before
//! it is kept.

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

/// How a check gathered its evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The check created test EHRs on the node and read each back.
    Full,
    /// The check made no write: it read the `ehr_id` and `system_id` of
    /// EHRs the node already holds, for a node whose governance forbids
    /// test data.
    ReadOnly,
}

/// What one admission check found on one member's endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    endpoint: EndpointId,
    node: NodeId,
    mode: Mode,
    created: Vec<String>,
    read: usize,
    findings: Vec<Finding>,
}

impl Report {
    /// The report of a full run on `endpoint` of `node`: the `ehr_id`s the
    /// node created and the findings, each line redacted against `subjects`.
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
            mode: Mode::Full,
            created: created.into_iter().map(redacted).collect(),
            read: 0,
            findings: findings
                .into_iter()
                .map(|finding| Finding {
                    evidence: finding.evidence.into_iter().map(redacted).collect(),
                    ..finding
                })
                .collect(),
        }
    }

    /// The report of a run without writes on `endpoint` of `node`: how many
    /// existing EHRs it read, and the findings.
    // NOTE: an existing EHR's ehr_id is the pseudonymous identifier of a real patient at the
    // node, so a run without writes names its EHRs by row and keeps no ehr_id (§5.4.1, N33).
    pub(super) fn read_only(
        endpoint: EndpointId,
        node: NodeId,
        read: usize,
        findings: Vec<Finding>,
    ) -> Self {
        Self {
            endpoint,
            node,
            mode: Mode::ReadOnly,
            created: Vec::new(),
            read,
            findings,
        }
    }

    /// How the check gathered its evidence.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// How many existing EHRs a run without writes read; their `ehr_id`s
    /// are never kept.
    #[must_use]
    pub fn read(&self) -> usize {
        self.read
    }

    /// The conditions the run left unproven: those it reports
    /// `cannot-check`, in the order of the §12b.2 table.
    #[must_use]
    pub fn unproven(&self) -> Vec<Condition> {
        self.findings
            .iter()
            .filter(|finding| finding.verdict == Verdict::CannotCheck)
            .map(|finding| finding.condition)
            .collect()
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
        match self.mode {
            Mode::Full => {
                writeln!(
                    f,
                    "This check creates test EHRs on the node, each with a synthetic EHR_STATUS subject in namespace {NAMESPACE}, and the node keeps every one it created."
                )?;
                listed(f, "EHRs created", &self.created)?;
            }
            Mode::ReadOnly => {
                writeln!(
                    f,
                    "This run made no write to the node: it read the ehr_id and system_id of EHRs the node already holds, and created no test EHR."
                )?;
                writeln!(f, "EHRs read: {}", self.read)?;
            }
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
        if self.mode == Mode::ReadOnly {
            let unproven: Vec<&str> = self.unproven().into_iter().map(Condition::name).collect();
            if !unproven.is_empty() {
                writeln!(
                    f,
                    "Left unproven by a run without writes: {}. §12b.1 asks for verification by test: prove these by a full run against a staging copy of the node, or by the node operator's documented procedures.",
                    unproven.join(", ")
                )?;
            }
        }
        write!(
            f,
            "Result: {} passed, {} failed, {} cannot be checked. The tool supplies evidence; the federation operator decides admission (§12b.1, N42a, CP-33a).",
            count(Verdict::Pass),
            count(Verdict::Fail),
            count(Verdict::CannotCheck)
        )
    }
}

/// Writes the line naming `what` and the `ehr_id`s of `ehr_ids`.
fn listed(f: &mut fmt::Formatter<'_>, what: &str, ehr_ids: &[String]) -> fmt::Result {
    if ehr_ids.is_empty() {
        writeln!(f, "{what}: none")
    } else {
        writeln!(f, "{what} ({}): {}", ehr_ids.len(), ehr_ids.join(", "))
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
