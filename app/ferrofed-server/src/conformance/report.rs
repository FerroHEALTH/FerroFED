// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The report of a conformance run: one row per §16.3 track and per §17
//! point, the node profile findings, and every scenario that did not pass,
//! the per-track and per-point logging §16.4 asks for.
//!
//! The rows have the columns and the result words of the report
//! `scripts/conformance/report.sh` writes from a harness run: `kind`, `id`,
//! `title`, `actor`, `status`, `result`, `passed`, `failed`, `not_run`,
//! `issue` and `reason`, over the tables `conformance/tracks.tsv` and
//! `conformance/matrix.tsv` as this build carries them. A covered row passes
//! when every scenario scoring it ran and passed, fails when one failed, and
//! is `not-run` when one did not run or none scores it, with the run's
//! reason in place of the table's. A Node or Operator point is its own class
//! and reads from the node profile findings, never a gateway pass (§16.2).
//! No specification governs the report's form: our own design.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::conformance::catalogue::Entry;
use crate::conformance::execute::Outcome;
use crate::conformance::seed::Written;

/// The track table this build carries.
const TRACKS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../conformance/tracks.tsv"
));

/// The conformance matrix this build carries.
const MATRIX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../conformance/matrix.tsv"
));

/// The header of `report.tsv`.
pub const REPORT_HEADER: &str =
    "kind\tid\ttitle\tactor\tstatus\tresult\tpassed\tfailed\tnot_run\tissue\treason";

/// The header of `node-profile.tsv`.
pub const FINDINGS_HEADER: &str = "product\tpoint\tcheck\tverdict\tevidence";

/// The reason of a covered row no scenario of a run scores.
pub const UNSCORED: &str = "no scenario of a conformance run scores this; the harness and the mock-node suites score it, with fault injection and node-side capture";

/// One node profile finding: what a check observed of one product, on one
/// point it assists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The CDR product and its release, or the node id.
    pub product: String,
    /// The §17 point.
    pub point: String,
    /// The check, as the §12b.2 table names its condition.
    pub check: String,
    /// `pass`, `fail` or `not-observable`.
    pub verdict: String,
    /// The evidence, one line per observation joined with `; `.
    pub evidence: String,
}

/// One row of the report, as `report.tsv` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// `track`, or the point's class: `gateway`, `node` or `operator`.
    pub kind: String,
    /// The track number or the point id.
    pub id: String,
    /// The track's title, `-` for a point.
    pub title: String,
    /// The point's actor, `-` for a track.
    pub actor: String,
    /// The status the table holds.
    pub status: String,
    /// The result of the run.
    pub result: String,
    /// The scenarios scoring the row that passed.
    pub passed: usize,
    /// Those that failed.
    pub failed: usize,
    /// Those that did not run.
    pub not_run: usize,
    /// The issues the table names.
    pub issue: String,
    /// The table's reason, or the run's where a covered row did not pass.
    pub reason: String,
}

/// A conformance run's report.
#[derive(Debug, Clone)]
pub struct Report {
    rows: Vec<Row>,
    scenarios: Vec<(String, String, Outcome)>,
    findings: Vec<Finding>,
    written: Vec<Written>,
    scored: String,
    cleartext: Vec<String>,
}

/// The counts of one token over the outcomes.
#[derive(Debug, Default, Clone)]
struct Tally {
    passed: usize,
    failed: usize,
    not_run: usize,
    reasons: Vec<String>,
}

