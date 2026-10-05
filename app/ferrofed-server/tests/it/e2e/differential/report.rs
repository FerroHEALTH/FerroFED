// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The differential report: one Markdown file and one TSV of differences per
//! test, each difference with the verdict the register holds for it.

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use ferrofed_testkit::reference::REFERENCE_COMMIT;

use crate::e2e::differential::observe::NORMALISATIONS;
use crate::e2e::differential::register;
use crate::e2e::differential::{Outcome, Side};

/// Writes `<name>.md` and `<name>.tsv` of `outcomes` under `dir`.
pub(crate) fn write(dir: &Path, name: &str, outcomes: &[Outcome]) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(format!("{name}.md")), markdown(name, outcomes))?;
    std::fs::write(dir.join(format!("{name}.tsv")), tsv(name, outcomes))?;
    Ok(())
}

/// Returns the TSV of every difference: test, step, track, aspect, both
/// values, verdict and where it is recorded.
fn tsv(name: &str, outcomes: &[Outcome]) -> String {
    let mut text = "test\tstep\ttrack\taspect\tferrofed\treference\tverdict\trecorded\n".to_owned();
    for outcome in outcomes {
        for (aspect, ours, theirs) in outcome.differences() {
            let (verdict, recorded) = verdict_of(name, outcome.step.id, &aspect);
            let _written: std::fmt::Result = writeln!(
                text,
                "{name}\t{}\t{}\t{aspect}\t{}\t{}\t{verdict}\t{recorded}",
                outcome.step.id,
                outcome.step.track,
                cell(ours.as_deref()),
                cell(theirs.as_deref()),
            );
        }
    }
    text
}

/// Returns the verdict and the record of a difference, or `unadjudicated`.
fn verdict_of(name: &str, step: &str, aspect: &str) -> (String, String) {
    register::find(name, step, aspect).map_or_else(
        || ("unadjudicated".to_owned(), String::new()),
        |entry| (entry.verdict.to_string(), entry.recorded.to_owned()),
    )
}

/// Returns `value` on one line, `(absent)` for none.
fn cell(value: Option<&str>) -> String {
    value.map_or_else(
        || "(absent)".to_owned(),
        |value| value.replace(['\t', '\n', '\r'], " ").replace('|', "\\|"),
    )
}

/// Returns the Markdown report.
fn markdown(name: &str, outcomes: &[Outcome]) -> String {
    let mut text = String::new();
    let mut line = |row: String| {
        text.push_str(&row);
        text.push('\n');
    };
    line(format!("# Differential run: {name}"));
    line(String::new());
    line(format!(
        "FerroFED and the Federation Tier reference implementation \
         (`syntaric/openehr-federation-ref` at `{REFERENCE_COMMIT}`) over the same two \
         FerroEHR nodes. The reference implementation is evidence, never an oracle: each \
         difference is adjudicated against the specification."
    ));
    line(String::new());
    line("## Normalisations".to_owned());
    line(String::new());
    for (what, why) in NORMALISATIONS {
        line(format!("- **{what}:** {why}"));
    }
    line(String::new());
    line("## Adjudicated causes".to_owned());
    line(String::new());
    for entry in register::REGISTER.iter().filter(|entry| entry.test == name) {
        line(format!(
            "- **{}** ({}): {} Steps: {}. Aspects: {}.",
            entry.verdict,
            entry.recorded,
            entry.cause,
            entry.steps.join(", "),
            entry.aspects.join(", ")
        ));
    }
    line(String::new());
    line("## Summary".to_owned());
    line(String::new());
    line("| Step | Track | FerroFED | Reference | Differences |".to_owned());
    line("|---|---|---|---|---|".to_owned());
    for outcome in outcomes {
        line(format!(
            "| `{}` | {} | {} | {} | {} |",
            outcome.step.id,
            outcome.step.track,
            outcome.ferrofed.reply.status.as_u16(),
            outcome.reference.reply.status.as_u16(),
            outcome.differences().len()
        ));
    }
    for outcome in outcomes {
        line(String::new());
        line(format!(
            "## `{}` (track {}): {}",
            outcome.step.id, outcome.step.track, outcome.step.what
        ));
        line(String::new());
        line(format!(
            "Request: `{} {}`",
            outcome.step.call.method, outcome.step.call.target
        ));
        for (field, value) in &outcome.step.call.fields {
            line(format!("- header `{field}: {value}`"));
        }
        if let Some(body) = &outcome.step.call.body {
            line(format!("- body `{}`", cell(Some(body))));
        }
        if let Some((node, fault)) = &outcome.step.fault {
            line(format!("- fault at node {node:?}: `{fault:?}`"));
        }
        line(String::new());
        let differences = outcome.differences();
        if differences.is_empty() {
            line("No difference.".to_owned());
        } else {
            line("| Aspect | FerroFED | Reference | Verdict |".to_owned());
            line("|---|---|---|---|".to_owned());
            for (aspect, ours, theirs) in &differences {
                let (verdict, recorded) = verdict_of(name, outcome.step.id, aspect);
                line(format!(
                    "| `{aspect}` | {} | {} | {verdict} {recorded} |",
                    cell(ours.as_deref()),
                    cell(theirs.as_deref())
                ));
            }
        }
        line(String::new());
        for (label, side) in [
            ("FerroFED", &outcome.ferrofed),
            ("Reference", &outcome.reference),
        ] {
            details(&mut line, label, side);
        }
    }
    text
}

/// Writes the notes the comparison does not read for one side.
fn details(line: &mut impl FnMut(String), label: &str, side: &Side) {
    if side.observed.info.is_empty() {
        return;
    }
    line(format!("<details><summary>{label}: notes</summary>"));
    line(String::new());
    for (key, value) in &side.observed.info {
        line(format!("- `{key}`:"));
        line(String::new());
        line("  ```text".to_owned());
        for row in value.lines() {
            line(format!("  {row}"));
        }
        line("  ```".to_owned());
    }
    line(String::new());
    line("</details>".to_owned());
    line(String::new());
}
