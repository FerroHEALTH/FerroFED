// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission half of track 11 against a FerroEHR candidate, behind the
//! `FERROFED_E2E` gate: the harness, as the federation operator, applies the
//! identifier-integrity conditions of §12b.2 before it admits a node, with
//! the gateway's admission check (§12b.1, §16.3 track 11; N42a; CP-33a).
//!
//! The candidate is a FerroEHR deployed with the `system_id` member A
//! already has. Recorded under that `system_id`, the registry refuses it as a
//! duplicate before anything is checked. Recorded under a `system_id` of its
//! own, the check creates its test EHRs on the candidate, reads back the
//! `system_id` the candidate stamps into each, and fails the condition naming
//! member A, so the operator does not admit it. FerroEHR is a node here,
//! never the oracle: the test does not assume which UUID version it mints.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::EndpointId;
use ferrofed_server::admission::report::{Condition, Verdict};
use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_testkit::containers::{self, API_PATH, NODE_A_SYSTEM_ID, ProxiedNode};
use ferrofed_testkit::{oauth, unreachable};

type TestResult = Result<(), Box<dyn Error>>;

/// The `system_id` the candidate's operator declares for it, which no member
/// holds.
const DECLARED: &str = "cdr-c.example.org";

/// The endpoint the candidate is registered with.
const CANDIDATE: &str = "node-c-pub";

/// The number of test EHRs the check creates.
const COUNT: u8 = 3;

/// The registry of members A and B, which the check never contacts, and the
/// candidate node C at `candidate`, recorded under `system_id`.
fn registry(candidate: &str, system_id: &str) -> String {
    let unreachable = unreachable::BASE;
    format!(
        r#"
[[organisation]]
id = "org-a"

[[organisation]]
id = "org-b"

[[organisation]]
id = "org-c"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "{NODE_A_SYSTEM_ID}"

[[node]]
id = "node-b"
organisation = "org-b"
system_id = "cdr-b.example.org"

[[node]]
id = "node-c"
organisation = "org-c"
system_id = "{system_id}"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "{unreachable}/node-a"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-b-pub"
node = "node-b"
url = "{unreachable}/node-b"
connection_type = "openehr-rest-query"
managing_organisation = "org-b"

[[endpoint]]
id = "{CANDIDATE}"
node = "node-c"
url = "{candidate}"
connection_type = "openehr-rest-query"
managing_organisation = "org-c"
"#
    )
}

/// The federation over `registry`, written into `dir` with a synthetic
/// signing key beside it.
fn federation(dir: &Path, registry: &str) -> Result<Option<Federation>, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let key = dir.join("signing-key.pem");
    std::fs::write(&key, oauth::es384_pem()?)?;
    let quoted = |path: &Path| toml::Value::String(path.display().to_string());
    let text = format!(
        "[registry]\ndocument = {}\n\n[federation]\nper_node_timeout_ms = 20000\noverall_timeout_ms = 25000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[signing]\nkey_file = {}\njwks_uri = \"https://gw.example.org/.well-known/jwks.json\"\n",
        quoted(&document),
        quoted(&key)
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    Ok(Federation::load(&settings)?)
}

/// Whether `error` is the registry's refusal of two nodes sharing
/// [`NODE_A_SYSTEM_ID`], naming member A first and the candidate second.
fn refuses_the_duplicate(error: &FederationError) -> bool {
    let FederationError::Registry { source, .. } = error else {
        return false;
    };
    matches!(
        source.as_ref(),
        LoadError::DuplicateSystemId { system_id, first, second }
            if system_id.as_str() == NODE_A_SYSTEM_ID
                && first.as_str() == "node-a"
                && second.as_str() == "node-c"
    )
}

// conformance: CP-33a track-11
#[tokio::test]
async fn a_candidate_sharing_a_members_system_id_is_reported_and_never_admitted() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let candidate = ProxiedNode::new(containers::ferroehr(NODE_A_SYSTEM_ID).await?).await?;
    let dir = tempfile::tempdir()?;

    let shared = registry(&candidate.api_root(), NODE_A_SYSTEM_ID);
    let refused = federation(dir.path(), &shared)
        .err()
        .ok_or("a registry with a shared system_id does not load")?;
    let refused = refused
        .downcast_ref::<FederationError>()
        .ok_or("the federation refuses the registry")?;
    assert!(
        refuses_the_duplicate(refused),
        "the duplicate system_id is reported with both nodes (§12b.2, N42a): {refused:?}"
    );
    assert!(
        candidate.proxy.journal().is_empty(),
        "nothing is checked against a registry that does not load"
    );

    let declared = registry(&candidate.api_root(), DECLARED);
    let federation = federation(dir.path(), &declared)?.ok_or("a registry is configured")?;
    let report =
        ferrofed_server::admission::check(&federation, &EndpointId::new(CANDIDATE)?, COUNT).await?;

    assert!(
        report.failed(),
        "a failed condition keeps the candidate out (§12b.1): {report}"
    );
    let system_id = report
        .finding(Condition::SystemIdUniqueness)
        .ok_or("a system_id finding")?;
    assert_eq!(
        Verdict::Fail,
        system_id.verdict(),
        "the candidate stamps a member's system_id into its EHRs (§12b.2): {report}"
    );
    let conflicting: Vec<&String> = system_id
        .evidence()
        .iter()
        .filter(|line| line.contains(NODE_A_SYSTEM_ID) && line.contains("node node-a"))
        .collect();
    assert_eq!(
        usize::from(COUNT),
        conflicting.len(),
        "each test EHR names the member whose system_id it carries: {report}"
    );
    assert_eq!(usize::from(COUNT), report.created().len(), "{report}");
    let generation = report
        .finding(Condition::EhrIdGeneration)
        .ok_or("a generation finding")?;
    assert_ne!(
        Verdict::Fail,
        generation.verdict(),
        "FerroEHR issues distinct UUIDs, so the conflict is the system_id alone: {report}"
    );

    let journal = candidate.proxy.journal();
    let create = format!("{API_PATH}/v1/ehr");
    let steps: Vec<(&str, &str)> = journal
        .iter()
        .map(|capture| (capture.method.as_str(), capture.path.as_str()))
        .collect();
    assert_eq!(
        usize::from(COUNT),
        steps
            .iter()
            .filter(|(method, path)| *method == "POST" && *path == create)
            .count(),
        "the candidate is asked to create each test EHR: {steps:?}"
    );
    assert_eq!(
        usize::from(COUNT),
        steps.iter().filter(|(method, _)| *method == "GET").count(),
        "and to read each back: {steps:?}"
    );
    Ok(())
}