impl Report {
    /// Builds the report of `outcomes`, one per catalogue entry, with the
    /// node profile `findings` and the writes `written`; `scored` names the
    /// deployment in the report's first lines.
    #[must_use]
    pub fn new(
        outcomes: &[(&Entry, Outcome)],
        findings: Vec<Finding>,
        written: Vec<Written>,
        scored: impl Into<String>,
    ) -> Self {
        let mut tallies: BTreeMap<&str, Tally> = BTreeMap::new();
        for (entry, outcome) in outcomes {
            for token in entry.tokens {
                let tally = tallies.entry(token).or_default();
                match outcome {
                    Outcome::Pass => tally.passed = tally.passed.saturating_add(1),
                    Outcome::Fail(reason) => {
                        tally.failed = tally.failed.saturating_add(1);
                        tally
                            .reasons
                            .push(format!("{}: failed: {reason}", entry.name));
                    }
                    Outcome::NotRun(reason) => {
                        tally.not_run = tally.not_run.saturating_add(1);
                        tally.reasons.push(format!("{}: {reason}", entry.name));
                    }
                }
            }
        }
        let mut rows = table(TRACKS, "track", &tallies, &findings);
        rows.extend(table(MATRIX, "point", &tallies, &findings));
        Self {
            rows,
            scenarios: outcomes
                .iter()
                .map(|(entry, outcome)| {
                    (
                        entry.name.to_owned(),
                        entry.tokens.join(" "),
                        outcome.clone(),
                    )
                })
                .collect(),
            findings,
            written,
            scored: scored.into(),
            cleartext: Vec::new(),
        }
    }

    /// Returns the report naming `lines`, each a site the run sent a
    /// credential or synthetic data to unencrypted, which only the
    /// development profile allows.
    #[must_use]
    pub fn with_cleartext(mut self, lines: Vec<String>) -> Self {
        self.cleartext = lines;
        self
    }

    /// Returns the rows, the tracks first.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Returns the row of `kind` and `id`, as `report.tsv` names them.
    #[must_use]
    pub fn row(&self, kind: &str, id: &str) -> Option<&Row> {
        self.rows
            .iter()
            .find(|row| row.kind == kind && row.id == id)
    }

