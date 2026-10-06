// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission check against a FerroEHR node behind its capturing proxy:
//! the test EHRs are created and read back over ITS-REST alone, the node
//! reports the `system_id` the registry records, and the proxy journal shows
//! that the only subjects sent were fresh synthetic ones, in the `EHR_STATUS`
//! body and nowhere else, the claims of each conveyed `openEHR-federation-client`
//! token included (§12b.1, §12b.2, §5.4.1, §13.1; N24, N33, N42a). A run
//! without writes sends the node one query of its existing EHRs and nothing
//! else.
//!
//! FerroEHR is a node here, never the oracle: the test asserts what the
//! check reports about it, and does not assume which UUID version it mints.

use ferrofed_engine::conveyance;
use ferrofed_registry::id::EndpointId;
use ferrofed_server::admission::report::{Condition, Mode, Verdict};
use ferrofed_server::admission::subject::VALUE_PREFIX;
use ferrofed_testkit::containers::{self, API_PATH};
use ferrofed_testkit::seed::{self, DemoComposition};

use crate::e2e::{TestResult, federation_resolving};
use crate::support::searched_claims;

/// Whether the header `name` with `value` carries `needle`: in its raw
/// bytes, or for the gateway's `openEHR-federation-client` token only in the
/// claims a node decodes from it ([`searched_claims`]), so a short needle
/// never matches the token's signature bytes; a token that does not decode
/// counts as carrying it.
fn header_carries(name: &str, value: &[u8], needle: &str) -> bool {
    let found = |haystack: &[u8]| {
        haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    };
    if !name.eq_ignore_ascii_case(conveyance::HEADER) {
        return found(value);
    }
    std::str::from_utf8(value)
        .ok()
        .and_then(|token| searched_claims(token).ok())
        .is_none_or(|claims| found(claims.as_bytes()))
}

#[tokio::test]
async fn the_check_reaches_a_ferroehr_node_with_synthetic_subjects_only() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let dir = tempfile::tempdir()?;
    let federation = federation_resolving(dir.path(), &nodes.a, &nodes.b, "")?;

    let report =
        ferrofed_server::admission::check(&federation, &EndpointId::new("node-a-pub")?, 3).await?;

    assert_eq!(3, report.created().len(), "{report}");
    let generation = report
        .finding(Condition::EhrIdGeneration)
        .ok_or("a finding")?;
    assert_ne!(Verdict::Fail, generation.verdict(), "{report}");
    let system_id = report
        .finding(Condition::SystemIdUniqueness)
        .ok_or("a finding")?;
    assert_eq!(Verdict::Pass, system_id.verdict(), "{report}");

    let journal = nodes.a.proxy.journal();
    let steps: Vec<(&str, &str)> = journal
        .iter()
        .map(|capture| (capture.method.as_str(), capture.path.as_str()))
        .collect();
    let create = format!("{API_PATH}/v1/ehr");
    assert_eq!(
        3,
        steps
            .iter()
            .filter(|(method, path)| *method == "POST" && *path == create)
            .count(),
        "three creates: {steps:?}"
    );
    assert_eq!(
        3,
        steps.iter().filter(|(method, _)| *method == "GET").count(),
        "three reads: {steps:?}"
    );
    assert!(
        nodes.b.proxy.journal().is_empty(),
        "no other member is contacted"
    );

    let text = report.to_string();
    for capture in journal.iter().filter(|capture| capture.method == "POST") {
        let body = String::from_utf8_lossy(&capture.body);
        let start = body
            .find(VALUE_PREFIX)
            .ok_or("a synthetic subject in the body")?;
        let subject: String = body
            .get(start..)
            .ok_or("the subject")?
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        assert!(subject.len() > VALUE_PREFIX.len(), "{subject}");
        assert!(!text.contains(&subject), "the report prints a subject");
        for other in &journal {
            assert!(
                !other.path.contains(&subject)
                    && !other
                        .query
                        .as_deref()
                        .unwrap_or_default()
                        .contains(&subject),
                "a subject travelled in the request target (§5.4.1, N33)"
            );
            assert!(
                !other
                    .headers
                    .iter()
                    .any(|(name, value)| header_carries(name, value, &subject)),
                "a subject travelled in a header or a conveyed claim (§5.4.1, N33)"
            );
        }
    }
    for capture in &journal {
        let conveyed: Vec<&[u8]> = capture
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(conveyance::HEADER))
            .map(|(_, value)| value.as_slice())
            .collect();
        let [token] = conveyed.as_slice() else {
            return Err(format!(
                "{} carries {} conveyed tokens",
                capture.path,
                conveyed.len()
            )
            .into());
        };
        let claims = searched_claims(std::str::from_utf8(token)?)?;
        assert!(
            !claims.contains(VALUE_PREFIX),
            "no synthetic subject in the conveyed claims (§5.4.1, N33): {claims}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_run_without_writes_reads_a_ferroehr_node_and_writes_nothing() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &crate::e2e::plan(crate::e2e::EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let dir = tempfile::tempdir()?;
    let federation = federation_resolving(dir.path(), &nodes.a, &nodes.b, "")?;

    let report = ferrofed_server::admission::read_only::check(
        &federation,
        &EndpointId::new("node-a-pub")?,
        3,
    )
    .await?;

    assert_eq!(Mode::ReadOnly, report.mode());
    assert_eq!(
        vec![crate::e2e::EHR_A.to_string()],
        report.read(),
        "the run reads the one EHR the node holds: {report}"
    );
    assert_eq!(
        Verdict::Pass,
        report
            .finding(Condition::SystemIdUniqueness)
            .ok_or("a finding")?
            .verdict(),
        "{report}"
    );
    assert!(
        report.unproven().contains(&Condition::EhrIdExchange),
        "{report}"
    );
    let journal = nodes.a.proxy.journal();
    let steps: Vec<(&str, &str)> = journal
        .iter()
        .map(|capture| (capture.method.as_str(), capture.path.as_str()))
        .collect();
    assert_eq!(
        vec![("POST", format!("{API_PATH}/v1/query/aql").as_str())],
        steps,
        "one query and no write reach the node"
    );
    assert!(
        nodes.b.proxy.journal().is_empty(),
        "no other member is contacted"
    );
    Ok(())
}
