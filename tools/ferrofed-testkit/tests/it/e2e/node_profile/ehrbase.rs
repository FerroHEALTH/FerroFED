// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Federation-Node profile run against EHRbase, the second harness CDR
//! product, behind the `FERROFED_E2E` gate (§16.2; N26, N27, N34; CP-18,
//! CP-19, CP-27).
//!
//! Each test runs the same check its FerroEHR counterpart runs, against the
//! pinned EHRbase image, and writes the finding beside FerroEHR's under the
//! product name EHRbase and its release. A test fails when the check could
//! not observe what the harness arranged for it, never on the node's
//! verdict: EHRbase is a node here, never the oracle. No specification
//! governs which products the harness runs: our own design.

use ferrofed_registry::id::EndpointId;
use ferrofed_testkit::containers::ehrbase::{
    self, RESTRICTED_ADMIN, RESTRICTED_USER, withheld_path,
};
use ferrofed_testkit::containers::{
    self, EHRBASE, NODE_A_SYSTEM_ID, NODE_B_SYSTEM_ID, ProxiedNode,
};
use ferrofed_testkit::node_profile::{
    self, Arrangement, Check, Credential, Finding, Interface, Profile, Verdict, checks,
};
use ferrofed_testkit::pix::PixManager;
use ferrofed_testkit::seed::{
    self, CompositionSeed, DemoComposition, EhrSeed, PatientId, SeedPlan,
};
use openehr_its::json::to_canonical_json;
use uuid::Uuid;

use super::{COUNT, TestResult, federation, integrity, registry};

/// The EHR the invocation check reads.
const EHR: Uuid = Uuid::from_u128(0x9393_9393_9393_4393_8393_0000_0000_0549);

/// The synthetic patient that EHR is recorded for.
const PATIENT: PatientId = PatientId::new(1, 549);

/// The EHR the restricted node withholds from its user role.
const WITHHELD: Uuid = Uuid::from_u128(0x5495_4954_9549_4549_8549_0000_0000_0549);

/// The product, as the report names it.
fn product() -> String {
    format!("EHRbase {}", EHRBASE.tag)
}

/// Records `finding` for EHRbase in the findings file `name`.
fn record(finding: Finding, name: &str) -> TestResult {
    let mut profile = Profile::new(product());
    profile.record(finding);
    profile.write(&node_profile::findings_dir(), &format!("ehrbase-{name}"))?;
    Ok(())
}

// conformance: CP-27
#[tokio::test]
async fn ehrbase_is_invocable_on_its_local_ehr_id_alone() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = ProxiedNode::new(ehrbase::ehrbase(NODE_A_SYSTEM_ID).await?).await?;
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

    let finding = checks::invocable_on_ehr_id(&Interface::new(&node.api_root())?, EHR).await?;
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
    record(finding, "invocable-on-ehr-id")
}

// conformance: CP-27
#[tokio::test]
async fn ehrbase_is_checked_for_an_ehr_created_without_a_subject() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = ehrbase::ehrbase(NODE_A_SYSTEM_ID).await?;

    let finding = checks::subject_not_required(&Interface::new(&node.api_root())?).await?;
    assert!(!finding.evidence().is_empty(), "{finding:?}");
    record(finding, "subject-not-required")
}

// conformance: CP-18
#[tokio::test]
async fn ehrbase_is_checked_for_passing_its_errors_through() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = ehrbase::ehrbase(NODE_A_SYSTEM_ID).await?;

    let finding = checks::errors_passed_through(&Interface::new(&node.api_root())?).await?;
    assert_eq!(3, finding.evidence().len(), "{finding:?}");
    record(finding, "errors-passed-through")
}

/// Creates the EHR `ehr_id`, with no subject, at the restricted node of
/// `api_root` as its administrator.
async fn created_by_the_administrator(api_root: &str, ehr_id: Uuid) -> TestResult {
    reqwest::Client::new()
        .put(format!("{api_root}/v1/ehr/{ehr_id}"))
        .basic_auth(RESTRICTED_ADMIN.user, Some(RESTRICTED_ADMIN.password))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Prefer", "return=minimal")
        .body(to_canonical_json(&seed::ehr_status(None)))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

// conformance: CP-18
#[tokio::test]
async fn ehrbase_is_checked_for_holding_its_own_access_decision() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = ehrbase::ehrbase_restricted(NODE_A_SYSTEM_ID, WITHHELD).await?;
    created_by_the_administrator(&node.api_root(), WITHHELD).await?;
    let arrangement = Arrangement::new(
        WITHHELD,
        Credential::basic(RESTRICTED_ADMIN.user, RESTRICTED_ADMIN.password),
        Credential::basic(RESTRICTED_USER.user, RESTRICTED_USER.password),
    );

    let observed =
        checks::access_decided_at_node(&Interface::new(&node.api_root())?, &arrangement).await?;
    assert_ne!(
        Verdict::NotObservable,
        observed.verdict(),
        "the harness arranged a refusal the node shows: {observed:?}"
    );
    let mut evidence = vec![format!(
        "arranged with EHRbase's security.additionalAuthorizations, its narrowest access control: the request path {} admits the ADMIN role alone, the EHR's content carries no policy",
        withheld_path(WITHHELD)
    )];
    evidence.extend_from_slice(observed.evidence());
    record(
        Finding::new(observed.check(), observed.verdict(), evidence),
        "access-decided-at-node",
    )
}

// conformance: CP-19
#[tokio::test]
async fn ehrbase_has_no_consent_refusal_arranged_in_the_harness() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let finding = Finding::not_arranged(
        Check::ConsentBeforeRelease,
        "the harness arranges no consent refusal at this product: EHRbase records no consent decision, its access controls being authentication (NONE, BASIC, OAUTH) and role rules over request paths, and ITS-REST defines no consent resource, so nothing at its interface shows a consent check",
    );
    assert_eq!(Verdict::NotObservable, finding.verdict());
    record(finding, "consent-before-release")
}

// conformance: CP-27 CP-33a
#[tokio::test]
async fn ehrbase_is_checked_against_the_identifier_integrity_conditions() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let candidate = ehrbase::ehrbase(NODE_B_SYSTEM_ID).await?;
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
        profile.record(integrity(
            finding.condition(),
            finding.verdict(),
            finding.evidence(),
        ));
    }
    profile.write(
        &node_profile::findings_dir(),
        "ehrbase-identifier-integrity",
    )?;
    Ok(())
}
