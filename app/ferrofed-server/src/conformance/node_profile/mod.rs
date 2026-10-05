// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Federation-Node profile checks: what a member CDR's ITS-REST interface
//! shows of the node obligations of §16.2.
//!
//! A node is invocable on its local `ehr_id` alone, never requires `subject`,
//! passes its own errors through, enforces consent before it releases data,
//! and meets the identifier-integrity conditions of §12b.2 (§16.2, N34). §17
//! scores CP-18, CP-19 and CP-27 against the node, never the gateway, and a
//! gateway cannot prove them. Each check exercises what a request to the
//! node's interface can observe and returns a [`Finding`] with its
//! [`Verdict`] and the evidence behind it, which an operator admitting a node
//! reads, which `conformance run --node-profile` reports against each member
//! ([`members`]), and which the harness records against its CDR products:
//!
//! | Check | Observed at the interface | Point |
//! |---|---|---|
//! | [`checks::invocable_on_ehr_id`] | the EHR, its `EHR_STATUS` and its compositions answer to the `ehr_id` alone | CP-27 |
//! | [`checks::subject_not_required`] | an EHR created with no subject is read and queried by its `ehr_id` | CP-27 |
//! | [`checks::errors_passed_through`] | an unknown `ehr_id` and an unparsable query answer the ITS-REST error status | CP-18 |
//! | [`checks::access_decided_at_node`] | an EHR the node's own policy withholds from a principal is refused on the read and on the `ehr_id`-scoped query | CP-18 |
//! | [`checks::consent_before_release`] | the same, for a refusal the operator arranged as a consent refusal | CP-19 |
//!
//! Every request goes through the endpoint's [`NodeClient`] and its outbound
//! gate, with the endpoint's onward credentials ([`interface::Interface`]).
//! The §12b.2 conditions are the gateway's admission check, whose findings
//! are recorded under [`Check::IdentifierIntegrity`]
//! ([`Finding::of_admission`]). A check that could not arrange what it needs
//! says so with [`Verdict::NotObservable`], never with a pass, and a node it
//! could not reach is a [`interface::CheckError`]: a check that reached
//! nothing concludes nothing.
//!
//! A [`Profile`] collects the findings for one product and writes them as
//! the tab-separated rows of `node-profile.tsv`, the node class of the
//! report, apart from the gateway's points. No specification governs the
//! form of the checks or of their record: our own design.
//!
//! [`NodeClient`]: ferrofed_engine::dispatch::NodeClient

pub mod checks;
pub mod interface;
pub mod members;

use std::fmt;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::admission::report::{self as admission, Condition};
use crate::conformance::report::{self, FINDINGS_HEADER};

/// One obligation of the Federation-Node profile, as a check observes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// The node is invocable on its local `ehr_id` alone (§5.5, N34).
    InvocableOnEhrId,
    /// The node never requires `subject` (§16.2).
    SubjectNotRequired,
    /// The node passes its own errors through (§16.2).
    ErrorsPassedThrough,
    /// The node makes the access decisions that need information the gateway
    /// cannot see (§13, N26).
    AccessDecidedAtNode,
    /// The node checks consent before it releases data, whatever was decided
    /// upstream (§13.2, N27).
    ConsentBeforeRelease,
    /// One identifier-integrity condition of §12b.2, as the admission check
    /// reports it.
    IdentifierIntegrity(Condition),
}

impl Check {
    /// Returns the check's name, the column a findings row carries.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::InvocableOnEhrId => "invocable on ehr_id alone",
            Self::SubjectNotRequired => "subject never required",
            Self::ErrorsPassedThrough => "node errors passed through",
            Self::AccessDecidedAtNode => "access decided at the node",
            Self::ConsentBeforeRelease => "consent before release",
            Self::IdentifierIntegrity(condition) => condition.name(),
        }
    }

    /// Returns the §17 conformance points the check assists.
    #[must_use]
    pub fn points(self) -> &'static [&'static str] {
        match self {
            Self::InvocableOnEhrId | Self::SubjectNotRequired => &["CP-27"],
            // NOTE: §16.2 lists error pass-through in the node profile and §17 scores no point for
            // it alone; it assists CP-18, whose delegated decisions reach the gateway as node errors.
            Self::ErrorsPassedThrough | Self::AccessDecidedAtNode => &["CP-18"],
            Self::ConsentBeforeRelease => &["CP-19"],
            // NOTE: §12b.2 names the ehr_id exchange of N34, which CP-27 scores; every condition
            // of its table is the operator's admission evidence, CP-33a.
            Self::IdentifierIntegrity(Condition::EhrIdExchange) => &["CP-27", "CP-33a"],
            Self::IdentifierIntegrity(_) => &["CP-33a"],
        }
    }
}

