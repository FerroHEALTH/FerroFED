// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fixed cross-reference of the offline suites: read out of a test's
//! configuration text, resolving and localizing only the rows it holds,
//! denying consent only where a row says so, and never showing a patient
//! identifier value.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Write as _;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::consent::{ConsentDecision, ConsentPrefilter};
use ferrofed_identity::role::localizer::{Localization, Localizer};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_identity::role::resolver::{Identification, Resolution, Resolver};
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::crossref::{Fixture, MODE, StaticConsentPrefilter, StaticResolver, take};

type TestResult = Result<(), Box<dyn Error>>;

/// Two synthetic members, `node-a` and `node-b`.
const REGISTRY: &str = r#"
[[organisation]]
id = "org-a"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[node]]
id = "node-b"
organisation = "org-a"
system_id = "cdr-b.example.org"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "https://cdr-a.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-b-pub"
node = "node-b"
url = "https://cdr-b.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"
"#;

/// The synthetic patient identifier of the fixtures.
const PATIENT_VALUE: &str = "12345";

const EHR_A: &str = "6f2a51a4-1b8e-4f8b-9a4c-1f6c2b1d7e30";

/// The registry of [`REGISTRY`].
fn registry() -> Result<RegistrySnapshot, Box<dyn Error>> {
    Ok(RegistrySnapshot::from_toml_str(REGISTRY)?)
}

/// A configuration text with one cross-reference row per
/// `(member, ehr_id)` for [`PATIENT_VALUE`] in namespace `2.999.1`, and one
/// consent row per `(value, member)` of `denials`.
fn text(rows: &[(&str, &str)], denials: &[(&str, &str)]) -> Result<String, Box<dyn Error>> {
    let mut text = String::from("profile = \"development\"\n");
    for (member, ehr_id) in rows {
        write!(
            text,
            "\n[[testkit.crossref]]\nnamespace = \"2.999.1\"\nvalue = \"{PATIENT_VALUE}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        )?;
    }
    for (value, member) in denials {
        write!(
            text,
            "\n[[testkit.consent_denied]]\nnamespace = \"2.999.1\"\nvalue = \"{value}\"\nmember = \"{member}\"\n"
        )?;
    }
    Ok(text)
}

/// The fixture `text` writes.
fn fixture(text: &str) -> Result<Fixture, Box<dyn Error>> {
    Ok(take(text).1.ok_or("the text writes the fixture's rows")?)
}

/// The resolver over the cross-reference rows of `rows`.
fn enabled(rows: &[(&str, &str)]) -> Result<Arc<StaticResolver>, Box<dyn Error>> {
    Ok(fixture(&text(rows, &[])?)?.resolver(&registry()?)?)
}

/// The consent pre-filter over the consent rows of `denials`.
fn denying(denials: &[(&str, &str)]) -> Result<Arc<StaticConsentPrefilter>, Box<dyn Error>> {
    Ok(fixture(&text(&[("node-a", EHR_A)], denials)?)?.prefilter(&registry()?)?)
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

/// Polls a future once and returns its output; the fixture's futures never
/// wait.
fn ready<F: Future>(future: F) -> Result<F::Output, Box<dyn Error>> {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => Ok(output),
        Poll::Pending => Err("the future was expected to complete without waiting".into()),
    }
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
fn the_rows_leave_the_text_and_the_rest_stays() -> TestResult {
    let (rest, fixture) = take(&text(&[("node-a", EHR_A)], &[])?);
    assert!(fixture.is_some(), "the rows are read");
    assert!(!rest.contains("testkit"), "{rest}");
    assert!(!rest.contains(PATIENT_VALUE), "{rest}");
    assert!(rest.contains("profile = \"development\""), "{rest}");
    Ok(())
}

#[test]
fn a_text_without_rows_or_without_toml_comes_back_as_it_is() {
    for text in ["profile = \"development\"\n", "this is not [ toml"] {
        let (rest, fixture) = take(text);
        assert!(fixture.is_none(), "{text}");
        assert_eq!(text, rest);
    }
}

#[test]
fn a_row_resolves_its_member_and_every_other_member_is_unknown() -> TestResult {
    let resolver = enabled(&[("node-a", EHR_A)])?;
    let outcome = ready(resolver.resolve(
        &patient("2.999.1", PATIENT_VALUE)?,
        &members(&["node-a", "node-b"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ))?;

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
    ))?;
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
    ))?;
    let node_a: NodeId = "node-a".parse()?;
    assert!(
        matches!(outcome.get(&node_a), Some(Resolution::Unknown)),
        "the namespace is part of the identifier"
    );
    Ok(())
}

