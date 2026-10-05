// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NVI localizer against the stub Localization Service: the custodians
//! the service returns become the members that hold their data, and a
//! service that fails fails the localization closed (N4, §14.1,
//! Annex B §B.1).

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_identity::fhir::{Authentication, Tls};
use ferrofed_identity::nl::nvi::{NviConfig, NviConfigError, NviLocalizer};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::localizer::{Localization, Localizer, LocalizerError};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::nvi::{LocalizationService, PSEUDO_BSN_SYSTEM};
use http::{HeaderMap, Method, StatusCode};
use nl_generic_functions::nvi::authorizer::{Authorized, Authorizer, AuthorizerError, Retry};
use secrecy::SecretString;
use url::Url;

use crate::support::registry;

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic pseudonym of the fixtures.
const PSEUDONYM: &str = "pbsn-synthetic-0001";

fn node(id: &str) -> Result<NodeId, Box<dyn Error>> {
    Ok(id.parse()?)
}

fn members() -> Result<Vec<NodeId>, Box<dyn Error>> {
    Ok(vec![node("node-a")?, node("node-b")?])
}

fn patient(namespace: &str, value: &str) -> Result<PatientRef, Box<dyn Error>> {
    Ok(PatientRef::new(
        IdentifierNamespace::new(namespace)?,
        value.into(),
    )?)
}

/// The configuration over the service at `base`: `ura-test-0001` holds its
/// data at `node-a`, and `ura-test-0002` and `ura-test-0003` at `node-b`.
fn config(base: &str) -> Result<NviConfig, Box<dyn Error>> {
    Ok(NviConfig {
        base: SecretUrl::new(base),
        auth: Authentication::None,
        authorizer: None,
        custodians: BTreeMap::from([
            ("ura-test-0001".to_owned(), node("node-a")?),
            ("ura-test-0002".to_owned(), node("node-b")?),
            ("ura-test-0003".to_owned(), node("node-b")?),
        ]),
        namespaces: BTreeSet::from([IdentifierNamespace::new("pseudo-bsn")?]),
        tls: Tls::default(),
    })
}

async fn localize(
    service: &LocalizationService,
    patient: &PatientRef,
    budget: Duration,
) -> Result<Localization, Box<dyn Error>> {
    let localizer = NviLocalizer::from_config(config(&service.base())?, &registry())?;
    Ok(localizer
        .localize(
            patient,
            &members()?,
            &OnBehalfOf::Gateway,
            Instant::now() + budget,
        )
        .await)
}

fn candidates(localization: &Localization) -> Result<Vec<String>, Box<dyn Error>> {
    match localization {
        Localization::Candidates(found) => Ok(found.iter().map(ToString::to_string).collect()),
        other => Err(format!("candidates, not {other:?}").into()),
    }
}

// conformance: CP-5
#[tokio::test]
async fn the_members_holding_the_returned_custodians_are_the_candidates() -> TestResult {
    let service = LocalizationService::start().await;
    service.index(PSEUDONYM, "ura-test-0002");
    service.index(PSEUDONYM, "ura-test-0003");
    let localization = localize(
        &service,
        &patient(PSEUDO_BSN_SYSTEM, PSEUDONYM)?,
        Duration::from_secs(2),
    )
    .await?;
    assert_eq!(candidates(&localization)?, ["node-b"]);
    Ok(())
}