    /// Whether a scenario failed, which a run exits `1` for.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.scenarios
            .iter()
            .any(|(_, _, outcome)| matches!(outcome, Outcome::Fail(_)))
    }

    /// Returns the count of each result over the rows, in result order.
    #[must_use]
    pub fn counts(&self) -> BTreeMap<&str, usize> {
        let mut counts = BTreeMap::new();
        for row in &self.rows {
            let count: &mut usize = counts.entry(row.result.as_str()).or_default();
            *count = count.saturating_add(1);
        }
        counts
    }

    /// Returns `report.tsv`.
    #[must_use]
    pub fn to_tsv(&self) -> String {
        let mut text = format!("{REPORT_HEADER}\n");
        for row in &self.rows {
            let cells = [
                row.kind.as_str(),
                &row.id,
                &row.title,
                &row.actor,
                &row.status,
                &row.result,
                &row.passed.to_string(),
                &row.failed.to_string(),
                &row.not_run.to_string(),
                &row.issue,
                &row.reason,
            ];
            let line: Vec<String> = cells.iter().map(|cell| tsv_cell(cell)).collect();
            text.push_str(&line.join("\t"));
            text.push('\n');
        }
        text
    }

    /// Returns `node-profile.tsv`.
    #[must_use]
    pub fn findings_tsv(&self) -> String {
        let mut text = format!("{FINDINGS_HEADER}\n");
        for finding in &self.findings {
            let cells = [
                &finding.product,
                &finding.point,
                &finding.check,
                &finding.verdict,
                &finding.evidence,
            ];
            let line: Vec<String> = cells.iter().map(|cell| tsv_cell(cell)).collect();
            text.push_str(&line.join("\t"));
            text.push('\n');
        }
        text
    }

    /// Returns `written.tsv`: every write the run made.
    #[must_use]
    pub fn written_tsv(&self) -> String {
        let mut text = "endpoint\twhat\n".to_owned();
        for written in &self.written {
            let _written = writeln!(
                text,
                "{}\t{}",
                tsv_cell(&written.endpoint),
                tsv_cell(&written.what)
            );
        }
        text
    }

    /// Returns `report.md`.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut text = String::new();
        let _intro = write!(
            text,
            "# Conformance report\n\nThe section 16.3 tracks and the section 17 conformance points of the Federation Tier with AQL, scored from the scenarios a conformance run drove against {} (section 16.4). FerroFED {}.\n\n",
            md_cell(&self.scored),
            crate::body::VERSION
        );
        text.push_str("A track or point passes only when every scenario that scores it ran and passed. A scenario that needs a fault injected at a node, node-side wire capture or a gateway configured for it is not-run against a deployment, with its reason, and so is every row it scores. A deferred track or point is not run, by a recorded decision. An open point has no test yet. A Node or Operator point is scored against that actor and never the gateway (section 16.2): its result comes from the node profile findings, and is never a gateway pass.\n");
        for (kind, heading) in [
            ("track", "Tracks"),
            ("gateway", "Gateway points"),
            ("node", "Node points"),
            ("operator", "Operator points"),
        ] {
            let rows: Vec<&Row> = self.rows.iter().filter(|row| row.kind == kind).collect();
            if rows.is_empty() {
                continue;
            }
            let first = if kind == "track" {
                "| Track | Title | Result | Tests passed, failed, not run | Issue | Reason |"
            } else {
                "| Point | Actor | Result | Tests passed, failed, not run | Issue | Reason |"
            };
            let _table = write!(
                text,
                "\n## {heading}\n\n{first}\n|---|---|---|---|---|---|\n"
            );
            for row in rows {
                let name = if kind == "track" {
                    &row.title
                } else {
                    &row.actor
                };
                let reason = if row.result == "pass" {
                    "-".to_owned()
                } else {
                    md_cell(&row.reason)
                };
                let _row = writeln!(
                    text,
                    "| {} | {} | {} | {}, {}, {} | {} | {reason} |",
                    row.id,
                    md_cell(name),
                    row.result,
                    row.passed,
                    row.failed,
                    row.not_run,
                    md_cell(&row.issue)
                );
            }
        }
        self.findings_markdown(&mut text);
        self.appendix(&mut text);
        text
    }

    /// Appends the node profile findings to `text`, as `report.sh` renders
    /// them: one row per point and check with a verdict column per product,
    /// products in name order and checks in the order first recorded, the
    /// worse verdict where a product recorded one check on one point twice,
    /// then one table per product with the evidence.
    fn findings_markdown(&self, text: &mut String) {
        text.push_str("\n## Node profile findings\n\n");
        if self.findings.is_empty() {
            text.push_str("No node profile finding was recorded in this run.\n");
            return;
        }
        text.push_str("What the node profile checks observed at each member's CDR product, per point they assist (section 16.2). A Node point above reads the worst verdict any product earned on it; this table shows each product apart, and a dash marks a check that product recorded no finding for.\n\n");
        let rank = |verdict: &str| match verdict {
            "fail" => 3,
            "not-observable" => 2,
            _ => 1,
        };
        let products: std::collections::BTreeSet<&str> = self
            .findings
            .iter()
            .map(|finding| finding.product.as_str())
            .collect();
        let mut keys: Vec<(&str, &str)> = Vec::new();
        let mut verdicts: BTreeMap<(&str, &str, &str), &str> = BTreeMap::new();
        for finding in &self.findings {
            let key = (finding.point.as_str(), finding.check.as_str());
            if !keys.contains(&key) {
                keys.push(key);
            }
            let slot = verdicts
                .entry((key.0, key.1, finding.product.as_str()))
                .or_insert(finding.verdict.as_str());
            if rank(&finding.verdict) > rank(slot) {
                *slot = finding.verdict.as_str();
            }
        }
        let mut header = "| Point | Check |".to_owned();
        let mut rule = "|---|---|".to_owned();
        for product in &products {
            let _column = write!(header, " {} |", md_cell(product));
            rule.push_str("---|");
        }
        let _summary = writeln!(text, "{header}\n{rule}");
        for (point, check) in &keys {
            let mut line = format!("| {point} | {} |", md_cell(check));
            for product in &products {
                let verdict = verdicts.get(&(*point, *check, *product)).unwrap_or(&"-");
                let _cell = write!(line, " {verdict} |");
            }
            let _line = writeln!(text, "{line}");
        }
        for product in &products {
            let _table = write!(
                text,
                "\n### {}\n\n| Point | Check | Verdict | Evidence |\n|---|---|---|---|\n",
                md_cell(product)
            );
            for finding in self
                .findings
                .iter()
                .filter(|finding| finding.product == *product)
            {
                let _finding = writeln!(
                    text,
                    "| {} | {} | {} | {} |",
                    finding.point,
                    md_cell(&finding.check),
                    finding.verdict,
                    md_cell(&finding.evidence)
                );
            }
        }
    }

    /// Appends the scenarios that did not pass, the unencrypted connections
    /// and the writes to `text`, the closing sections of `report.md`.
    fn appendix(&self, text: &mut String) {
        let unpassed: Vec<_> = self
            .scenarios
            .iter()
            .filter(|(_, _, outcome)| *outcome != Outcome::Pass)
            .collect();
        if !unpassed.is_empty() {
            text.push_str("\n## Scenarios that did not pass\n\n");
            for (name, tokens, outcome) in unpassed {
                let _line = writeln!(
                    text,
                    "- {}: `{name}` ({tokens}): {}",
                    outcome.word(),
                    md_cell(outcome.reason().unwrap_or_default())
                );
            }
        }
        if !self.cleartext.is_empty() {
            text.push_str("\n## Unencrypted connections\n\nThe development profile admits plain http to a loopback host, and this run used it:\n\n");
            for line in &self.cleartext {
                let _line = writeln!(text, "- {}", md_cell(line));
            }
        }
        text.push_str("\n## What this run wrote\n\n");
        if self.written.is_empty() {
            text.push_str("Nothing.\n");
        } else {
            text.push_str("Every write is synthetic and was made by this run; the run deleted nothing.\n\n| Endpoint | What |\n|---|---|\n");
            for written in &self.written {
                let _written = writeln!(
                    text,
                    "| {} | {} |",
                    md_cell(&written.endpoint),
                    md_cell(&written.what)
                );
            }
        }
    }

    /// Writes `report.tsv`, `report.md`, `node-profile.tsv` and
    /// `written.tsv` into `dir`, creating it, and returns the paths written.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the directory cannot be created or a file
    /// cannot be written.
    pub fn write(&self, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
        std::fs::create_dir_all(dir)?;
        let mut paths = Vec::new();
        for (name, text) in [
            ("report.tsv", self.to_tsv()),
            ("report.md", self.to_markdown()),
            ("node-profile.tsv", self.findings_tsv()),
            ("written.tsv", self.written_tsv()),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, text)?;
            paths.push(path);
        }
        Ok(paths)
    }
}

