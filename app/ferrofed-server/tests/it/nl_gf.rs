// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Dutch binding's localizer, `[nl_gf.nvi]`, under the localized
//! selection, and the pseudonymised walkthrough of Annex B §B.7 end to end:
//! the client presents a pseudonym and never a BSN, the NVI names the care
//! providers holding the patient's data (Step 1a, §B.1), the PIX Manager
//! cross-references the pseudonym to each node's `ehr_id` (Step 1c), and each
//! node receives AQL keyed on its own `ehr_id` alone (Step 2, N33, §5.4).
//!
//! In process: three mock CDRs, the stub Localization Service and the harness
//! PIX Manager fed over ITI-104. Every identifier is synthetic, in the
//! `urn:oid:2.999` example arc, which `[nl_gf.nvi] namespaces` lists as
//! standing for the pseudonymised BSN.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::{Config, error, transport};
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_server::localization::{LocalizationError, NL_GF_NVI};
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::nvi::LocalizationService;
use ferrofed_testkit::pix::PixManager;
use ferrofed_testkit::seed::{self, CrossReferenceSeed, EhrDomain, PatientId};
use http::{Request, StatusCode};
use openehr_federation::options::OptionsRoot;
use uuid::Uuid;

use crate::facade::{
    Answer, body, node_answering, post, received, schema, settings_with_room, statuses, wire,
};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

const MEMBERS: [&str; 3] = ["node-a", "node-b", "node-c"];

/// The care provider, by a synthetic URA, whose data each member holds.
const URAS: [&str; 3] = ["ura-test-0001", "ura-test-0002", "ura-test-0003"];

/// The `ehr_id` domains of the three members at the PIX Manager.
const DOMAINS: [EhrDomain; 3] = [EhrDomain::new(21), EhrDomain::new(22), EhrDomain::new(23)];

/// The patient's `ehr_id` at node A and node B, unrelated values (§B.7 Step
/// 1c); node C holds no data for the patient.
const EHR_A: Uuid = Uuid::from_u128(0x7d8e_7d8e_7d8e_4d8e_8d8e_7d8e_7d8e_7a19);
const EHR_B: Uuid = Uuid::from_u128(0xb3c1_b3c1_b3c1_4b3c_8b3c_b3c1_b3c1_05f0);

/// The pseudonymised patient the application presents, in the example arc.
fn patient() -> PatientId {
    PatientId::new(1, 487)
}

/// The façade query of §B.7 Step 0, naming only the pseudonym, with the one
/// column the mock nodes answer.
fn query() -> String {
    let patient = patient();
    format!(
        "SELECT c/uid/value AS composition_id FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{}' \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        patient.value(),
        patient.namespace()
    )
}

/// The gateway over the members at `urls`, localized by the NVI at `nvi`
/// and resolved by the `[pixm]` Manager at `manager`.
fn gateway(
    dir: &Path,
    urls: [&str; 3],
    nvi: &str,
    manager: &str,
) -> Result<Router, Box<dyn Error>> {
    let text = configuration(dir, "development", urls, nvi, manager)?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    Ok(ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &settings_with_room(),
    ))
}

/// The configuration under `profile` of the gateway [`gateway`] builds, with
/// its registry document written into `dir`.
fn configuration(
    dir: &Path,
    profile: &str,
    urls: [&str; 3],
    nvi: &str,
    manager: &str,
) -> Result<String, Box<dyn Error>> {
    let mut registry = String::new();
    let mut members = String::new();
    let mut custodians = String::new();
    for (((member, url), domain), ura) in MEMBERS.into_iter().zip(urls).zip(DOMAINS).zip(URAS) {
        write!(
            registry,
            "\n[[organisation]]\nid = \"org-{member}\"\n\n[[node]]\nid = \"{member}\"\norganisation = \"org-{member}\"\nsystem_id = \"{member}.example.org\"\n\n[[endpoint]]\nid = \"{member}-pub\"\nnode = \"{member}\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-{member}\"\n"
        )?;
        writeln!(members, "\"{member}\" = \"{}\"", domain.system())?;
        writeln!(custodians, "\"{ura}\" = \"{member}\"")?;
    }
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    Ok(format!(
        "profile = \"{profile}\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"localized\"\nid = \"example-federation\"\n\n[federation.localization]\ntimeout_ms = 1000\n\n[nl_gf.nvi]\nurl = \"{nvi}\"\nnamespaces = [\"{namespace}\"]\n\n[nl_gf.nvi.custodians]\n{custodians}\n[audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"{manager}\"\n\n[pixm.manager.members]\n{members}",
        document = toml::Value::String(document.display().to_string()),
        namespace = patient().namespace(),
    ))
}

