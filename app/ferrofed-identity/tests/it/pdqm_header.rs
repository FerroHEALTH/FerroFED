// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The summary header over PDQm: what the Supplier holds of one patient for
//! the identification and contact elements of a patient summary (eHN PS
//! A.1.1, A.1.2), asked with ITI-78 or ITI-119 by the identifier as the
//! client gave it (PDQm §2:3.78.4.1.2.3, §2:3.119.4.1.2). The one active
//! Patient matched gives the header; no match, several matches and an
//! outage each have their own answer, and none is a header.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::time::{Duration, Instant};

use ferrofed_identity::fhir::{Authentication, Tls};
use ferrofed_identity::ihe::pdqm::{PdqmConfig, PdqmDemographics, Transaction};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::demographics::{Ambiguity, Demographics};
use ferrofed_identity::role::header::{HeaderAnswer, PatientHeader};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::pdq::PdqSupplier;
use secrecy::SecretString;

type TestResult = Result<(), Box<dyn Error>>;

/// The client's identifier, in a namespace the step maps to a system.
const CLIENT_ID: &str = "SENTINEL-LOCAL-663";
const LOCAL_NAMESPACE: &str = "2.999.7";
const LOCAL_SYSTEM: &str = "urn:oid:2.999.7";

/// The master domain, and the master identifier the Supplier knows the
/// patient under.
const MASTER: &str = "urn:oid:2.999.1";
const MASTER_ID: &str = "SENTINEL-MASTER-663";

/// A FHIR identifier system the step maps no namespace to.
const OTHER_SYSTEM: &str = "urn:oid:2.999.5";
const OTHER_ID: &str = "SENTINEL-OTHER-663";

/// The synthetic name and birth date the Supplier holds.
const FAMILY: &str = "SENTINEL-FAMILY-663";
const GIVEN: &str = "SENTINEL-GIVEN-663";
const BIRTH_DATE: &str = "1970-01-01";

/// The step over the Supplier at `base`, asked with `transaction`.
fn step(base: &str, transaction: Transaction) -> Result<PdqmDemographics, Box<dyn Error>> {
    Ok(PdqmDemographics::from_config(PdqmConfig {
        tls: Tls::default(),
        base: SecretUrl::new(base.to_owned()),
        auth: Authentication::None,
        transaction,
        master: MASTER.to_owned(),
        namespaces: BTreeMap::from([(
            IdentifierNamespace::new(LOCAL_NAMESPACE)?,
            LOCAL_SYSTEM.to_owned(),
        )]),
    })?)
}

/// The patient `value` in `namespace`.
fn patient(namespace: &str, value: &str) -> Result<PatientRef, Box<dyn Error>> {
    Ok(PatientRef::new(
        IdentifierNamespace::new(namespace)?,
        SecretString::from(value),
    )?)
}

/// An instant five seconds away.
fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

/// A Supplier that holds the patient under the client's identifier, the
/// master identifier and an identifier no namespace maps to, with a
/// synthetic name and birth date.
async fn supplier() -> Result<PdqSupplier, Box<dyn Error>> {
    let supplier = PdqSupplier::start().await?;
    let id = supplier.add(
        &[
            (LOCAL_SYSTEM, CLIENT_ID),
            (MASTER, MASTER_ID),
            (OTHER_SYSTEM, OTHER_ID),
        ],
        true,
    )?;
    supplier.describe(&id, (FAMILY, GIVEN), Some(BIRTH_DATE))?;
    Ok(supplier)
}