/// The report rows of the data rows of `source`, a table of `kind` (`track`
/// or `point`), joined with `tallies` and `findings`, as `report.sh` joins
/// a harness run's results.
fn table(
    source: &str,
    kind: &str,
    tallies: &BTreeMap<&str, Tally>,
    findings: &[Finding],
) -> Vec<Row> {
    let empty = Tally::default();
    source
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .skip(1)
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            let (id, second, status, issue, reason) = (
                *cells.first()?,
                *cells.get(1)?,
                *cells.get(4)?,
                *cells.get(5)?,
                *cells.get(6)?,
            );
            let key = if kind == "track" {
                format!("track-{id}")
            } else {
                id.to_owned()
            };
            let tally = tallies.get(key.as_str()).unwrap_or(&empty);
            let class = match status {
                "node-profile" => "node",
                "operator" => "operator",
                _ => "gateway",
            };
            let (result, run_reason) = result(status, class, tally, id, findings);
            let reason = match run_reason {
                Some(run_reason) => run_reason,
                None => reason.to_owned(),
            };
            let (out_kind, title, actor) = if kind == "track" {
                ("track", second, "-")
            } else {
                (class, "-", second)
            };
            Some(Row {
                kind: out_kind.to_owned(),
                id: id.to_owned(),
                title: title.to_owned(),
                actor: actor.to_owned(),
                status: status.to_owned(),
                result,
                passed: tally.passed,
                failed: tally.failed,
                not_run: tally.not_run,
                issue: issue.to_owned(),
                reason,
            })
        })
        .collect()
}

