// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Federation-Node profile run against the harness CDR products, behind
//! the `FERROFED_E2E` gate (§16.2; N26, N27, N34; CP-18, CP-19, CP-27).
//!
//! A gateway cannot prove a node's obligations, so these tests score nothing
//! of the gateway's. Each runs one check of the gateway's node profile
//! (`ferrofed_server::conformance::node_profile`, the checks `conformance run
//! --node-profile` runs) against FerroEHR, the CDR every harness node runs,
//! through a node client as the gateway reaches a member, and writes its
//! finding to the findings directory, which the conformance report shows as
//! the node class. The same checks run against EHRbase, a second product, in
//! [`ehrbase`]. A test fails when the check could not observe what the
//! harness arranged for it, never on the node's verdict: FerroEHR is a node
//! here, never the oracle, and its verdict is the report's to show.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use ferrofed_server::config::Config;
use ferrofed_server::conformance::node_profile::interface::{Arrangement, Interface, Principal};
use ferrofed_server::conformance::node_profile::{Check, Finding, Profile, Verdict, checks};
use ferrofed_server::federation::Federation;
use ferrofed_testkit::containers::images::FERROEHR;
use ferrofed_testkit::containers::restricted::{self, RESTRICTED_ADMIN, RESTRICTED_CLINICIAN};
use ferrofed_testkit::containers::{
    self, HarnessUser, NODE_A_SYSTEM_ID, NODE_B_SYSTEM_ID, ProxiedNode,
};
use ferrofed_testkit::node_profile;
use ferrofed_testkit::pix::PixManager;
use ferrofed_testkit::seed::{
    self, CompositionSeed, DemoComposition, EhrDomain, EhrSeed, PatientId, SeedPlan,
};
use ferrofed_testkit::{oauth, unreachable};
use openehr_its::json::from_canonical_json;
use openehr_its::rest::client::Credentials;
use openehr_its::rest::generated::query::AdhocQueryExecute;
use openehr_rm::v1_2::ehr::ehr::Ehr;
use uuid::Uuid;

mod ehrbase;

type TestResult = Result<(), Box<dyn Error>>;

/// The EHR the invocation check reads.
const EHR: Uuid = Uuid::from_u128(0x9393_9393_9393_4393_8393_0000_0000_0093);

/// The synthetic patient that EHR is recorded for.
const PATIENT: PatientId = PatientId::new(1, 93);

/// The number of test EHRs the admission check creates.
const COUNT: u8 = 3;

/// The endpoint a checked node is registered under.
const CHECKED: &str = "node-a-pub";

/// The product every harness node runs, as the report names it.
fn product() -> String {
    format!("FerroEHR {}", FERROEHR.tag)
}

/// Records `finding` for FerroEHR in the findings file `name`.
fn record(finding: Finding, name: &str) -> TestResult {
    let mut profile = Profile::new(product());
    profile.record(finding);
    profile.write(&node_profile::findings_dir(), &format!("ferroehr-{name}"))?;
    Ok(())
}

/// The federation over the one member at `api_root`, reached with `onward`
/// as its Basic onward credentials when given, written into `dir` with a
/// synthetic signing key beside it.
pub(super) fn member(
    dir: &Path,
    api_root: &str,
    onward: Option<HarnessUser>,
) -> Result<Federation, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        format!(
            "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"{NODE_A_SYSTEM_ID}\"\n\n[[endpoint]]\nid = \"{CHECKED}\"\nnode = \"node-a\"\nurl = \"{api_root}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
        ),
    )?;
    let key = dir.join("signing-key.pem");
    std::fs::write(&key, oauth::es384_pem()?)?;
    let quoted = |path: &Path| toml::Value::String(path.display().to_string());
    let credentials = onward.map_or_else(String::new, |user| {
        format!(
            "\n[credentials.\"{CHECKED}\"]\nuser = \"{}\"\npassword = \"{}\"\n",
            user.user, user.password
        )
    });
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {}\n\n[federation]\nper_node_timeout_ms = 20000\noverall_timeout_ms = 25000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[signing]\nkey_file = {}\njwks_uri = \"https://gw.example.org/.well-known/jwks.json\"\n{credentials}",
        quoted(&document),
        quoted(&key)
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    Federation::load(&settings)?.ok_or_else(|| "a registry is configured".into())
}

