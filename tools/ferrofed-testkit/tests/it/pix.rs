// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness PIX Manager, held to the PIXm client of `crates/ihe-iti` and to
//! the vendored IHE PIXm 3.1.0 artefacts: ITI-104 seeds what ITI-83 answers.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;

use ferrofed_testkit::pix::PixManager;
use ferrofed_testkit::proxy::{CapturingProxy, Fault};
use ferrofed_testkit::seed::{self, CrossReferenceSeed, EhrDomain, PatientId};
use fhir_types::codec::{Json, Path, Value, expect_object};
use fhir_types::r4::patient::Patient;
use http::StatusCode;
use ihe_iti::pixm::error::PixmError;
use ihe_iti::pixm::identifier::{CrossReference, SourceIdentifier, TargetSystem};
use ihe_iti::pixm::{Invocation, PixmClient};
use ihe_iti::user::OnBehalfOf;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

type TestResult = Result<(), Box<dyn Error>>;

const EHR_A: Uuid = Uuid::from_u128(0x0a0a_0a0a_0a0a_4a0a_8a0a_0a0a_0a0a_0a0a);
const EHR_B: Uuid = Uuid::from_u128(0x0b0b_0b0b_0b0b_4b0b_8b0b_0b0b_0b0b_0b0b);

/// The vendored PIXm package.
const PIXM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/ihe-pixm/package"
);

/// The patient the tests feed, known at node 1 and node 2.
fn known() -> CrossReferenceSeed {
    CrossReferenceSeed {
        patient: PatientId::new(1, 47),
        ehrs: vec![(EhrDomain::new(1), EHR_A), (EhrDomain::new(2), EHR_B)],
    }
}

fn client(pix: &PixManager) -> Result<PixmClient, Box<dyn Error>> {
    Ok(PixmClient::new(
        url::Url::parse(&pix.base_url())?,
        reqwest::Client::new(),
    )?)
}

fn source(patient: PatientId) -> Result<SourceIdentifier, Box<dyn Error>> {
    Ok(SourceIdentifier::new(
        patient.namespace(),
        SecretString::from(patient.value()),
    )?)
}

fn target(domain: EhrDomain) -> Result<TargetSystem, Box<dyn Error>> {
    Ok(TargetSystem::new(domain.system())?)
}

const BUDGET: Duration = Duration::from_secs(5);

/// The `(system, value)` pairs of a matched answer, sorted.
fn identifiers(answer: &CrossReference) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let CrossReference::Matched(matched) = answer else {
        return Err(format!("expected a match, got {answer:?}").into());
    };
    let mut found: Vec<(String, String)> = matched
        .identifiers()
        .iter()
        .map(|id| {
            (
                id.system().to_owned(),
                id.value().expose_secret().to_owned(),
            )
        })
        .collect();
    found.sort();
    Ok(found)
}

#[tokio::test]
async fn a_fed_patient_resolves_through_the_pixm_client_in_every_domain() -> TestResult {
    let pix = PixManager::start().await?;
    assert_eq!(
        StatusCode::CREATED,
        seed::feed(&pix.base_url(), &known()).await?
    );
    let answer = client(&pix)?
        .cross_reference(&source(known().patient)?, &[], &OnBehalfOf::System, BUDGET)
        .await?;
    assert_eq!(
        vec![
            (EhrDomain::new(1).system(), EHR_A.to_string()),
            (EhrDomain::new(2).system(), EHR_B.to_string()),
        ],
        identifiers(&answer)?,
        "every other domain's identifier, the source's own left out"
    );
    assert_eq!(1, pix.queries());
    Ok(())
}

#[tokio::test]
async fn a_target_system_narrows_the_answer_to_that_domain() -> TestResult {
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let answer = client(&pix)?
        .cross_reference(
            &source(known().patient)?,
            &[target(EhrDomain::new(2))?],
            &OnBehalfOf::System,
            BUDGET,
        )
        .await?;
    assert_eq!(
        vec![(EhrDomain::new(2).system(), EHR_B.to_string())],
        identifiers(&answer)?
    );
    Ok(())
}