/// The result of one row and, for a covered row that did not pass, the
/// run's reason in place of the table's.
fn result(
    status: &str,
    class: &str,
    tally: &Tally,
    id: &str,
    findings: &[Finding],
) -> (String, Option<String>) {
    match status {
        "covered" => {
            if tally.failed > 0 {
                ("fail".to_owned(), Some(tally.reasons.join("; ")))
            } else if tally.not_run > 0 {
                ("not-run".to_owned(), Some(tally.reasons.join("; ")))
            } else if tally.passed == 0 {
                ("not-run".to_owned(), Some(UNSCORED.to_owned()))
            } else {
                ("pass".to_owned(), None)
            }
        }
        "deferred" => ("deferred".to_owned(), None),
        _ if class != "gateway" => {
            let verdicts: Vec<&str> = findings
                .iter()
                .filter(|finding| finding.point == id)
                .map(|finding| finding.verdict.as_str())
                .collect();
            let word = if tally.failed > 0 {
                "check-failed".to_owned()
            } else if verdicts.contains(&"fail") {
                format!("{class}-fail")
            } else if verdicts.contains(&"pass") {
                format!("{class}-pass")
            } else if !verdicts.is_empty() {
                format!("{class}-not-observable")
            } else if tally.not_run > 0 {
                "not-run".to_owned()
            } else if tally.passed > 0 {
                "not-reported".to_owned()
            } else if class == "node" {
                "unchecked".to_owned()
            } else {
                "not-applicable".to_owned()
            };
            (word, None)
        }
        _ => ("open".to_owned(), None),
    }
}