/// The interface of the one member at `api_root`, reached with `onward` as
/// its Basic onward credentials when given.
pub(super) fn interface(
    api_root: &str,
    onward: Option<HarnessUser>,
) -> Result<Interface, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let federation = member(dir.path(), api_root, onward)?;
    Ok(Interface::of(&federation, &EndpointId::new(CHECKED)?)?)
}

/// `user` as the principal a node's policy refuses.
pub(super) fn principal(user: HarnessUser) -> Principal {
    Principal::new(
        user.user,
        Arc::new(Credentials::basic(user.user, user.password)),
    )
}

// conformance: CP-27
#[tokio::test]
async fn ferroehr_is_invocable_on_its_local_ehr_id_alone() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = ProxiedNode::new(containers::ferroehr(NODE_A_SYSTEM_ID).await?).await?;
    let plan = SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id: EHR,
            subject: Some(PATIENT),
        }],
        template: true,
        compositions: vec![CompositionSeed {
            ehr_id: EHR,
            composition: DemoComposition::FirstClinic,
        }],
    };
    seed::seed(&node.api_root(), &plan).await?;
    node.proxy.clear_journal();

    let finding = checks::invocable_on_ehr_id(&interface(&node.api_root(), None)?, EHR).await?;
    assert_eq!(4, finding.evidence().len(), "{finding:?}");

    let journal = node.proxy.journal();
    assert_eq!(4, journal.len(), "one request per observation");
    for value in [PATIENT.value(), PATIENT.namespace()] {
        assert!(
            !node.proxy.journal_contains(value.as_bytes()),
            "no request the check sent carries the patient's identifier"
        );
    }
    for capture in &journal {
        assert!(
            capture.contains(EHR.to_string().as_bytes()),
            "each request names the EHR by its ehr_id: {} {}",
            capture.method,
            capture.path
        );
    }
    let mut queries = 0_usize;
    for capture in journal
        .iter()
        .filter(|capture| capture.path.ends_with("/query/aql"))
    {
        let body: AdhocQueryExecute = serde_json::from_slice(&capture.body)?;
        assert!(
            !body.q.contains(&EHR.to_string()),
            "the ehr_id reaches the node as a parameter value, never in the AQL: {}",
            body.q
        );
        assert_eq!(
            Some(EHR.to_string().as_str()),
            body.query_parameters
                .as_ref()
                .and_then(|parameters| parameters.get("ehr_id"))
                .and_then(|value| value.as_str()),
            "the ehr_id is the value of the query's $ehr_id: {}",
            body.q
        );
        queries = queries.saturating_add(1);
    }
    assert_eq!(2, queries, "both scoped queries");
    record(finding, "invocable-on-ehr-id")
}

// conformance: CP-27
#[tokio::test]
async fn ferroehr_is_checked_for_an_ehr_created_without_a_subject() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = containers::ferroehr(NODE_A_SYSTEM_ID).await?;

    let finding = checks::subject_not_required(&interface(&node.api_root(), None)?).await?;
    assert!(!finding.evidence().is_empty(), "{finding:?}");
    record(finding, "subject-not-required")
}

// conformance: CP-18
#[tokio::test]
async fn ferroehr_is_checked_for_passing_its_errors_through() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = containers::ferroehr(NODE_A_SYSTEM_ID).await?;

    let finding = checks::errors_passed_through(&interface(&node.api_root(), None)?).await?;
    assert_eq!(3, finding.evidence().len(), "{finding:?}");
    record(finding, "errors-passed-through")
}

/// Creates an EHR at the restricted node of `api_root` as its administrator
/// and returns its `ehr_id`.
async fn created_by_the_administrator(api_root: &str) -> Result<Uuid, Box<dyn Error>> {
    let answer = reqwest::Client::new()
        .post(format!("{api_root}/v1/ehr"))
        .basic_auth(RESTRICTED_ADMIN.user, Some(RESTRICTED_ADMIN.password))
        .header("Accept", "application/json")
        .header("Prefer", "return=representation")
        .send()
        .await?
        .error_for_status()?;
    let ehr: Ehr = from_canonical_json(&answer.text().await?)?;
    Ok(Uuid::try_parse(ehr.ehr_id.value())?)
}