/// The configuration [`configuration`] writes, with every URL `https`, the
/// localized selection, and `edit` applied to its text.
fn edited(
    dir: &Path,
    profile: &str,
    nvi: &str,
    edit: impl Fn(String) -> String,
) -> Result<String, Box<dyn Error>> {
    let text = configuration(
        dir,
        profile,
        [
            "https://a.example.org",
            "https://b.example.org",
            "https://c.example.org",
        ],
        nvi,
        "https://pix.example.org/fhir/",
    )?;
    Ok(crate::support::signed(&edit(text)))
}

/// The harness PIX Manager, fed with the patient's `ehr_id` at node A and
/// node B.
async fn fed_manager() -> Result<PixManager, Box<dyn Error>> {
    let pix = PixManager::start().await?;
    let feed = CrossReferenceSeed {
        patient: patient(),
        ehrs: vec![(DOMAINS[0], EHR_A), (DOMAINS[1], EHR_B)],
    };
    assert_eq!(
        StatusCode::CREATED,
        seed::feed(&pix.base_url(), &feed).await?,
        "ITI-104 creates the Patient"
    );
    Ok(pix)
}

async fn members() -> [Server; 3] {
    [
        node_answering("a-uid::node-a.example.org::1").await,
        node_answering("b-uid::node-b.example.org::1").await,
        node_answering("c-uid::node-c.example.org::1").await,
    ]
}

async fn asked_counts(servers: &[Server; 3]) -> Result<[usize; 3], Box<dyn Error>> {
    Ok([
        received(&servers[0]).await?.len(),
        received(&servers[1]).await?.len(),
        received(&servers[2]).await?.len(),
    ])
}

// conformance: CP-5 CP-26
#[tokio::test]
async fn the_pseudonymised_walkthrough_runs_end_to_end_and_no_node_sees_the_pseudonym() -> TestResult
{
    let nvi = LocalizationService::start().await;
    nvi.index(&patient().value(), URAS[0]);
    nvi.index(&patient().value(), URAS[1]);
    nvi.index(&patient().value(), "ura-outside-federation");
    let pix = fed_manager().await?;
    let servers = members().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        &nvi.base(),
        &pix.base_url(),
    )?;

    let (status, text) = call(app.clone(), post(body(&query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "not-localized"),
        ],
        statuses(&answer),
        "§B.7 Step 1a: members the NVI did not return are not-localized (§11.1)"
    );
    assert_eq!([1, 1, 0], asked_counts(&servers).await?);

    let searches = nvi.searches().await;
    assert_eq!(1, searches.len(), "one localization per query");
    assert!(
        searches
            .iter()
            .all(|search| search.contains(&patient().value())),
        "the NVI is asked by the pseudonym (§B.1)"
    );

    let pseudonym = patient().value();
    for (server, ehr_id) in servers.iter().zip([Some(EHR_A), Some(EHR_B), None]) {
        let wire = wire(server).await?;
        assert!(
            !wire.contains(&pseudonym),
            "§B.7 Step 2: the pseudonym reached a node (N33, §5.4)"
        );
        assert!(
            !wire.contains(&patient().namespace()),
            "the namespace predicate is consumed with the value (§5.4.2)"
        );
        if let Some(ehr_id) = ehr_id {
            assert!(
                wire.contains(&ehr_id.to_string()),
                "the node is asked by its own ehr_id"
            );
        }
    }

    let request = Request::options("/").body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status);
    schema::validate_options(&text)?;
    let options: OptionsRoot = serde_json::from_str(&text)?;
    assert_eq!("closed", options.federation.localization.on_failure);
    assert_eq!(
        Some(format!("\"{NL_GF_NVI}\"").as_str()),
        options
            .federation
            .localization
            .extra
            .get("mode")
            .map(serde_json::value::RawValue::get)
    );
    Ok(())
}

