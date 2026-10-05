// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The static development cross-reference: enabled only under the
//! development profile, resolving only the rows it holds, and never showing a
//! patient identifier value.

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use ferrofed_identity::dev::{DevCrossRefError, Profile, StaticResolver};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::localizer::{Localization, Localizer};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_identity::role::resolver::{Resolution, Resolver};
use ferrofed_registry::id::{EhrId, NodeId};

use crate::support::{Config, PATIENT_VALUE, config, ready, registry};

type TestResult = Result<(), Box<dyn Error>>;

const EHR_A: &str = "6f2a51a4-1b8e-4f8b-9a4c-1f6c2b1d7e30";

fn enabled(rows: &[(&str, &str)]) -> Result<StaticResolver, Box<dyn Error>> {
    let config: Config = toml::from_str(&config("development", rows))?;
    let resolver = StaticResolver::from_config(config.profile, config.dev, &registry())?;
    Ok(resolver.ok_or("the table is present, so the resolver is built")?)
}

fn patient(namespace: &str, value: &str) -> Result<PatientRef, Box<dyn Error>> {
    Ok(PatientRef::new(
        IdentifierNamespace::new(namespace)?,
        value.into(),
    )?)
}

fn members(ids: &[&str]) -> Result<Vec<NodeId>, Box<dyn Error>> {
    ids.iter().map(|id| Ok(id.parse()?)).collect()
}

#[test]
fn the_table_is_refused_outside_the_development_profile() -> TestResult {
    let config: Config = toml::from_str(&config("production", &[("node-a", EHR_A)]))?;
    assert_eq!(
        config.profile,
        Profile::Production,
        "the profile as written"
    );
    assert_eq!(
        StaticResolver::from_config(config.profile, config.dev, &registry()).err(),
        Some(DevCrossRefError::NotDevelopment),
        "a production configuration with the table cannot start"
    );
    Ok(())
}

#[test]
fn no_table_means_no_static_resolver_in_any_profile() -> TestResult {
    for profile in ["production", "development"] {
        let config: Config = toml::from_str(&config(profile, &[]))?;
        assert!(
            StaticResolver::from_config(config.profile, config.dev, &registry())?.is_none(),
            "no [dev] table under {profile}"
        );
    }
    Ok(())
}

#[test]
fn an_unknown_profile_is_refused() {
    let parsed: Result<Config, _> = toml::from_str("profile = \"staging\"\n");
    assert!(parsed.is_err(), "only production and development exist");
}