/// What a check concluded about one obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// What the interface showed meets the obligation.
    Pass,
    /// What the interface showed breaks it.
    Fail,
    /// The interface showed nothing that decides it; the evidence says why.
    NotObservable,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::NotObservable => "not-observable",
        })
    }
}

/// The verdict on one obligation, the evidence for it, and the EHRs the
/// check created on the node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The obligation.
    check: Check,
    /// The verdict.
    verdict: Verdict,
    /// One line per observation, in the order the check made them.
    evidence: Vec<String>,
    /// The EHRs the check created, which it never removes.
    created: Vec<Uuid>,
}

impl Finding {
    /// Creates the finding on `check` with `verdict` and its `evidence`.
    #[must_use]
    pub fn new(check: Check, verdict: Verdict, evidence: Vec<String>) -> Self {
        Self {
            check,
            verdict,
            evidence,
            created: Vec::new(),
        }
    }

    /// Creates the finding on `check` from its observations: one that fails
    /// fails it, else one that decides nothing leaves it not observable, else
    /// it passes. No observation at all leaves it not observable.
    #[must_use]
    pub fn from_observations(check: Check, observations: Vec<(Verdict, String)>) -> Self {
        let verdict = if observations.iter().any(|(v, _)| *v == Verdict::Fail) {
            Verdict::Fail
        } else if observations.is_empty()
            || observations
                .iter()
                .any(|(v, _)| *v == Verdict::NotObservable)
        {
            Verdict::NotObservable
        } else {
            Verdict::Pass
        };
        Self::new(
            check,
            verdict,
            observations.into_iter().map(|(_, line)| line).collect(),
        )
    }

    /// Creates the finding on `check` that nothing was arranged to observe
    /// it, for the `reason` given.
    #[must_use]
    pub fn not_arranged(check: Check, reason: impl Into<String>) -> Self {
        Self::new(check, Verdict::NotObservable, vec![reason.into()])
    }

    /// Creates the finding of one condition of an admission check, its
    /// cannot-check verdict read as not observable.
    #[must_use]
    pub fn of_admission(finding: &admission::Finding) -> Self {
        let verdict = match finding.verdict() {
            admission::Verdict::Pass => Verdict::Pass,
            admission::Verdict::Fail => Verdict::Fail,
            admission::Verdict::CannotCheck => Verdict::NotObservable,
        };
        Self::new(
            Check::IdentifierIntegrity(finding.condition()),
            verdict,
            finding.evidence().to_vec(),
        )
    }

    /// Returns the finding with the evidence lines `before` ahead of its own,
    /// for what was arranged before the check ran.
    #[must_use]
    pub fn after(mut self, before: Vec<String>) -> Self {
        let mut evidence = before;
        evidence.append(&mut self.evidence);
        self.evidence = evidence;
        self
    }

    /// Returns the finding recording that the check created `ehr_id`.
    #[must_use]
    pub fn with_created(mut self, ehr_id: Uuid) -> Self {
        self.created.push(ehr_id);
        self
    }

    /// Returns the obligation.
    #[must_use]
    pub fn check(&self) -> Check {
        self.check
    }

    /// Returns the verdict.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// Returns the evidence, one line per observation.
    #[must_use]
    pub fn evidence(&self) -> &[String] {
        &self.evidence
    }

    /// Returns the EHRs the check created on the node.
    #[must_use]
    pub fn created(&self) -> &[Uuid] {
        &self.created
    }
}

/// The findings of the node profile for one CDR product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// The product and its release, as the report names it.
    product: String,
    /// The findings, in the order they were recorded.
    findings: Vec<Finding>,
}

impl Profile {
    /// Creates an empty profile of `product`.
    #[must_use]
    pub fn new(product: impl Into<String>) -> Self {
        Self {
            product: product.into(),
            findings: Vec::new(),
        }
    }

