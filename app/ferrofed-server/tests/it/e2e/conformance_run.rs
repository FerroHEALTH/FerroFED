// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ferrofed conformance run` against the two FerroEHR nodes of the harness,
//! configured as an operator configures a deployment: the development
//! cross-reference naming the patient at both, a stored-query registry, the
//! gateway started in-process from the file, and the run seeding its own
//! fixture through each node's ITS-REST writes.
//!
//! The harness run passes every covered track and point it scores, so the
//! run's report must read `pass` exactly where every scenario scoring a row
//! is one a deployment can run, `not-run` with its reason everywhere else,
//! and never `fail`; a Node or Operator row is never a gateway pass (§16.2,
//! §16.4).

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use ferrofed_server::conformance::catalogue::{CATALOGUE, Kind, Scenario};
use ferrofed_testkit::containers;
use ferrofed_testkit::seed::DemoComposition;

use crate::e2e::{EHR_A, EHR_B, PATIENT, TestResult, dev_resolver, registry_document};
use crate::support::{auth_toml, signing_toml, token};

/// The live scenarios whose precondition the harness does not meet: both
/// members resolve the patient, so none is left for the named-unresolved
/// directive.
const UNMET_IN_THE_HARNESS: [Scenario; 1] = [Scenario::NamedUnresolved];

/// Whether every catalogue entry scoring `token` runs against the harness,
/// so a run's row of it can read `pass`.
fn fully_live(token: &str) -> bool {
    let scoring: Vec<_> = CATALOGUE
        .iter()
        .filter(|entry| entry.tokens.contains(&token))
        .collect();
    !scoring.is_empty()
        && scoring.iter().all(|entry| match entry.kind {
            Kind::Live(scenario) => !UNMET_IN_THE_HARNESS.contains(&scenario),
            Kind::Harness(_) => false,
        })
}

/// The rows of `report.tsv`, by kind and id, as status, result and reason.
fn rows(text: &str) -> BTreeMap<(String, String), (String, String, String)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            Some((
                ((*cells.first()?).to_owned(), (*cells.get(1)?).to_owned()),
                (
                    (*cells.get(4)?).to_owned(),
                    (*cells.get(5)?).to_owned(),
                    (*cells.get(10)?).to_owned(),
                ),
            ))
        })
        .collect()
}

/// Asserts that every covered row passes exactly when the run scores it
/// whole and is otherwise not-run with a reason, every deferred row is
/// deferred, and no Node or Operator row is a gateway pass or fail.
fn every_row_matches_the_harness(rows: &BTreeMap<(String, String), (String, String, String)>) {
    for ((kind, id), (status, result, reason)) in rows {
        match (kind.as_str(), status.as_str()) {
            (_, "covered") => {
                let token = if kind == "track" {
                    format!("track-{id}")
                } else {
                    id.clone()
                };
                let expected = if fully_live(&token) {
                    "pass"
                } else {
                    "not-run"
                };
                assert_eq!(
                    expected, result,
                    "{kind} {id}: the run passes only what it scores whole: {reason}"
                );
                if result != "pass" {
                    assert!(!reason.is_empty() && reason != "-", "{kind} {id} says why");
                }
            }
            (_, "deferred") => assert_eq!("deferred", result, "{kind} {id}"),
            ("node" | "operator", _) => assert!(
                result != "pass" && result != "fail",
                "{kind} {id} is scored against its actor, never as a gateway pass: {result}"
            ),
            _ => {}
        }
    }
}

/// The configuration file of the deployment over `nodes`, written in `dir`.
fn configuration(
    dir: &Path,
    nodes: &containers::TwoNodes,
) -> Result<String, Box<dyn std::error::Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry_document(&nodes.a, &nodes.b, ""))?;
    let quoted = |path: &Path| toml::Value::String(path.display().to_string());
    let text = format!(
        "{}\n\n[server]\nrequest_timeout_ms = 60000\n\n[registry]\ndocument = {}\n\n[federation]\nper_node_timeout_ms = 20000\noverall_timeout_ms = 25000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[stored_queries]\npath = {}\n{}{}",
        dev_resolver(),
        quoted(&document),
        quoted(&dir.join("definitions.redb")),
        signing_toml(),
        auth_toml()?
    );
    let file = dir.join("ferrofed.toml");
    std::fs::write(&file, text)?;
    Ok(file.display().to_string())
}

#[tokio::test]
async fn a_run_against_the_harness_scores_what_the_harness_scores_and_nothing_more() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    let dir = tempfile::tempdir()?;
    let config = configuration(dir.path(), &nodes)?;
    let token_file = dir.path().join("token");
    std::fs::write(&token_file, token()?)?;
    let out = dir.path().join("report");
    let seed_data = DemoComposition::FirstHospital
        .path()
        .parent()
        .ok_or("the demo data has a directory")?
        .to_path_buf();
    let argv: Vec<String> = [
        "ferrofed",
        "--config",
        &config,
        "conformance",
        "run",
        "--allow-writes",
        "--patient-namespace",
        &PATIENT.namespace(),
        "--patient-value",
        &PATIENT.value(),
        "--token-file",
        &token_file.display().to_string(),
        "--seed-data",
        &seed_data.display().to_string(),
        "--out",
        &out.display().to_string(),
        "--node-profile",
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .collect();

    let code = tokio::task::spawn_blocking(move || ferrofed_server::command::run(argv)).await?;
    let report = std::fs::read_to_string(out.join("report.md"))?;
    assert_eq!(
        format!("{:?}", ExitCode::SUCCESS),
        format!("{code:?}"),
        "no scenario failed: {report}"
    );

    let rows = rows(&std::fs::read_to_string(out.join("report.tsv"))?);
    assert_eq!(11, rows.keys().filter(|(kind, _)| kind == "track").count());
    every_row_matches_the_harness(&rows);
    let row = |kind: &str, id: &str| rows.get(&(kind.to_owned(), id.to_owned()));
    for (kind, id) in [
        ("gateway", "CP-1"),
        ("gateway", "CP-3"),
        ("gateway", "CP-38"),
    ] {
        assert_eq!(
            Some("pass"),
            row(kind, id).map(|(_, result, _)| result.as_str()),
            "{id}"
        );
    }
    let track10 = row("track", "10").ok_or("track 10 is reported")?;
    assert!(
        track10.1 == "not-run" && track10.2.contains("wire capture"),
        "track 10 needs node-side capture: {track10:?}"
    );
    let track4 = row("track", "4").ok_or("track 4 is reported")?;
    assert!(
        track4.1 == "not-run" && track4.2.contains("fault"),
        "track 4 needs injected faults: {track4:?}"
    );
    let operator = row("operator", "CP-33a").ok_or("CP-33a is reported")?;
    assert!(
        operator.1.starts_with("operator-"),
        "the admission check's findings score CP-33a: {operator:?}"
    );

    let written = std::fs::read_to_string(out.join("written.tsv"))?;
    for ehr_id in [EHR_A, EHR_B] {
        assert!(
            written.contains(&ehr_id.to_string()),
            "the run records the patient's EHR {ehr_id} it created: {written}"
        );
    }
    for file in ["report.md", "report.tsv", "node-profile.tsv", "written.tsv"] {
        let text = std::fs::read_to_string(out.join(file))?;
        assert!(
            !text.contains(&PATIENT.value()),
            "{file} never prints the patient's identifier"
        );
    }
    Ok(())
}