#[tokio::test]
async fn a_refeed_replaces_the_patient_instead_of_adding_one() -> TestResult {
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let moved = CrossReferenceSeed {
        patient: known().patient,
        ehrs: vec![(EhrDomain::new(1), EHR_A)],
    };
    assert_eq!(StatusCode::OK, seed::feed(&pix.base_url(), &moved).await?);
    assert_eq!((2, 1), (pix.feeds(), pix.patients()));
    let answer = client(&pix)?
        .cross_reference(&source(known().patient)?, &[], &OnBehalfOf::System, BUDGET)
        .await?;
    assert_eq!(
        vec![(EhrDomain::new(1).system(), EHR_A.to_string())],
        identifiers(&answer)?,
        "the update is a replacement (ITI-104 conditional update)"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_patient_in_a_known_domain_is_not_found() -> TestResult {
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let answer = client(&pix)?
        .cross_reference(
            &source(PatientId::new(1, 48))?,
            &[],
            &OnBehalfOf::System,
            BUDGET,
        )
        .await?;
    assert!(
        matches!(answer, CrossReference::SourceNotFound),
        "ITI TF-2 §3.83.4.2.2.2, Case 2: {answer:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_source_domain_is_refused_as_case_3() -> TestResult {
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let refused = client(&pix)?
        .cross_reference(
            &source(PatientId::new(9, 47))?,
            &[],
            &OnBehalfOf::System,
            BUDGET,
        )
        .await;
    assert!(
        matches!(refused, Err(PixmError::SourceDomainNotRecognized)),
        "ITI TF-2 §3.83.4.2.2.3: {refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_proxy_in_front_journals_the_request_and_injects_a_fault() -> TestResult {
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let proxy = CapturingProxy::start(pix.origin()).await?;
    let through = PixmClient::new(
        url::Url::parse(&format!("{}/fhir/", proxy.origin()))?,
        reqwest::Client::new(),
    )?;
    let patient = known().patient;

    let answer = through
        .cross_reference(&source(patient)?, &[], &OnBehalfOf::System, BUDGET)
        .await?;
    assert_eq!(2, identifiers(&answer)?.len(), "forwarded unmodified");
    let journal = proxy.journal();
    let asked = journal
        .first()
        .ok_or("the proxy journaled the ITI-83 call")?;
    assert_eq!(
        ("GET", "/fhir/Patient/$ihe-pix"),
        (asked.method.as_str(), asked.path.as_str())
    );

    proxy.set_fault(Fault::Status(StatusCode::SERVICE_UNAVAILABLE));
    let failed = through
        .cross_reference(&source(patient)?, &[], &OnBehalfOf::System, BUDGET)
        .await;
    assert!(
        matches!(
            failed,
            Err(PixmError::Rejected { status, .. }) if status == StatusCode::SERVICE_UNAVAILABLE
        ),
        "{failed:?}"
    );
    assert_eq!(1, pix.queries(), "the failed call never reached the device");
    Ok(())
}

#[tokio::test]
async fn a_posted_query_resolves_as_the_get_does_and_names_the_patient_in_no_url() -> TestResult {
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let proxy = CapturingProxy::start(pix.origin()).await?;
    let posting = PixmClient::new(
        url::Url::parse(&format!("{}/fhir/", proxy.origin()))?,
        reqwest::Client::new(),
    )?
    .invoked_by(Invocation::Post);
    let patient = known().patient;
    let all = posting
        .cross_reference(&source(patient)?, &[], &OnBehalfOf::System, BUDGET)
        .await?;
    let narrowed = posting
        .cross_reference(
            &source(patient)?,
            &[target(EhrDomain::new(2))?],
            &OnBehalfOf::System,
            BUDGET,
        )
        .await?;
    assert_eq!(
        (
            identifiers(
                &client(&pix)?
                    .cross_reference(&source(patient)?, &[], &OnBehalfOf::System, BUDGET)
                    .await?
            )?,
            vec![(EhrDomain::new(2).system(), EHR_B.to_string())],
        ),
        (identifiers(&all)?, identifiers(&narrowed)?),
        "the device answers a POST as it answers the GET"
    );
    let unknown = posting
        .cross_reference(
            &source(PatientId::new(1, 48))?,
            &[],
            &OnBehalfOf::System,
            BUDGET,
        )
        .await?;
    assert!(
        matches!(unknown, CrossReference::SourceNotFound),
        "ITI TF-2 §3.83.4.2.2.2, Case 2: {unknown:?}"
    );
    let journal = proxy.journal();
    assert_eq!(3, journal.len(), "every posted call passed the proxy");
    let asked_about = [patient, patient, PatientId::new(1, 48)];
    for (asked, about) in journal.iter().zip(asked_about) {
        assert_eq!(
            ("POST", "/fhir/Patient/$ihe-pix", None),
            (
                asked.method.as_str(),
                asked.path.as_str(),
                asked.query.as_deref()
            ),
            "the parameters ride in the body"
        );
        let value = about.value();
        assert!(
            asked
                .body
                .windows(value.len())
                .any(|w| w == value.as_bytes()),
            "the body carries the source identifier"
        );
        assert!(!asked.path.contains(&value), "the path names the patient");
    }
    Ok(())
}

#[tokio::test]
async fn a_posted_body_that_is_not_parameters_or_names_another_input_is_refused() -> TestResult {
    let pix = PixManager::start().await?;
    let operation = format!("{}Patient/$ihe-pix", pix.base_url());
    let http = reqwest::Client::new();
    for (media, body) in [
        ("application/fhir+json", r#"{"resourceType":"Bundle"}"#),
        (
            "application/fhir+json",
            r#"{"resourceType":"Parameters","parameter":[{"name":"identifier","valueString":"x"}]}"#,
        ),
        (
            "application/fhir+json",
            r#"{"resourceType":"Parameters","parameter":[{"name":"sourceIdentifier","valueUri":"x"}]}"#,
        ),
        ("text/plain", "sourceIdentifier=x"),
    ] {
        let status = http
            .post(&operation)
            .header("content-type", media)
            .body(body)
            .send()
            .await?
            .status();
        assert_eq!(StatusCode::BAD_REQUEST, status, "{media} {body}");
    }
    assert_eq!(0, pix.queries(), "no refused body was asked about");
    Ok(())
}

#[tokio::test]
async fn an_unknown_target_domain_is_refused_as_case_4() -> TestResult {
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let refused = client(&pix)?
        .cross_reference(
            &source(known().patient)?,
            &[target(EhrDomain::new(9))?],
            &OnBehalfOf::System,
            BUDGET,
        )
        .await;
    assert!(
        matches!(refused, Err(PixmError::TargetDomainNotRecognized)),
        "ITI TF-2 §3.83.4.2.2.4: {refused:?}"
    );
    Ok(())
}

/// The bytes and the first identifier of a vendored example Patient.
fn example(file: &str) -> Result<(Vec<u8>, Option<String>), Box<dyn Error>> {
    let bytes = std::fs::read(PathBuf::from(PIXM).join("example").join(file))?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let patient = Patient::from_json(
        expect_object(&value, &Path::root("Patient"))?,
        &mut Path::root("Patient"),
    )?;
    let identifier = patient.identifier.first().and_then(|id| {
        let system = id.system.as_ref()?.value.clone()?;
        let value = id.value.as_ref()?.value.clone()?;
        Some(format!("{system}|{value}"))
    });
    Ok((bytes, identifier))
}

/// Sends a vendored example Patient as an ITI-104 conditional update.
async fn feed_example(
    pix: &PixManager,
    file: &str,
    condition: &str,
) -> Result<StatusCode, Box<dyn Error>> {
    let (bytes, _) = example(file)?;
    let mut url = url::Url::parse(&pix.base_url())?.join("Patient")?;
    url.query_pairs_mut().append_pair("identifier", condition);
    Ok(reqwest::Client::new()
        .put(url)
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .body(bytes)
        .send()
        .await?
        .status())
}

const PROFILED: [&str; 7] = [
    "Patient-Patient-MaidenAlice-Red.json",
    "Patient-Patient-MohrAlice-Blue.json",
    "Patient-Patient-MohrAlice-Green.json",
    "Patient-Patient-MohrAlice-Red.json",
    "Patient-Patient-MohrAlice.json",
    "Patient-Patient-MohrAlissa-Red.json",
    "Patient-Patient-MohrMaidenResolvedByMohrMalice-Red.json",
];

#[tokio::test]
async fn the_feed_accepts_every_vendored_example_that_claims_the_pixm_profile() -> TestResult {
    // The IG's examples are each a conformant Patient, not one feed: several
    // share an identifier, so each is fed to a device of its own.
    for file in PROFILED {
        let pix = PixManager::start().await?;
        let (_, identifier) = example(file)?;
        let identifier = identifier.ok_or("a profiled example carries an identifier")?;
        let status = feed_example(&pix, file, &identifier).await?;
        assert_eq!(StatusCode::CREATED, status, "{file}");
    }
    Ok(())
}

#[tokio::test]
async fn a_condition_matching_two_patients_is_refused_with_412() -> TestResult {
    let pix = PixManager::start().await?;
    let first = CrossReferenceSeed {
        patient: PatientId::new(1, 1),
        ehrs: Vec::new(),
    };
    let second = CrossReferenceSeed {
        patient: PatientId::new(1, 2),
        ehrs: vec![(EhrDomain::new(1), EHR_A)],
    };
    seed::feed(&pix.base_url(), &first).await?;
    seed::feed(&pix.base_url(), &second).await?;
    // A third Patient carrying the first's identifier and the second's ehr_id,
    // conditioned on that ehr_id, matches the second alone and replaces it;
    // now the ehr_id names one Patient and the first's identifier names two.
    let bridge = r#"{"resourceType":"Patient","identifier":[{"system":"urn:oid:2.999.1.1","value":"ffd-test-0001"},{"system":"urn:oid:2.999.2.1","value":"0a0a0a0a-0a0a-4a0a-8a0a-0a0a0a0a0a0a"}],"name":[{"family":"Synthetic"}]}"#;
    let put = |condition: &str| -> Result<reqwest::RequestBuilder, Box<dyn Error>> {
        let mut url = url::Url::parse(&pix.base_url())?.join("Patient")?;
        url.query_pairs_mut().append_pair("identifier", condition);
        Ok(reqwest::Client::new()
            .put(url)
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .body(bridge))
    };
    let replaced = put("urn:oid:2.999.2.1|0a0a0a0a-0a0a-4a0a-8a0a-0a0a0a0a0a0a")?
        .send()
        .await?
        .status();
    assert_eq!(StatusCode::OK, replaced);
    let ambiguous = put("urn:oid:2.999.1.1|ffd-test-0001")?
        .send()
        .await?
        .status();
    assert_eq!(
        StatusCode::PRECONDITION_FAILED,
        ambiguous,
        "a conditional update matching several resources is a 412 (FHIR R4 http.html#cond-update)"
    );
    Ok(())
}

#[tokio::test]
async fn the_feed_refuses_the_example_that_claims_no_pixm_profile() -> TestResult {
    let pix = PixManager::start().await?;
    let status = feed_example(&pix, "Patient-ex-patient.json", "urn:oid:2.999.1.1|x").await?;
    assert_eq!(
        StatusCode::UNPROCESSABLE_ENTITY,
        status,
        "IHE.PIXm.Patient makes Patient.identifier 1..*"
    );
    assert_eq!(0, pix.patients());
    Ok(())
}

#[tokio::test]
async fn identifiers_fed_together_cross_reference_each_other() -> TestResult {
    let pix = PixManager::start().await?;
    let (_, identifier) = example("Patient-Patient-MohrAlice.json")?;
    let identifier = identifier.ok_or("the example carries identifiers")?;
    feed_example(&pix, "Patient-Patient-MohrAlice.json", &identifier).await?;
    let blue = SourceIdentifier::new(
        "urn:oid:1.3.6.1.4.1.21367.13.20.3000",
        SecretString::from("IHEBLUE-994".to_owned()),
    )?;
    let answer = client(&pix)?
        .cross_reference(&blue, &[], &OnBehalfOf::System, BUDGET)
        .await?;
    assert_eq!(
        vec![
            (
                "urn:oid:1.3.6.1.4.1.21367.13.20.1000".to_owned(),
                "IHERED-994".to_owned()
            ),
            (
                "urn:oid:1.3.6.1.4.1.21367.13.20.2000".to_owned(),
                "IHEGREEN-994".to_owned()
            ),
        ],
        identifiers(&answer)?,
        "the IG's three-domain Patient, asked by its blue identifier"
    );
    Ok(())
}

#[tokio::test]
async fn a_deprecated_patient_answers_the_empty_bundle_of_a_merge() -> TestResult {
    let pix = PixManager::start().await?;
    let file = "Patient-Patient-MohrMaidenResolvedByMohrMalice-Red.json";
    let (_, identifier) = example(file)?;
    let identifier = identifier.ok_or("the example carries an identifier")?;
    feed_example(&pix, file, &identifier).await?;
    let deprecated = SourceIdentifier::new(
        "urn:oid:1.3.6.1.4.1.21367.13.20.1000",
        SecretString::from("IHERED-m94".to_owned()),
    )?;
    let answer = client(&pix)?
        .cross_reference(&deprecated, &[], &OnBehalfOf::System, BUDGET)
        .await?;
    assert!(
        matches!(answer, CrossReference::SourceNotFound),
        "ITI TF-2 §3.83.4.2.2.5: {answer:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_without_a_name_is_refused() -> TestResult {
    let pix = PixManager::start().await?;
    let mut url = url::Url::parse(&pix.base_url())?.join("Patient")?;
    url.query_pairs_mut()
        .append_pair("identifier", "urn:oid:2.999.1.1|ffd-test-0047");
    let status = reqwest::Client::new()
        .put(url)
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .body(
            r#"{"resourceType":"Patient","identifier":[{"system":"urn:oid:2.999.1.1","value":"ffd-test-0047"}]}"#,
        )
        .send()
        .await?
        .status();
    assert_eq!(
        StatusCode::UNPROCESSABLE_ENTITY,
        status,
        "Patient.name is 1..*"
    );
    Ok(())
}

#[tokio::test]
async fn the_remove_patient_option_forgets_the_patient() -> TestResult {
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let patient = known().patient;
    let mut url = url::Url::parse(&pix.base_url())?.join("Patient")?;
    url.query_pairs_mut().append_pair(
        "identifier",
        &format!("{}|{}", patient.namespace(), patient.value()),
    );
    let status = reqwest::Client::new().delete(url).send().await?.status();
    assert_eq!(StatusCode::NO_CONTENT, status);
    assert_eq!(0, pix.patients());
    Ok(())
}

/// The names of a vendored artefact's `$.<list>[*].<key>` where `$.<list>[*].
/// <filter>` is `wanted`.
fn names(
    file: &str,
    list: &str,
    filter: &str,
    wanted: &str,
    key: &str,
) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let bytes = std::fs::read(PathBuf::from(PIXM).join(file))?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let items = value
        .get(list)
        .and_then(Value::as_array)
        .ok_or("the artefact has the list")?;
    Ok(items
        .iter()
        .filter(|item| item.get(filter).and_then(Value::as_str) == Some(wanted))
        .filter_map(|item| item.get(key).and_then(Value::as_str))
        .map(str::to_owned)
        .collect())
}

#[tokio::test]
async fn an_answer_names_only_the_out_parameters_of_the_operation_definition() -> TestResult {
    let out = names(
        "OperationDefinition-IHE.PIXm.pix.json",
        "parameter",
        "use",
        "out",
        "name",
    )?;
    assert_eq!(
        BTreeSet::from(["targetId".to_owned(), "targetIdentifier".to_owned()]),
        out,
        "the vendored $ihe-pix out parameters"
    );
    let pix = PixManager::start().await?;
    seed::feed(&pix.base_url(), &known()).await?;
    let patient = known().patient;
    let mut url = url::Url::parse(&pix.base_url())?.join("Patient/$ihe-pix")?;
    url.query_pairs_mut().append_pair(
        "sourceIdentifier",
        &format!("{}|{}", patient.namespace(), patient.value()),
    );
    let body = reqwest::Client::new()
        .get(url)
        .send()
        .await?
        .bytes()
        .await?;
    let value: Value = serde_json::from_slice(&body)?;
    let sent: BTreeSet<String> = value
        .get("parameter")
        .and_then(Value::as_array)
        .ok_or("a Parameters answer")?
        .iter()
        .filter_map(|parameter| parameter.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    assert!(
        sent.is_subset(&out),
        "{sent:?} names a parameter $ihe-pix does not declare"
    );
    Ok(())
}

#[test]
fn the_feed_enforces_exactly_the_profile_minimums_on_identifier_and_name() -> TestResult {
    let bytes =
        std::fs::read(PathBuf::from(PIXM).join("StructureDefinition-IHE.PIXm.Patient.json"))?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let required: BTreeSet<String> = value
        .get("snapshot")
        .and_then(|snapshot| snapshot.get("element"))
        .and_then(Value::as_array)
        .ok_or("the profile has a snapshot")?
        .iter()
        .filter(|element| {
            element
                .get("min")
                .and_then(|min| match min {
                    Value::Number(number) => number.as_u64(),
                    _ => None,
                })
                .is_some_and(|min| min > 0)
        })
        .filter_map(|element| element.get("path").and_then(Value::as_str))
        .filter(|path| path.starts_with("Patient.identifier") || *path == "Patient.name")
        .map(str::to_owned)
        .collect();
    assert_eq!(
        BTreeSet::from([
            "Patient.identifier".to_owned(),
            "Patient.identifier.system".to_owned(),
            "Patient.identifier.value".to_owned(),
            "Patient.name".to_owned(),
        ]),
        required,
        "the minimums the harness feed checks; a re-pin that moves them fails here"
    );
    Ok(())
}