/// The header answer `supplier` gives with `transaction` for the client's
/// identifier.
async fn ask(
    supplier: &PdqSupplier,
    transaction: Transaction,
) -> Result<HeaderAnswer, Box<dyn Error>> {
    Ok(step(&supplier.base_url(), transaction)?
        .header(
            &patient(LOCAL_NAMESPACE, CLIENT_ID)?,
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await)
}

/// The header of `answer`, or an error naming what it was instead.
fn found(answer: HeaderAnswer) -> Result<PatientHeader, String> {
    match answer {
        HeaderAnswer::Found(header) => Ok(*header),
        other => Err(format!("a header, got {other:?}")),
    }
}

#[tokio::test]
async fn the_supplier_s_patient_is_the_header_under_both_transactions() -> TestResult {
    let supplier = supplier().await?;
    for transaction in [Transaction::Search, Transaction::Match] {
        let header = found(
            step(&supplier.base_url(), transaction)?
                .header(
                    &patient(LOCAL_NAMESPACE, CLIENT_ID)?,
                    &OnBehalfOf::Gateway,
                    soon(),
                )
                .await,
        )?;
        let name = header.names.first().ok_or("a name")?;
        assert_eq!(name.family.as_deref(), Some(FAMILY), "{transaction}");
        assert_eq!(name.given, vec![GIVEN.to_owned()], "{transaction}");
        assert_eq!(header.birth_date.as_deref(), Some(BIRTH_DATE));
        assert!(header.named());
    }
    assert_eq!((1, 1), (supplier.searches(), supplier.matches()));
    let bodies = supplier.bodies().join("\n");
    assert!(
        bodies.contains(LOCAL_SYSTEM) && bodies.contains(CLIENT_ID),
        "asked in the system the namespace maps to: {bodies}"
    );
    Ok(())
}

#[tokio::test]
async fn a_master_or_unmapped_absolute_system_is_asked_as_it_is() -> TestResult {
    let supplier = supplier().await?;
    let step = step(&supplier.base_url(), Transaction::Search)?;
    for (namespace, value) in [(MASTER, MASTER_ID), (OTHER_SYSTEM, OTHER_ID)] {
        let header = found(
            step.header(&patient(namespace, value)?, &OnBehalfOf::Gateway, soon())
                .await,
        )?;
        assert!(header.named(), "{namespace}");
    }
    Ok(())
}

#[tokio::test]
async fn a_namespace_that_names_no_system_is_not_handled() -> TestResult {
    let supplier = supplier().await?;
    let answer = step(&supplier.base_url(), Transaction::Search)?
        .header(
            &patient("2.999.8", CLIENT_ID)?,
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert!(matches!(answer, HeaderAnswer::NotHandled), "{answer:?}");
    assert_eq!(0, supplier.searches(), "the Supplier is never asked");
    Ok(())
}

#[tokio::test]
async fn an_identifier_the_supplier_does_not_know_is_no_match() -> TestResult {
    let supplier = supplier().await?;
    for transaction in [Transaction::Search, Transaction::Match] {
        let answer = step(&supplier.base_url(), transaction)?
            .header(
                &patient(LOCAL_NAMESPACE, "SENTINEL-UNKNOWN")?,
                &OnBehalfOf::Gateway,
                soon(),
            )
            .await;
        assert!(
            matches!(answer, HeaderAnswer::NoMatch),
            "{transaction}: {answer:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn several_matched_patients_give_no_header() -> TestResult {
    let supplier = supplier().await?;
    supplier.add(&[(LOCAL_SYSTEM, CLIENT_ID)], true)?;
    let searched = ask(&supplier, Transaction::Search).await?;
    assert!(
        matches!(
            searched,
            HeaderAnswer::Ambiguous(Ambiguity::SeveralPatients)
        ),
        "§2:3.78.4.1.3 Case 1: never one of them picked: {searched:?}"
    );
    let matched = ask(&supplier, Transaction::Match).await?;
    assert!(
        matches!(matched, HeaderAnswer::NoMatch),
        "§2:3.119.4.1.3 Case 5: several matches under onlyCertainMatches are none: {matched:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_outage_is_unavailable_never_no_match() -> TestResult {
    let answer = step("http://127.0.0.1:9/fhir/", Transaction::Search)?
        .header(
            &patient(LOCAL_NAMESPACE, CLIENT_ID)?,
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert!(matches!(answer, HeaderAnswer::Unavailable(_)), "{answer:?}");
    Ok(())
}

#[tokio::test]
async fn a_header_s_debug_output_shows_no_value() -> TestResult {
    let supplier = supplier().await?;
    let header = found(
        step(&supplier.base_url(), Transaction::Search)?
            .header(
                &patient(LOCAL_NAMESPACE, CLIENT_ID)?,
                &OnBehalfOf::Gateway,
                soon(),
            )
            .await,
    )?;
    let shown = format!("{header:?}");
    for value in [FAMILY, GIVEN, BIRTH_DATE] {
        assert!(!shown.contains(value), "{shown}");
    }
    Ok(())
}