    /// Records `finding`.
    pub fn record(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// Returns the product.
    #[must_use]
    pub fn product(&self) -> &str {
        &self.product
    }

    /// Returns the findings.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Returns the findings as the report's rows: one per finding and point
    /// it assists, the evidence lines joined with `; `.
    #[must_use]
    pub fn rows(&self) -> Vec<report::Finding> {
        let mut rows = Vec::new();
        for finding in &self.findings {
            let evidence = finding.evidence.join("; ");
            for point in finding.check.points() {
                rows.push(report::Finding {
                    product: self.product.clone(),
                    point: (*point).to_owned(),
                    check: finding.check.name().to_owned(),
                    verdict: finding.verdict.to_string(),
                    evidence: evidence.clone(),
                });
            }
        }
        rows
    }

    /// Returns the findings as tab-separated rows under [`FINDINGS_HEADER`],
    /// one row per finding and point it assists ([`Profile::rows`]).
    ///
    /// Tabs and line breaks inside a cell become spaces.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrofed_server::conformance::node_profile::{Check, Finding, Profile, Verdict};
    ///
    /// let mut profile = Profile::new("Example CDR 1.0");
    /// let evidence = vec!["GET /ehr/{ehr_id} answered 200".to_owned()];
    /// profile.record(Finding::new(Check::InvocableOnEhrId, Verdict::Pass, evidence));
    /// let rows = profile.to_tsv();
    /// assert!(rows.contains("Example CDR 1.0\tCP-27\tinvocable on ehr_id alone\tpass\t"));
    /// ```
    #[must_use]
    pub fn to_tsv(&self) -> String {
        let mut lines = vec![FINDINGS_HEADER.to_owned()];
        for row in self.rows() {
            lines.push(
                [
                    &row.product,
                    &row.point,
                    &row.check,
                    &row.verdict,
                    &row.evidence,
                ]
                .map(|text| cell(text))
                .join("\t"),
            );
        }
        lines.push(String::new());
        lines.join("\n")
    }

    /// Writes [`Profile::to_tsv`] to `dir/name.tsv`, creating `dir`, and
    /// returns the path written.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the directory cannot be created or the file
    /// cannot be written.
    pub fn write(&self, dir: &Path, name: &str) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{name}.tsv"));
        std::fs::write(&path, self.to_tsv())?;
        Ok(path)
    }
}

/// One cell of a findings row, with no tab or line break.
fn cell(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::{Check, Finding, Profile, Verdict};
    use crate::admission::report::{self as admission, Condition};

    #[test]
    fn the_worst_observation_decides_the_finding() {
        let pass = (Verdict::Pass, "seen".to_owned());
        let open = (Verdict::NotObservable, "unseen".to_owned());
        let fail = (Verdict::Fail, "broken".to_owned());
        let of = |lines: Vec<(Verdict, String)>| {
            Finding::from_observations(Check::InvocableOnEhrId, lines).verdict()
        };
        assert_eq!(Verdict::Pass, of(vec![pass.clone()]));
        assert_eq!(Verdict::NotObservable, of(vec![pass.clone(), open.clone()]));
        assert_eq!(Verdict::Fail, of(vec![open, fail, pass]));
        assert_eq!(Verdict::NotObservable, of(Vec::new()));
    }

    #[test]
    fn an_admission_finding_assists_the_points_of_its_condition() {
        let exchange = admission::Finding::new(
            Condition::EhrIdExchange,
            admission::Verdict::CannotCheck,
            vec!["no cross-reference".to_owned()],
        );
        let finding = Finding::of_admission(&exchange);
        assert_eq!(Verdict::NotObservable, finding.verdict());
        assert_eq!(&["CP-27", "CP-33a"], finding.check().points());
        assert_eq!("ehr_id exchange", finding.check().name());
        let generation = admission::Finding::new(
            Condition::EhrIdGeneration,
            admission::Verdict::Pass,
            Vec::new(),
        );
        assert_eq!(
            &["CP-33a"],
            Finding::of_admission(&generation).check().points()
        );
    }

    #[test]
    fn a_profile_writes_one_row_per_point_with_no_tab_inside_a_cell() {
        let mut profile = Profile::new("Example\tCDR");
        let exchange = admission::Finding::new(
            Condition::EhrIdExchange,
            admission::Verdict::Pass,
            vec!["one\ttwo".to_owned(), "three".to_owned()],
        );
        profile.record(Finding::of_admission(&exchange));
        let tsv = profile.to_tsv();
        let lines: Vec<&str> = tsv.lines().collect();
        assert_eq!(
            vec![
                "product\tpoint\tcheck\tverdict\tevidence",
                "Example CDR\tCP-27\tehr_id exchange\tpass\tone two; three",
                "Example CDR\tCP-33a\tehr_id exchange\tpass\tone two; three",
            ],
            lines
        );
    }
}
