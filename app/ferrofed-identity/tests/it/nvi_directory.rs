// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NVI localizer over a registry a directory gave: each member
//! organisation's URA, read by the LRZa rules (Annex B §B.2), maps the NVI's
//! custodians to the members that organisation operates, so the configured
//! custodian map is optional; when both are given they must agree, and every
//! member still needs a URA (N4, §14.1, Annex B §B.1).
//!
//! The registry is the synthetic FHIR-form federation of the directory tests:
//! `org-a` operates `node-a` and `org-region` operates `node-b`.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::time::{Duration, Instant};

use ferrofed_identity::fhir::{Authentication, Tls};
use ferrofed_identity::ihe::mcsd;
use ferrofed_identity::nl::nvi::{NviConfig, NviConfigError, NviLocalizer};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::localizer::{Localization, Localizer};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::nvi::{LocalizationService, PSEUDO_BSN_SYSTEM};
use serde_json::json;

use crate::mcsd::{ORG_A, ORG_REGION, fhir, resource};

type TestResult = Result<(), Box<dyn Error>>;

const URA_SYSTEM: &str = "http://fhir.nl/fhir/NamingSystem/ura";

/// The synthetic pseudonym of the fixtures.
const PSEUDONYM: &str = "pbsn-synthetic-0002";

/// The registry where `org-a` carries the URAs `a` and `org-region` the URAs
/// `region`, beside their registry ids.
fn registry(a: &[&str], region: &[&str]) -> Result<RegistrySnapshot, Box<dyn Error>> {
    let mut bundle = fhir();
    for (index, id, uras) in [(ORG_A, "org-a", a), (ORG_REGION, "org-region", region)] {
        let mut identifiers = vec![json!({
            "system": "https://ferrofed.eu/fhir/sid/organisation-id",
            "value": id
        })];
        identifiers.extend(
            uras.iter()
                .map(|ura| json!({"system": URA_SYSTEM, "value": ura})),
        );
        resource(&mut bundle, index)["identifier"] = identifiers.into();
    }
    Ok(mcsd::snapshot_from_json(bundle.to_string().as_bytes())?)
}

fn node(id: &str) -> Result<NodeId, Box<dyn Error>> {
    Ok(id.parse()?)
}

/// The localizer configuration over the service at `base`, with the
/// configured `custodians`.
fn config(base: &str, custodians: &[(&str, &str)]) -> Result<NviConfig, Box<dyn Error>> {
    let mut written = BTreeMap::new();
    for (ura, member) in custodians {
        written.insert((*ura).to_owned(), node(member)?);
    }
    Ok(NviConfig {
        base: SecretUrl::new(base),
        auth: Authentication::None,
        authorizer: None,
        custodians: written,
        namespaces: BTreeSet::new(),
        tls: Tls::default(),
    })
}

// conformance: CP-5
#[tokio::test]
async fn the_directory_maps_each_custodian_to_its_members() -> TestResult {
    let service = LocalizationService::start().await;
    service.index(PSEUDONYM, "ura-test-0002");
    let localizer = NviLocalizer::from_config(
        config(&service.base(), &[])?,
        &registry(&["ura-test-0001"], &["ura-test-0002"])?,
    )?;
    let patient = PatientRef::new(
        IdentifierNamespace::new(PSEUDO_BSN_SYSTEM)?,
        PSEUDONYM.into(),
    )?;
    let localization = localizer
        .localize(
            &patient,
            &[node("node-a")?, node("node-b")?],
            &OnBehalfOf::Gateway,
            Instant::now() + Duration::from_secs(2),
        )
        .await;
    match localization {
        Localization::Candidates(found) => {
            assert_eq!(BTreeSet::from([node("node-b")?]), found);
        }
        other => return Err(format!("candidates, not {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_configured_map_that_agrees_with_the_directory_is_accepted() -> TestResult {
    NviLocalizer::from_config(
        config(
            "https://nvi.example.org/fhir",
            &[("ura-test-0001", "node-a"), ("ura-test-0002", "node-b")],
        )?,
        &registry(&["ura-test-0001"], &["ura-test-0002"])?,
    )?;
    Ok(())
}

#[test]
fn a_configured_map_that_disagrees_with_the_directory_is_refused() -> TestResult {
    for custodians in [
        &[("ura-test-0001", "node-b"), ("ura-test-0002", "node-b")][..],
        &[("ura-test-0001", "node-a")][..],
        &[
            ("ura-test-0001", "node-a"),
            ("ura-test-0002", "node-b"),
            ("ura-test-0003", "node-b"),
        ][..],
    ] {
        match NviLocalizer::from_config(
            config("https://nvi.example.org/fhir", custodians)?,
            &registry(&["ura-test-0001"], &["ura-test-0002"])?,
        ) {
            Err(NviConfigError::CustodiansDisagree(_)) => {}
            other => {
                return Err(
                    format!("the two maps are never merged ({custodians:?}): {other:?}").into(),
                );
            }
        }
    }
    Ok(())
}

#[test]
fn an_organisation_with_two_uras_is_refused() -> TestResult {
    match NviLocalizer::from_config(
        config("https://nvi.example.org/fhir", &[])?,
        &registry(&["ura-test-0001", "ura-test-0009"], &["ura-test-0002"])?,
    ) {
        Err(NviConfigError::Directory { organisation, .. }) => {
            assert_eq!("org-a", organisation.as_str());
            Ok(())
        }
        other => Err(format!("two URAs name no one care provider: {other:?}").into()),
    }
}

#[test]
fn a_member_whose_organisation_has_no_ura_is_unlocated() -> TestResult {
    match NviLocalizer::from_config(
        config("https://nvi.example.org/fhir", &[])?,
        &registry(&["ura-test-0001"], &[])?,
    ) {
        Err(NviConfigError::UnlocatedMember(member)) => {
            assert_eq!("node-b", member.as_str());
            Ok(())
        }
        other => Err(format!("node-b could never be localized: {other:?}").into()),
    }
}