/// The NVI applies consent itself and never says which holders it dropped,
/// so a member it leaves out is `not-localized`, never `consent-denied`, and
/// with every named member answering the answer stays complete (§14.3,
/// N27a, N37, §11.4).
// conformance: CP-5
#[tokio::test]
async fn a_member_the_nvi_leaves_out_is_not_localized_and_complete_holds() -> TestResult {
    let nvi = LocalizationService::start().await;
    nvi.index(&patient().value(), URAS[0]);
    nvi.index(&patient().value(), URAS[1]);
    let pix = fed_manager().await?;
    let servers = members().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        &nvi.base(),
        &pix.base_url(),
    )?;

    let (status, text) = call(app, post(body(&query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    let left_out = answer
        .meta
        .federation
        .endpoints
        .iter()
        .find(|endpoint| endpoint.id == "node-c-pub")
        .ok_or("node C is reported")?;
    assert_eq!("not-localized", left_out.status, "never consent-denied");
    assert!(left_out.error.is_none(), "the NVI answered: no error");
    assert!(
        answer.meta.federation.complete,
        "N37: every member in scope answered"
    );
    Ok(())
}

// conformance: CP-5
#[tokio::test]
async fn an_nvi_that_does_not_answer_fails_the_query_closed() -> TestResult {
    let nvi = LocalizationService::start().await;
    nvi.index(&patient().value(), URAS[0]);
    nvi.refuse();
    let pix = fed_manager().await?;
    let servers = members().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        &nvi.base(),
        &pix.base_url(),
    )?;

    let (status, text) = call(app, post(body(&query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    for endpoint in &answer.meta.federation.endpoints {
        assert_eq!("not-localized", endpoint.status);
        assert!(
            endpoint.error.is_some(),
            "every member carries the localization error (§14.1)"
        );
    }
    let errors = serde_json::to_string(
        &answer
            .meta
            .federation
            .endpoints
            .iter()
            .map(|endpoint| &endpoint.error)
            .collect::<Vec<_>>(),
    )?;
    assert!(
        !errors.contains(&patient().value()),
        "no error quotes the pseudonym: {errors}"
    );
    assert_eq!(
        [0, 0, 0],
        asked_counts(&servers).await?,
        "no dispatch, no ask-all (§14.1)"
    );
    assert_eq!(0, pix.queries(), "nothing was resolved");
    Ok(())
}

#[tokio::test]
async fn a_patient_in_a_namespace_that_is_no_pseudonym_is_never_sent_to_the_nvi() -> TestResult {
    let nvi = LocalizationService::start().await;
    let pix = fed_manager().await?;
    let servers = members().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        &nvi.base(),
        &pix.base_url(),
    )?;
    let direct = PatientId::new(2, 487);
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{}' \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        direct.value(),
        direct.namespace()
    );
    let (status, text) = call(app, post(body(&aql)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        answer
            .meta
            .federation
            .endpoints
            .iter()
            .all(|endpoint| endpoint.status == "not-localized" && endpoint.error.is_some()),
        "the gateway never pseudonymises, so the localization fails closed (§B.7, §14.1)"
    );
    assert!(nvi.searches().await.is_empty(), "the NVI was never asked");
    assert_eq!([0, 0, 0], asked_counts(&servers).await?);
    Ok(())
}

/// The settings `text` resolves to.
fn resolved(text: &str) -> Result<Settings, error::Error> {
    Config::from_sources(Some(text), &BTreeMap::new())?.resolve()
}

#[test]
fn an_nvi_over_plain_http_is_refused_outside_development() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = edited(
        dir.path(),
        "production",
        "http://nvi.example.org/fhir",
        |text| text,
    )?;
    match resolved(&text) {
        Err(error::Error::Cleartext(refused)) => {
            assert_eq!("nl_gf.nvi.url", refused.site.url_key);
            Ok(())
        }
        other => Err(format!("the pseudonym never travels in clear text: {other:?}").into()),
    }
}

#[test]
fn an_nvi_over_plain_http_is_named_under_development() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = edited(
        dir.path(),
        "development",
        "http://nvi.example.org/fhir",
        |text| text,
    )?;
    let settings = resolved(&text)?;
    let sites: Vec<String> = transport::check(&settings, None)?
        .into_iter()
        .map(|site| format!("{}: {}", site.url_key, site.payload))
        .collect();
    assert!(
        sites.contains(&"nl_gf.nvi.url: the patient identifiers asked of nl_gf.nvi".to_owned()),
        "{sites:?}"
    );
    Ok(())
}

#[test]
fn a_credential_in_the_nvi_url_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = edited(
        dir.path(),
        "production",
        "https://user:Qz7secret@nvi.example.org/fhir",
        |text| text,
    )?;
    match resolved(&text) {
        Err(refused @ error::Error::UrlCredentials { .. }) => {
            assert!(!refused.to_string().contains("Qz7secret"), "{refused}");
            Ok(())
        }
        other => Err(format!("a credential goes in its own section: {other:?}").into()),
    }
}

