// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The static development consent pre-filter: enabled only under the
//! development profile, denying only the candidates its rows name for the
//! patient, and asserting nothing about any other candidate (N27a, §14.3).

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Write as _;
use std::time::Instant;

use ferrofed_identity::consent::{ConsentDecision, ConsentPrefilter};
use ferrofed_identity::dev::{DevCrossRefError, STATIC_CONSENT_MODE, StaticConsentPrefilter};
use ferrofed_identity::patient::{IdentifierNamespace, PatientRef};
use ferrofed_registry::id::NodeId;

use crate::support::{Config, PATIENT_VALUE, config, ready, registry};

type TestResult = Result<(), Box<dyn Error>>;

const EHR_A: &str = "6f2a51a4-1b8e-4f8b-9a4c-1f6c2b1d7e30";

/// A configuration under `profile` with one cross-reference row and one
/// `[[dev.consent_denied]]` row per `(value, member)` in `denials`.
fn with_denials(profile: &str, denials: &[(&str, &str)]) -> Result<Config, Box<dyn Error>> {
    let mut text = config(profile, &[("node-a", EHR_A)]);
    for (value, member) in denials {
        write!(
            text,
            "\n[[dev.consent_denied]]\nnamespace = \"2.999.1\"\nvalue = \"{value}\"\nmember = \"{member}\"\n"
        )?;
    }
    Ok(toml::from_str(&text)?)
}

/// The pre-filter of a development configuration denying `denials`.
fn enabled(denials: &[(&str, &str)]) -> Result<StaticConsentPrefilter, Box<dyn Error>> {
    let config = with_denials("development", denials)?;
    let table = config.dev.ok_or("the [dev] table is present")?;
    let prefilter = StaticConsentPrefilter::from_config(config.profile, &table, &registry())?;
    Ok(prefilter.ok_or("the table has consent rows, so the pre-filter is built")?)
}

/// The refusal `denials` builds into, as an error.
fn refused(profile: &str, denials: &[(&str, &str)]) -> Result<DevCrossRefError, Box<dyn Error>> {
    let config = with_denials(profile, denials)?;
    let table = config.dev.ok_or("the [dev] table is present")?;
    StaticConsentPrefilter::from_config(config.profile, &table, &registry())
        .err()
        .ok_or_else(|| "the rows were accepted".into())
}

fn patient(value: &str) -> Result<PatientRef, Box<dyn Error>> {
    Ok(PatientRef::new(
        IdentifierNamespace::new("2.999.1")?,
        value.into(),
    )?)
}

fn members(ids: &[&str]) -> Result<Vec<NodeId>, Box<dyn Error>> {
    ids.iter().map(|id| Ok(id.parse()?)).collect()
}

/// The members `decision` denies, or `None` for no signal.
fn denied(decision: ConsentDecision) -> Result<Option<BTreeSet<String>>, Box<dyn Error>> {
    match decision {
        ConsentDecision::Denied(set) => Ok(Some(set.iter().map(ToString::to_string).collect())),
        ConsentDecision::NoSignal => Ok(None),
        ConsentDecision::NotAsked(reason) => {
            Err(format!("the table is always consulted, never not asked: {reason:?}").into())
        }
        ConsentDecision::Unavailable(error) | ConsentDecision::Partial { failure: error, .. } => {
            Err(error.into())
        }
    }
}

#[test]
fn consent_rows_are_refused_outside_the_development_profile() -> TestResult {
    assert_eq!(
        refused("production", &[(PATIENT_VALUE, "node-b")])?,
        DevCrossRefError::NotDevelopment
    );
    Ok(())
}

#[test]
fn a_table_with_no_consent_row_builds_no_prefilter() -> TestResult {
    let config = with_denials("development", &[])?;
    let table = config.dev.ok_or("the [dev] table is present")?;
    assert!(StaticConsentPrefilter::from_config(config.profile, &table, &registry())?.is_none());
    Ok(())
}

#[test]
fn a_row_naming_no_member_or_repeated_is_refused() -> TestResult {
    assert_eq!(
        refused("development", &[(PATIENT_VALUE, "node-z")])?,
        DevCrossRefError::UnknownMember("node-z".parse()?)
    );
    assert!(matches!(
        refused(
            "development",
            &[(PATIENT_VALUE, "node-b"), (PATIENT_VALUE, "node-b")]
        )?,
        DevCrossRefError::DuplicateRow { .. }
    ));
    assert!(matches!(
        refused("development", &[("", "node-b")])?,
        DevCrossRefError::EmptyValue(_)
    ));
    Ok(())
}

#[test]
fn only_the_candidates_a_row_names_for_the_patient_are_denied() -> TestResult {
    let prefilter = enabled(&[(PATIENT_VALUE, "node-b")])?;
    assert_eq!(prefilter.mode(), STATIC_CONSENT_MODE);
    let decision = ready(prefilter.prefilter(
        &patient(PATIENT_VALUE)?,
        None,
        &members(&["node-a", "node-b"])?,
        Instant::now(),
    ));
    assert_eq!(
        denied(decision)?,
        Some(BTreeSet::from(["node-b".to_owned()])),
        "node-a is not denied, which asserts nothing about its consent (§14.3)"
    );
    Ok(())
}

#[test]
fn a_member_that_is_not_a_candidate_is_never_denied() -> TestResult {
    let prefilter = enabled(&[(PATIENT_VALUE, "node-b")])?;
    let decision = ready(prefilter.prefilter(
        &patient(PATIENT_VALUE)?,
        None,
        &members(&["node-a"])?,
        Instant::now(),
    ));
    assert_eq!(denied(decision)?, None, "no candidate denied: no signal");
    Ok(())
}

#[test]
fn another_patient_gets_no_signal() -> TestResult {
    let prefilter = enabled(&[(PATIENT_VALUE, "node-b")])?;
    let decision = ready(prefilter.prefilter(
        &patient("67890")?,
        None,
        &members(&["node-a", "node-b"])?,
        Instant::now(),
    ));
    assert_eq!(denied(decision)?, None);
    Ok(())
}

#[test]
fn debug_shows_no_patient_identifier() -> TestResult {
    let prefilter = enabled(&[(PATIENT_VALUE, "node-b")])?;
    let shown = format!("{prefilter:?}");
    assert!(!shown.contains(PATIENT_VALUE), "{shown}");
    let config = with_denials("development", &[(PATIENT_VALUE, "node-b")])?;
    let shown = format!("{:?}", config.dev.ok_or("the [dev] table is present")?);
    assert!(!shown.contains(PATIENT_VALUE), "{shown}");
    assert!(shown.contains("consent_denied_rows: 1"), "{shown}");
    Ok(())
}