/// One cell of a tab-separated row, with no tab or line break.
fn tsv_cell(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// One cell of a Markdown table, its pipes escaped and its line breaks
/// spaces.
fn md_cell(text: &str) -> String {
    text.replace(['\n', '\r'], " ").replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::{Finding, Report, UNSCORED};
    use crate::conformance::catalogue::{CATALOGUE, Entry, Kind};
    use crate::conformance::execute::Outcome;

    fn harness_only() -> Vec<(&'static Entry, Outcome)> {
        CATALOGUE
            .iter()
            .map(|entry| {
                let outcome = match entry.kind {
                    Kind::Harness(reason) => Outcome::NotRun(reason.to_owned()),
                    Kind::Live(_) => Outcome::Pass,
                };
                (entry, outcome)
            })
            .collect()
    }

    #[test]
    fn a_point_only_live_scenarios_score_passes_and_one_a_harness_part_scores_is_not_run() {
        let report = Report::new(&harness_only(), Vec::new(), Vec::new(), "a test");
        let result = |kind: &str, id: &str| report.row(kind, id).map(|row| row.result.clone());
        assert_eq!(Some("pass".to_owned()), result("gateway", "CP-1"));
        assert_eq!(Some("pass".to_owned()), result("gateway", "CP-38"));
        assert_eq!(
            Some("not-run".to_owned()),
            result("gateway", "CP-2"),
            "CP-2's wire part is a harness scenario"
        );
        assert_eq!(Some("not-run".to_owned()), result("track", "1"));
        assert_eq!(Some("deferred".to_owned()), result("track", "8"));
        assert_eq!(Some("deferred".to_owned()), result("gateway", "CP-14"));
        assert_eq!(Some("unchecked".to_owned()), result("node", "CP-18"));
        assert_eq!(
            Some("not-applicable".to_owned()),
            result("operator", "CP-20")
        );
        assert!(!report.failed(), "a not-run scenario is no failure");
        let unscored = report.row("gateway", "CP-29").map(|row| row.reason.clone());
        assert_eq!(
            Some(UNSCORED.to_owned()),
            unscored,
            "no scenario scores CP-29"
        );
    }

    #[test]
    fn a_failed_scenario_fails_its_rows_and_the_run() {
        let mut outcomes = harness_only();
        if let Some((_, outcome)) = outcomes.first_mut() {
            *outcome = Outcome::Fail("the rows differ".to_owned());
        }
        let report = Report::new(&outcomes, Vec::new(), Vec::new(), "a test");
        let row = report.row("gateway", "CP-1").expect("CP-1 is a row");
        assert_eq!(("fail", 1), (row.result.as_str(), row.failed));
        assert!(row.reason.contains("the rows differ"), "{}", row.reason);
        assert!(report.failed());
    }

    #[test]
    fn findings_decide_the_node_and_operator_classes() {
        let finding = |point: &str, verdict: &str| Finding {
            product: "Example CDR 1.0".to_owned(),
            point: point.to_owned(),
            check: "ehr_id exchange".to_owned(),
            verdict: verdict.to_owned(),
            evidence: "seen".to_owned(),
        };
        let findings = vec![
            finding("CP-27", "pass"),
            finding("CP-33a", "fail"),
            finding("CP-33a", "pass"),
        ];
        let report = Report::new(&harness_only(), findings, Vec::new(), "a test");
        let result = |kind: &str, id: &str| report.row(kind, id).map(|row| row.result.clone());
        assert_eq!(Some("node-pass".to_owned()), result("node", "CP-27"));
        assert_eq!(
            Some("operator-fail".to_owned()),
            result("operator", "CP-33a")
        );
        assert!(
            report
                .findings_tsv()
                .contains("Example CDR 1.0\tCP-27\tehr_id exchange\tpass\tseen")
        );
    }

    #[test]
    fn the_findings_render_a_column_per_product_as_the_harness_report_does() {
        let finding = |product: &str, point: &str, verdict: &str| Finding {
            product: product.to_owned(),
            point: point.to_owned(),
            check: "a check".to_owned(),
            verdict: verdict.to_owned(),
            evidence: "seen".to_owned(),
        };
        let findings = vec![
            finding("Other CDR 2.0", "CP-27", "pass"),
            finding("Demo CDR 1.0", "CP-27", "pass"),
            finding("Demo CDR 1.0", "CP-27", "fail"),
            finding("Demo CDR 1.0", "CP-33a", "not-observable"),
        ];
        let markdown = Report::new(&harness_only(), findings, Vec::new(), "a test").to_markdown();
        for line in [
            "| Point | Check | Demo CDR 1.0 | Other CDR 2.0 |",
            "| CP-27 | a check | fail | pass |",
            "| CP-33a | a check | not-observable | - |",
            "### Other CDR 2.0",
            "| CP-27 | a check | fail | seen |",
        ] {
            assert!(
                markdown.lines().any(|candidate| candidate == line),
                "{line}: {markdown}"
            );
        }
    }

    #[test]
    fn the_tsv_has_the_harness_columns_and_one_row_per_track_and_point() {
        let report = Report::new(&harness_only(), Vec::new(), Vec::new(), "a test");
        let tsv = report.to_tsv();
        let mut lines = tsv.lines();
        assert_eq!(Some(super::REPORT_HEADER), lines.next());
        assert!(lines.all(|line| line.split('\t').count() == 11));
        assert_eq!(
            11,
            report
                .rows()
                .iter()
                .filter(|row| row.kind == "track")
                .count()
        );
        let markdown = report.to_markdown();
        assert!(markdown.contains("## Gateway points"), "{markdown}");
        assert!(
            markdown.contains("## Scenarios that did not pass"),
            "{markdown}"
        );
    }
}