#[test]
fn the_nvi_under_the_ask_all_selection_refuses_to_boot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = edited(
        dir.path(),
        "production",
        "https://nvi.example.org/fhir",
        |text| {
            text.replace(
                "node_selection = \"localized\"",
                "node_selection = \"ask-all\"",
            )
            .replace("[federation.localization]\ntimeout_ms = 1000\n", "")
        },
    )?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Localization(LocalizationError::NviUnused)) => Ok(()),
        other => Err(format!("a localizer no query uses is refused: {other:?}").into()),
    }
}

#[test]
fn the_nvi_and_xcpd_together_refuse_to_boot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = edited(
        dir.path(),
        "production",
        "https://nvi.example.org/fhir",
        |text| {
            format!(
                "{text}\n[xcpd]\nsender_device = \"2.999.40.1\"\naudit = \"log\"\n\n\
             [[xcpd.gateway]]\nurl = \"https://xcpd.example.org/rg\"\ndevice = \"2.999.50.1\"\n\n\
             [xcpd.communities]\n\"2.999.50\" = \"node-a\"\n\"2.999.60\" = \"node-b\"\n\
             \"2.999.70\" = \"node-c\"\n"
            )
        },
    )?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Localization(LocalizationError::TwoLocalizers)) => Ok(()),
        other => Err(format!("exactly one localizer is active: {other:?}").into()),
    }
}

#[test]
fn a_member_no_custodian_names_refuses_to_boot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = edited(
        dir.path(),
        "production",
        "https://nvi.example.org/fhir",
        |text| text.replace("\"ura-test-0003\" = \"node-c\"\n", ""),
    )?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Localization(LocalizationError::Nvi(_))) => Ok(()),
        other => Err(format!("node-c could never be localized: {other:?}").into()),
    }
}

// conformance: CP-26
#[test]
fn a_bsn_system_listed_as_the_pseudonym_is_refused_at_load() -> TestResult {
    for bsn in [
        "http://fhir.nl/fhir/NamingSystem/bsn",
        "urn:oid:2.16.840.1.113883.2.4.6.3",
        "2.16.840.1.113883.2.4.6.3",
    ] {
        let dir = tempfile::tempdir()?;
        let text = edited(
            dir.path(),
            "production",
            "https://nvi.example.org/fhir",
            |text| text.replace("namespaces = [\"", &format!("namespaces = [\"{bsn}\", \"")),
        )?;
        match resolved(&text) {
            Err(error::Error::BsnAsPseudonym { namespace }) => assert_eq!(namespace, bsn),
            other => {
                return Err(
                    format!("{bsn} never stands for the pseudonym (N33): {other:?}").into(),
                );
            }
        }
    }
    Ok(())
}