#[test]
fn a_row_resolves_its_member_and_every_other_member_is_unknown() -> TestResult {
    let resolver = enabled(&[("node-a", EHR_A)])?;
    let outcome = ready(resolver.resolve(
        &patient("2.999.1", PATIENT_VALUE)?,
        &members(&["node-a", "node-b"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ));

    assert_eq!(outcome.len(), 2, "one outcome per member asked");
    let node_a: NodeId = "node-a".parse()?;
    let node_b: NodeId = "node-b".parse()?;
    assert!(
        matches!(outcome.get(&node_a), Some(Resolution::Resolved(id)) if *id == EhrId::new(EHR_A)?),
        "node-a holds the patient under its ehr_id"
    );
    assert!(
        matches!(outcome.get(&node_b), Some(Resolution::Unknown)),
        "node-b does not know the patient (N6)"
    );
    Ok(())
}

#[test]
fn a_member_not_asked_gets_no_outcome() -> TestResult {
    let resolver = enabled(&[("node-a", EHR_A)])?;
    let outcome = ready(resolver.resolve(
        &patient("2.999.1", PATIENT_VALUE)?,
        &members(&["node-b"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ));
    let node_a: NodeId = "node-a".parse()?;
    assert!(!outcome.contains_key(&node_a), "node-a was not asked");
    assert_eq!(outcome.len(), 1, "only node-b");
    Ok(())
}

#[test]
fn the_same_value_in_another_namespace_is_another_patient() -> TestResult {
    let resolver = enabled(&[("node-a", EHR_A)])?;
    let outcome = ready(resolver.resolve(
        &patient("2.999.2", PATIENT_VALUE)?,
        &members(&["node-a"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ));
    let node_a: NodeId = "node-a".parse()?;
    assert!(
        matches!(outcome.get(&node_a), Some(Resolution::Unknown)),
        "the namespace is part of the identifier"
    );
    Ok(())
}

#[test]
fn the_resolver_works_behind_the_seam() -> TestResult {
    let resolver: Arc<dyn Resolver> = Arc::new(enabled(&[("node-a", EHR_A)])?);
    let outcome = ready(resolver.resolve(
        &patient("2.999.1", PATIENT_VALUE)?,
        &members(&["node-a"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ));
    assert_eq!(outcome.len(), 1, "one member, one outcome");
    Ok(())
}

#[test]
fn a_row_naming_a_member_outside_the_registry_is_refused() -> TestResult {
    let config: Config = toml::from_str(&config("development", &[("node-z", EHR_A)]))?;
    assert!(
        matches!(
            StaticResolver::from_config(config.profile, config.dev, &registry()),
            Err(DevCrossRefError::UnknownMember(member)) if member.as_str() == "node-z"
        ),
        "every row names a member"
    );
    Ok(())
}

#[test]
fn two_rows_for_one_identifier_at_one_member_are_refused() -> TestResult {
    let ehr_other = "0b6f1d2c-3a4e-4b5f-8c6d-7e8f9a0b1c2d";
    let config: Config = toml::from_str(&config(
        "development",
        &[("node-a", EHR_A), ("node-a", ehr_other)],
    ))?;
    assert!(
        matches!(
            StaticResolver::from_config(config.profile, config.dev, &registry()),
            Err(DevCrossRefError::DuplicateRow { member, .. }) if member.as_str() == "node-a"
        ),
        "one patient, one ehr_id per member"
    );
    Ok(())
}

#[test]
fn a_row_with_an_empty_value_is_refused() -> TestResult {
    let text = config("development", &[("node-a", EHR_A)])
        .replace(&format!("value = \"{PATIENT_VALUE}\""), "value = \"\"");
    let config: Config = toml::from_str(&text)?;
    assert!(
        matches!(
            StaticResolver::from_config(config.profile, config.dev, &registry()),
            Err(DevCrossRefError::EmptyValue(_))
        ),
        "an empty identifier"
    );
    Ok(())
}

#[test]
fn a_row_with_an_unknown_field_is_refused() {
    let text = config("development", &[("node-a", EHR_A)])
        .replace("ehr_id =", "nickname = \"synthetic\"\nehr_id =");
    let parsed: Result<Config, _> = toml::from_str(&text);
    assert!(parsed.is_err(), "deny_unknown_fields on every row");
}

#[test]
fn a_row_with_a_malformed_ehr_id_is_refused() {
    let parsed: Result<Config, _> =
        toml::from_str(&config("development", &[("node-a", "not an ehr id")]));
    assert!(parsed.is_err(), "an ehr_id is a HIER_OBJECT_ID");
}

#[test]
fn no_rendering_shows_a_patient_identifier_value() -> TestResult {
    let config: Config = toml::from_str(&config("development", &[("node-a", EHR_A)]))?;
    let table_debug = format!("{:?}", config.dev);
    let resolver = StaticResolver::from_config(config.profile, config.dev, &registry())?;
    let resolver_debug = format!("{resolver:?}");
    let duplicate = DevCrossRefError::DuplicateRow {
        namespace: IdentifierNamespace::new("2.999.1")?,
        member: "node-a".parse()?,
    };
    for rendered in [table_debug, resolver_debug, duplicate.to_string()] {
        assert!(!rendered.contains(PATIENT_VALUE), "no value in {rendered}");
    }
    Ok(())
}

#[test]
fn as_a_localizer_it_names_the_asked_members_that_hold_a_row() -> TestResult {
    let localizer = enabled(&[("node-a", EHR_A)])?;
    let answer = ready(localizer.localize(
        &patient("2.999.1", PATIENT_VALUE)?,
        &members(&["node-a", "node-b"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ));
    let node_a: NodeId = "node-a".parse()?;
    match answer {
        Localization::Candidates(named) => {
            assert_eq!(BTreeSet::from([node_a]), named, "only node-a holds a row");
            Ok(())
        }
        other => Err(format!("a candidate set (N4, §14.1): {other:?}").into()),
    }
}

#[test]
fn as_a_localizer_it_finds_no_records_for_a_patient_with_no_row() -> TestResult {
    let localizer = enabled(&[("node-a", EHR_A)])?;
    let answer = ready(localizer.localize(
        &patient("2.999.2", PATIENT_VALUE)?,
        &members(&["node-a", "node-b"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ));
    match answer {
        Localization::NoRecords => Ok(()),
        other => Err(format!("no member holds the patient's data: {other:?}").into()),
    }
}