// conformance: CP-18
#[tokio::test]
async fn ferroehr_is_checked_for_holding_its_own_access_decision() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = restricted::ferroehr_restricted(NODE_A_SYSTEM_ID).await?;
    let ehr_id = created_by_the_administrator(&node.api_root()).await?;
    let arrangement = Arrangement::new(ehr_id, principal(RESTRICTED_CLINICIAN));

    let finding = checks::access_decided_at_node(
        &interface(&node.api_root(), Some(RESTRICTED_ADMIN))?,
        &arrangement,
    )
    .await?;
    assert_ne!(
        Verdict::NotObservable,
        finding.verdict(),
        "the harness arranged a refusal the node shows: {finding:?}"
    );
    record(finding, "access-decided-at-node")
}

// conformance: CP-19
#[tokio::test]
async fn ferroehr_has_no_consent_refusal_arranged_in_the_harness() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let finding = Finding::not_arranged(
        Check::ConsentBeforeRelease,
        "the harness arranges no consent refusal at this product: ITS-REST defines no consent resource, and the harness records no consent decision at the node, so nothing at its interface shows a consent check",
    );
    assert_eq!(Verdict::NotObservable, finding.verdict());
    record(finding, "consent-before-release")
}

/// The registry of member A, which the check never contacts, and the
/// candidate node B at `candidate`.
fn registry(candidate: &str) -> String {
    let unreachable = unreachable::BASE;
    format!(
        r#"
[[organisation]]
id = "org-a"

[[organisation]]
id = "org-b"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "{NODE_A_SYSTEM_ID}"

[[node]]
id = "node-b"
organisation = "org-b"
system_id = "{NODE_B_SYSTEM_ID}"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "{unreachable}/node-a"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-b-pub"
node = "node-b"
url = "{candidate}"
connection_type = "openehr-rest-query"
managing_organisation = "org-b"
"#
    )
}

/// The federation over `registry`, resolving through the harness PIX Manager
/// at `pix`, written into `dir` with a synthetic signing key beside it.
fn federation(dir: &Path, registry: &str, pix: &str) -> Result<Federation, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let key = dir.join("signing-key.pem");
    std::fs::write(&key, oauth::es384_pem()?)?;
    let quoted = |path: &Path| toml::Value::String(path.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[[pixm.manager]]\nurl = \"{pix}\"\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n\n[registry]\ndocument = {}\n\n[federation]\nper_node_timeout_ms = 20000\noverall_timeout_ms = 25000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[signing]\nkey_file = {}\njwks_uri = \"https://gw.example.org/.well-known/jwks.json\"\n",
        EhrDomain::new(1).system(),
        EhrDomain::new(2).system(),
        quoted(&document),
        quoted(&key)
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    Federation::load(&settings)?.ok_or_else(|| "a registry is configured".into())
}

// conformance: CP-27 CP-33a
#[tokio::test]
async fn ferroehr_is_checked_against_the_identifier_integrity_conditions() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let candidate = containers::ferroehr(NODE_B_SYSTEM_ID).await?;
    let pix = PixManager::start().await?;
    let dir = tempfile::tempdir()?;
    let federation = federation(
        dir.path(),
        &registry(&candidate.api_root()),
        &pix.base_url(),
    )?;

    let report =
        ferrofed_server::admission::check(&federation, &EndpointId::new("node-b-pub")?, COUNT)
            .await?;
    assert_eq!(
        usize::from(COUNT),
        report.created().len(),
        "the check reached the node and created its test EHRs: {report}"
    );
    assert_eq!(
        5,
        report.findings().len(),
        "one finding per §12b.2 condition"
    );

    let mut profile = Profile::new(product());
    for finding in report.findings() {
        profile.record(Finding::of_admission(finding));
    }
    profile.write(
        &node_profile::findings_dir(),
        "ferroehr-identifier-integrity",
    )?;
    Ok(())
}
