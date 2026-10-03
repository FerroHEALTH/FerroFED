// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission check against a FerroEHR node behind its capturing proxy:
//! the test EHRs are created and read back over ITS-REST alone, the node
//! reports the `system_id` the registry records, and the proxy journal shows
//! that the only subjects sent were fresh synthetic ones, in the `EHR_STATUS`
//! body and nowhere else (§12b.1, §12b.2, §5.4.1; N33, N42a).
//!
//! FerroEHR is a node here, never the oracle: the test asserts what the
//! check reports about it, and does not assume which UUID version it mints.

use ferrofed_registry::id::EndpointId;
use ferrofed_server::admission::report::{Condition, Verdict};
use ferrofed_server::admission::subject::VALUE_PREFIX;
use ferrofed_testkit::containers::{self, API_PATH};

use crate::e2e::{TestResult, federation_resolving};

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
                !other.headers.iter().any(|(_, value)| value
                    .windows(subject.len())
                    .any(|window| window == subject.as_bytes())),
                "a subject travelled in a header (§5.4.1, N33)"
            );
        }
    }
    Ok(())
}