#[test]
fn the_resolver_works_behind_the_seam() -> TestResult {
    let resolver: Arc<dyn Resolver> = enabled(&[("node-a", EHR_A)])?;
    let outcome = ready(resolver.resolve(
        &patient("2.999.1", PATIENT_VALUE)?,
        &members(&["node-a"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ))?;
    assert_eq!(outcome.len(), 1, "one member, one outcome");
    Ok(())
}

#[test]
fn a_row_naming_a_member_outside_the_registry_is_refused() -> TestResult {
    let refused = fixture(&text(&[("node-z", EHR_A)], &[])?)?
        .resolver(&registry()?)
        .err()
        .ok_or("the row is refused")?;
    assert!(refused.contains("node-z"), "{refused}");
    Ok(())
}

#[test]
fn two_rows_for_one_identifier_at_one_member_are_refused() -> TestResult {
    let ehr_other = "0b6f1d2c-3a4e-4b5f-8c6d-7e8f9a0b1c2d";
    let refused = fixture(&text(&[("node-a", EHR_A), ("node-a", ehr_other)], &[])?)?
        .resolver(&registry()?)
        .err()
        .ok_or("one patient has one ehr_id per member")?;
    assert!(refused.contains("node-a"), "{refused}");
    assert!(!refused.contains(PATIENT_VALUE), "{refused}");
    Ok(())
}

#[test]
fn a_row_with_an_empty_value_is_refused() -> TestResult {
    let text = text(&[("node-a", EHR_A)], &[])?
        .replace(&format!("value = \"{PATIENT_VALUE}\""), "value = \"\"");
    assert!(
        fixture(&text)?.resolver(&registry()?).is_err(),
        "an empty identifier"
    );
    Ok(())
}

#[test]
fn a_row_with_an_unknown_field_or_a_malformed_ehr_id_is_not_read() -> TestResult {
    let unknown =
        text(&[("node-a", EHR_A)], &[])?.replace("ehr_id =", "nickname = \"synthetic\"\nehr_id =");
    let malformed = text(&[("node-a", "not an ehr id")], &[])?;
    for text in [unknown, malformed] {
        let (rest, fixture) = take(&text);
        assert!(fixture.is_none(), "{text}");
        assert_eq!(text, rest, "the text comes back for the gateway to refuse");
    }
    Ok(())
}

#[test]
fn no_rendering_shows_a_patient_identifier_value() -> TestResult {
    let text = text(&[("node-a", EHR_A)], &[(PATIENT_VALUE, "node-b")])?;
    let fixture = fixture(&text)?;
    let resolver = fixture.resolver(&registry()?)?;
    let prefilter = fixture.prefilter(&registry()?)?;
    for rendered in [
        format!("{fixture:?}"),
        format!("{resolver:?}"),
        format!("{prefilter:?}"),
    ] {
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
    ))?;
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
    ))?;
    match answer {
        Localization::NoRecords => Ok(()),
        other => Err(format!("no member holds the patient's data: {other:?}").into()),
    }
}

/// The namespaces the access log asks the patient of an `ehr_id` in.
fn asked(namespaces: &[&str]) -> Result<Vec<IdentifierNamespace>, Box<dyn Error>> {
    namespaces
        .iter()
        .map(|namespace| Ok(IdentifierNamespace::new(*namespace)?))
        .collect()
}

