// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness PDQm Supplier, held to the PDQm client of `crates/ihe-iti`:
//! ITI-78 searches by identifier and ITI-119 matches answer from the
//! synthetic Patients a test adds, in the `urn:oid:2.999` example arc.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::time::Duration;

use ferrofed_testkit::pdq::{PdqError, PdqSupplier};
use ihe_iti::pdqm::PdqmClient;
use ihe_iti::pdqm::error::PdqmError;
use ihe_iti::pdqm::input::MatchInput;
use ihe_iti::pdqm::matches::MatchGrade;
use ihe_iti::pdqm::query::PatientQuery;
use secrecy::SecretString;

type TestResult = Result<(), Box<dyn Error>>;

/// The local domain a client names the patient in, and the master domain.
const LOCAL: &str = "urn:oid:2.999.7";
const MASTER: &str = "urn:oid:2.999.1";

const PROMPT: Duration = Duration::from_secs(5);

fn client(supplier: &PdqSupplier) -> Result<PdqmClient, Box<dyn Error>> {
    Ok(PdqmClient::new(
        url::Url::parse(&supplier.base_url())?,
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
    )?)
}

fn by_identifier(value: &str) -> Result<PatientQuery, Box<dyn Error>> {
    Ok(PatientQuery::new()
        .identifier(Some(LOCAL), &SecretString::from(value))?
        .identifier_domains(&[MASTER])?)
}

#[tokio::test]
async fn a_search_by_identifier_answers_the_patient_with_its_master_identifier_only() -> TestResult
{
    let supplier = PdqSupplier::start().await?;
    supplier.add(&[(LOCAL, "L-1"), (MASTER, "M-1")], true)?;
    supplier.add(&[(LOCAL, "L-2"), (MASTER, "M-2")], true)?;
    let page = client(&supplier)?
        .search(&by_identifier("L-1")?, PROMPT)
        .await?;
    assert_eq!(1, page.total(), "§2:3.78.4.1.3, Case 1");
    let [matched] = page.patients() else {
        panic!("one match");
    };
    let systems: Vec<Option<&str>> = matched
        .patient()
        .identifier
        .iter()
        .map(|identifier| {
            identifier
                .system
                .as_ref()
                .and_then(|it| it.value.as_deref())
        })
        .collect();
    assert_eq!(vec![Some(MASTER)], systems, "§2:3.78.4.1.3, Case 2");
    assert_eq!(1, supplier.searches());
    assert!(
        supplier.bodies()[0].contains("L-1"),
        "the search reached it"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_identifier_is_a_total_of_zero() -> TestResult {
    let supplier = PdqSupplier::start().await?;
    supplier.add(&[(LOCAL, "L-1"), (MASTER, "M-1")], true)?;
    let page = client(&supplier)?
        .search(&by_identifier("L-9")?, PROMPT)
        .await?;
    assert_eq!(0, page.total(), "§2:3.78.4.1.3, Case 3");
    Ok(())
}

#[tokio::test]
async fn a_domain_no_patient_carries_is_not_recognized() -> TestResult {
    let supplier = PdqSupplier::start().await?;
    supplier.add(&[(LOCAL, "L-1")], true)?;
    let error = client(&supplier)?
        .search(&by_identifier("L-1")?, PROMPT)
        .await
        .expect_err("the master domain is unknown");
    assert!(
        matches!(error, PdqmError::DomainNotRecognized),
        "§2:3.78.4.1.3, Case 4: {error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_match_answers_a_certain_match() -> TestResult {
    let supplier = PdqSupplier::start().await?;
    supplier.add(&[(LOCAL, "L-1"), (MASTER, "M-1")], true)?;
    let input = MatchInput::new(LOCAL, &SecretString::from("L-1"))?.only_certain_matches(true);
    let found = client(&supplier)?.match_patient(&input, PROMPT).await?;
    let [matched] = found.patients() else {
        panic!("one match");
    };
    assert_eq!(Some(MatchGrade::Certain), matched.grade());
    assert_eq!(1, supplier.matches());
    Ok(())
}

#[tokio::test]
async fn several_matches_under_only_certain_matches_are_the_zero_result_set() -> TestResult {
    let supplier = PdqSupplier::start().await?;
    supplier.add(&[(LOCAL, "L-1"), (MASTER, "M-1")], true)?;
    supplier.add(&[(LOCAL, "L-1"), (MASTER, "M-2")], true)?;
    let certain = MatchInput::new(LOCAL, &SecretString::from("L-1"))?.only_certain_matches(true);
    let found = client(&supplier)?.match_patient(&certain, PROMPT).await?;
    assert!(found.patients().is_empty(), "§2:3.119.4.1.3, Case 5");
    let any = MatchInput::new(LOCAL, &SecretString::from("L-1"))?;
    let found = client(&supplier)?.match_patient(&any, PROMPT).await?;
    assert_eq!(2, found.patients().len(), "§2:3.119.4.1.3, Case 2");
    Ok(())
}

#[tokio::test]
async fn a_system_outside_the_example_arc_is_refused() -> TestResult {
    let supplier = PdqSupplier::start().await?;
    assert!(matches!(
        supplier.add(&[("urn:oid:1.2.3", "L-1")], true),
        Err(PdqError::OutsideExampleArc)
    ));
    assert!(matches!(supplier.add(&[], true), Err(PdqError::Empty)));
    Ok(())
}