#[tokio::test]
async fn a_namespace_the_configuration_lists_stands_for_the_pseudonym() -> TestResult {
    let service = LocalizationService::start().await;
    service.index(PSEUDONYM, "ura-test-0001");
    let localization = localize(
        &service,
        &patient("pseudo-bsn", PSEUDONYM)?,
        Duration::from_secs(2),
    )
    .await?;
    assert_eq!(candidates(&localization)?, ["node-a"]);
    let searches = service.searches().await;
    assert_eq!(searches.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_patient_no_custodian_holds_has_no_records() -> TestResult {
    let service = LocalizationService::start().await;
    service.index(PSEUDONYM, "ura-outside-federation");
    let localization = localize(
        &service,
        &patient(PSEUDO_BSN_SYSTEM, PSEUDONYM)?,
        Duration::from_secs(2),
    )
    .await?;
    assert!(
        matches!(localization, Localization::NoRecords),
        "a provider outside the federation names no member: {localization:?}"
    );
    Ok(())
}

// conformance: CP-5
#[tokio::test]
async fn a_refusing_service_fails_the_localization_closed_with_its_status() -> TestResult {
    let service = LocalizationService::start().await;
    service.refuse();
    let localization = localize(
        &service,
        &patient(PSEUDO_BSN_SYSTEM, PSEUDONYM)?,
        Duration::from_secs(2),
    )
    .await?;
    match localization {
        Localization::Unavailable(error) => {
            assert_eq!(error.status(), Some(StatusCode::SERVICE_UNAVAILABLE));
        }
        other => panic!("unavailable, not {other:?}"),
    }
    Ok(())
}

// conformance: CP-5
#[tokio::test]
async fn a_silent_service_runs_out_the_budget() -> TestResult {
    let service = LocalizationService::start().await;
    service.go_silent();
    let localization = localize(
        &service,
        &patient(PSEUDO_BSN_SYSTEM, PSEUDONYM)?,
        Duration::from_millis(200),
    )
    .await?;
    assert!(
        matches!(
            localization,
            Localization::Unavailable(LocalizerError::DeadlineExceeded)
        ),
        "{localization:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_named_by_anything_but_the_pseudonym_is_never_sent() -> TestResult {
    let service = LocalizationService::start().await;
    let localization = localize(
        &service,
        &patient("urn:oid:2.999.1", "12345")?,
        Duration::from_secs(2),
    )
    .await?;
    let Localization::Unavailable(error) = localization else {
        panic!("unavailable, not {localization:?}");
    };
    let text = format!("{error} {error:?}");
    assert!(!text.contains("12345"), "{text}");
    assert!(service.searches().await.is_empty(), "nothing was asked");
    Ok(())
}

#[test]
fn a_member_no_custodian_maps_to_is_refused() -> TestResult {
    let mut written = config("https://nvi.example.org/fhir")?;
    written
        .custodians
        .retain(|_ura, member| member.as_str() != "node-b");
    match NviLocalizer::from_config(written, &registry()) {
        Err(NviConfigError::UnlocatedMember(member)) => assert_eq!(member.as_str(), "node-b"),
        other => panic!("an unlocated member, not {other:?}"),
    }
    Ok(())
}

#[test]
fn a_custodian_of_an_unknown_member_is_refused() -> TestResult {
    let mut written = config("https://nvi.example.org/fhir")?;
    written
        .custodians
        .insert("ura-test-0009".to_owned(), node("node-z")?);
    assert!(matches!(
        NviLocalizer::from_config(written, &registry()),
        Err(NviConfigError::UnknownMember { .. })
    ));
    Ok(())
}

#[test]
fn a_base_that_is_no_fhir_base_is_refused() -> TestResult {
    let written = config("https://nvi.example.org/fhir?x=1")?;
    assert!(matches!(
        NviLocalizer::from_config(written, &registry()),
        Err(NviConfigError::Base(_))
    ));
    Ok(())
}

// conformance: CP-26
#[test]
fn a_bsn_system_never_stands_for_the_pseudonym() -> TestResult {
    for bsn in [
        "http://fhir.nl/fhir/NamingSystem/bsn",
        "urn:oid:2.16.840.1.113883.2.4.6.3",
        "2.16.840.1.113883.2.4.6.3",
    ] {
        let mut written = config("https://nvi.example.org/fhir")?;
        written.namespaces.insert(IdentifierNamespace::new(bsn)?);
        match NviLocalizer::from_config(written, &registry()) {
            Err(NviConfigError::BsnAsPseudonym(namespace)) => assert_eq!(namespace.as_str(), bsn),
            other => panic!("{bsn} is refused as an alias of the pseudonym, not {other:?}"),
        }
    }
    Ok(())
}

/// An authorizer that makes no headers; the configuration check refuses it
/// before any request.
#[derive(Debug)]
struct Unused;

impl Authorizer for Unused {
    fn authorize<'a>(&'a self, _method: &'a Method, _url: &'a Url) -> Authorized<'a> {
        Box::pin(async { Err(AuthorizerError::new(std::fmt::Error)) })
    }

    fn answered(&self, _url: &Url, _status: StatusCode, _headers: &HeaderMap) -> Retry {
        Retry::Done
    }
}

#[test]
fn a_fixed_credential_beside_an_authorizer_is_refused() -> TestResult {
    let mut written = config("https://nvi.example.org/fhir")?;
    written.auth = Authentication::Bearer(SecretString::from("synthetic-token"));
    written.authorizer = Some(Arc::new(Unused));
    match NviLocalizer::from_config(written, &registry()) {
        Err(NviConfigError::TwoCredentials) => Ok(()),
        other => Err(format!("a request would carry two credentials: {other:?}").into()),
    }
}
