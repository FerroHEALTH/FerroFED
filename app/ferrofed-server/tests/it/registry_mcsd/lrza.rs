// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Dutch addressing of Annex B §B.2 over the harness directory: each
//! member organisation publishes its care provider's URA, the NVI localizer
//! reads its custodian map from the registry the directory gives, so
//! `[nl_gf.nvi]` needs no `custodians` table, and a refresh rebuilds the map
//! (N4, N21, §14.1, §15.1, Annex B §B.1).
//!
//! The patient is the facade's synthetic one, its namespace listed as
//! standing for the pseudonymised BSN, resolved by the development
//! cross-reference; every URA is synthetic.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_server::directory::RefreshOutcome;
use ferrofed_server::federation::error::FederationError;
use ferrofed_server::localization::LocalizationError;
use ferrofed_server::reload::ReloadError;
use ferrofed_testkit::mcsd::{HarnessDirectory, Member};
use ferrofed_testkit::nvi::LocalizationService;
use http::StatusCode;

use super::{Gateway, member};
use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, PATIENT, body, crossref, node_answering, patient_query, post,
    received, statuses,
};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

const URA_SYSTEM: &str = "http://fhir.nl/fhir/NamingSystem/ura";

/// The development configuration reading its registry from the directory at
/// `directory`, localized by the NVI at `nvi`, with `custodians` as the
/// optional `[nl_gf.nvi.custodians]` table.
fn config(directory: &str, nvi: &str, custodians: &str) -> String {
    config_resolving(
        directory,
        nvi,
        custodians,
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )
}

/// The configuration of [`config`], the development cross-reference holding
/// the patient at the members of `rows`.
fn config_resolving(directory: &str, nvi: &str, custodians: &str, rows: &[(&str, &str)]) -> String {
    format!(
        "profile = \"development\"\n\n[registry.mcsd]\nurl = \"{directory}\"\nrefresh_interval_s = 3600\ndeadline_ms = 5000\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"localized\"\nid = \"example-federation\"\n\n[federation.localization]\ntimeout_ms = 1000\n\n[nl_gf.nvi]\nurl = \"{nvi}\"\nnamespaces = [\"{NAMESPACE}\"]\n{custodians}\n{}",
        crossref(rows)
    )
}

/// Publishes `member` with its organisation carrying `ura`, when given.
fn publish(harness: &HarnessDirectory, member: &Member, ura: Option<&str>) -> TestResult {
    let identifiers: Vec<(&str, &str)> = ura.map(|ura| (URA_SYSTEM, ura)).into_iter().collect();
    harness.put_organization(member.organisation_identified_by(&identifiers)?);
    harness.put_endpoint(member.endpoint()?);
    Ok(())
}

/// The endpoint statuses of the patient query through `gateway`, with
/// whether the answer is complete.
async fn asked(gateway: &Gateway) -> Result<(Vec<(String, String)>, bool), Box<dyn Error>> {
    let (status, text) = call(gateway.router(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    let statuses = statuses(&answer)
        .into_iter()
        .map(|(id, status)| (id.to_owned(), status.to_owned()))
        .collect();
    Ok((statuses, answer.meta.federation.complete))
}

fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(id, status)| ((*id).to_owned(), (*status).to_owned()))
        .collect()
}

// conformance: CP-5
#[tokio::test]
async fn the_custodians_come_from_the_directory_without_a_table() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let harness = HarnessDirectory::start().await;
    publish(&harness, &member("a", &a.uri()), Some("ura-test-0001"))?;
    publish(&harness, &member("b", &b.uri()), Some("ura-test-0002"))?;
    let nvi = LocalizationService::start().await;
    nvi.index(PATIENT, "ura-test-0002");
    let gateway = Gateway::boot_from(&config(&harness.base(), &nvi.base(), ""))?;

    let (statuses, complete) = asked(&gateway).await?;
    assert_eq!(
        pairs(&[("node-a-pub", "not-localized"), ("node-b-pub", "active")]),
        statuses,
        "the NVI named org-b's care provider, which operates node B"
    );
    assert!(complete, "§11.4, N37: node A was never in scope");
    assert!(received(&a).await?.is_empty(), "node A is not asked");
    Ok(())
}

#[tokio::test]
async fn a_refresh_rebuilds_the_custodian_map() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let harness = HarnessDirectory::start().await;
    publish(&harness, &member("a", &a.uri()), Some("ura-test-0001"))?;
    publish(&harness, &member("b", &b.uri()), Some("ura-test-0002"))?;
    let nvi = LocalizationService::start().await;
    nvi.index(PATIENT, "ura-test-0009");
    let gateway = Gateway::boot_from(&config(&harness.base(), &nvi.base(), ""))?;
    let (statuses, _) = asked(&gateway).await?;
    assert_eq!(
        pairs(&[
            ("node-a-pub", "not-localized"),
            ("node-b-pub", "not-localized")
        ]),
        statuses,
        "ura-test-0009 is no member's care provider yet"
    );

    harness.put_organization(
        member("b", &b.uri()).organisation_identified_by(&[(URA_SYSTEM, "ura-test-0009")])?,
    );
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(matches!(outcome, RefreshOutcome::Applied(_)), "{outcome:?}");
    let (statuses, _) = asked(&gateway).await?;
    assert_eq!(
        pairs(&[("node-a-pub", "not-localized"), ("node-b-pub", "active")]),
        statuses,
        "org-b now publishes ura-test-0009"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_added_without_a_ura_makes_the_refresh_refused() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let harness = HarnessDirectory::start().await;
    publish(&harness, &member("a", &a.uri()), Some("ura-test-0001"))?;
    let nvi = LocalizationService::start().await;
    nvi.index(PATIENT, "ura-test-0001");
    let gateway = Gateway::boot_from(&config_resolving(
        &harness.base(),
        &nvi.base(),
        "",
        &[("node-a", EHR_A)],
    ))?;
    let before = gateway.addresses()?;

    publish(&harness, &member("b", &b.uri()), None)?;
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(
        matches!(
            &outcome,
            RefreshOutcome::Refused(ReloadError::Federation { source, .. })
                if matches!(
                    source.as_ref(),
                    FederationError::Localization(LocalizationError::Nvi(_))
                )
        ),
        "node B could never be localized, so the change is refused: {outcome:?}"
    );
    assert_eq!(before, gateway.addresses()?, "the running registry stays");
    Ok(())
}

#[test]
fn a_custodians_table_that_disagrees_with_the_directory_refuses_to_boot() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (harness, nvi) = runtime.block_on(async {
        let harness = HarnessDirectory::start().await;
        let nvi = LocalizationService::start().await;
        (harness, nvi)
    });
    publish(
        &harness,
        &member("a", "https://cdr-a.example.org/openehr"),
        Some("ura-test-0001"),
    )?;
    publish(
        &harness,
        &member("b", "https://cdr-b.example.org/openehr"),
        Some("ura-test-0002"),
    )?;
    let table = "\n[nl_gf.nvi.custodians]\n\"ura-test-0001\" = \"node-b\"\n\"ura-test-0002\" = \"node-a\"\n";
    let booted = Gateway::boot_from(&config(&harness.base(), &nvi.base(), table));
    let refusal = booted
        .err()
        .map(|error| format!("{error:?}"))
        .unwrap_or_default();
    assert!(
        refusal.contains("CustodiansDisagree"),
        "the two maps are never merged: {refusal}"
    );
    drop(runtime);
    Ok(())
}