/// Regulation (EU) 2025/327 Art 9(1): the patient a row maps to the `ehr_id`
/// at the member is named, in the namespace asked.
#[test]
fn it_names_the_patient_a_row_maps_to_the_ehr_id() -> TestResult {
    let resolver = enabled(&[("node-a", EHR_A)])?;
    let answer = ready(resolver.identify(
        &"node-a".parse()?,
        &EhrId::new(EHR_A)?,
        &asked(&["2.999.1"])?,
        &OnBehalfOf::Gateway,
        Instant::now(),
    ))?;
    match answer {
        Identification::Named(named) => {
            assert_eq!(1, named.len());
            assert_eq!("2.999.1", named[0].namespace().as_str());
            assert!(!format!("{named:?}").contains(PATIENT_VALUE));
            Ok(())
        }
        other => Err(format!("the patient is named: {other:?}").into()),
    }
}

#[test]
fn it_names_no_patient_for_another_member_or_namespace() -> TestResult {
    let resolver = enabled(&[("node-a", EHR_A)])?;
    for (member, namespace) in [("node-b", "2.999.1"), ("node-a", "2.999.2")] {
        let answer = ready(resolver.identify(
            &member.parse()?,
            &EhrId::new(EHR_A)?,
            &asked(&[namespace])?,
            &OnBehalfOf::Gateway,
            Instant::now(),
        ))?;
        assert!(
            matches!(answer, Identification::Unknown),
            "{member} {namespace}: {answer:?}"
        );
    }
    Ok(())
}

#[test]
fn a_fixture_with_no_consent_row_denies_nothing() -> TestResult {
    assert!(!fixture(&text(&[("node-a", EHR_A)], &[])?)?.denies());
    assert!(fixture(&text(&[("node-a", EHR_A)], &[(PATIENT_VALUE, "node-b")])?)?.denies());
    Ok(())
}

#[test]
fn a_consent_row_naming_no_member_repeated_or_empty_is_refused() {
    for denials in [
        vec![(PATIENT_VALUE, "node-z")],
        vec![(PATIENT_VALUE, "node-b"), (PATIENT_VALUE, "node-b")],
        vec![("", "node-b")],
    ] {
        assert!(denying(&denials).is_err(), "{denials:?}");
    }
}

#[test]
fn only_the_candidates_a_row_names_for_the_patient_are_denied() -> TestResult {
    let prefilter = denying(&[(PATIENT_VALUE, "node-b")])?;
    assert_eq!(prefilter.mode(), MODE);
    let decision = ready(prefilter.prefilter(
        &patient("2.999.1", PATIENT_VALUE)?,
        None,
        &members(&["node-a", "node-b"])?,
        Instant::now(),
    ))?;
    assert_eq!(
        denied(decision)?,
        Some(BTreeSet::from(["node-b".to_owned()])),
        "node-a is not denied, which asserts nothing about its consent (§14.3)"
    );
    Ok(())
}

#[test]
fn a_member_that_is_not_a_candidate_is_never_denied() -> TestResult {
    let prefilter = denying(&[(PATIENT_VALUE, "node-b")])?;
    let decision = ready(prefilter.prefilter(
        &patient("2.999.1", PATIENT_VALUE)?,
        None,
        &members(&["node-a"])?,
        Instant::now(),
    ))?;
    assert_eq!(denied(decision)?, None, "no candidate denied: no signal");
    Ok(())
}

#[test]
fn another_patient_gets_no_consent_signal() -> TestResult {
    let prefilter = denying(&[(PATIENT_VALUE, "node-b")])?;
    let decision = ready(prefilter.prefilter(
        &patient("2.999.1", "67890")?,
        None,
        &members(&["node-a", "node-b"])?,
        Instant::now(),
    ))?;
    assert_eq!(denied(decision)?, None);
    Ok(())
}
