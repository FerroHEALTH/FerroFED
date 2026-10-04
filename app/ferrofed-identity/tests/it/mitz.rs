// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mitz consent pre-filter over the stub Mitz: the data holder of each
//! candidate asked once, a member ruled out only when Mitz denies its holder,
//! a patient Mitz cannot be asked about left with no signal, and every
//! refused configuration (Annex B §B.6, N27a, §14.3).

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::time::{Duration, Instant};

use ferrofed_identity::consent::{ConsentDecision, ConsentError, ConsentPrefilter, Requester};
use ferrofed_identity::mitz::{
    HolderConfig, MITZ_MODE, MitzConfig, MitzConfigError, MitzPrefilter,
};
use ferrofed_identity::patient::{IdentifierNamespace, PatientRef};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::mitz::Mitz;

use crate::support::{PATIENT_VALUE, registry};

type TestResult = Result<(), Box<dyn Error>>;

/// The namespace the configuration lists as standing for the BSN.
const BSN_ALIAS: &str = "2.999.1";

fn member(id: &str) -> Result<NodeId, Box<dyn Error>> {
    Ok(id.parse()?)
}

/// A configuration asking the Mitz at `endpoint`, both members held by
/// `holders`.
fn config(endpoint: &str, holders: &[(&str, Option<&str>)]) -> Result<MitzConfig, Box<dyn Error>> {
    let mut held = BTreeMap::new();
    for (id, ura) in holders {
        held.insert(
            member(id)?,
            HolderConfig {
                ura: ura.map(str::to_owned),
                kind: String::from("V6"),
            },
        );
    }
    Ok(MitzConfig {
        endpoint: SecretUrl::new(endpoint),
        development: true,
        credentials: None,
        client_identity: None,
        trust_roots: None,
        namespaces: BTreeSet::from([IdentifierNamespace::new(BSN_ALIAS)?]),
        categories: vec![String::from("GGC002")],
        purpose: String::from("TREAT"),
        holders: held,
        custodians: BTreeMap::new(),
        timeout: Duration::from_secs(2),
    })
}

fn patient(namespace: &str) -> Result<PatientRef, Box<dyn Error>> {
    Ok(PatientRef::new(
        IdentifierNamespace::new(namespace)?,
        PATIENT_VALUE.into(),
    )?)
}

fn candidates() -> Result<Vec<NodeId>, Box<dyn Error>> {
    Ok(vec![member("node-a")?, member("node-b")?])
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

/// A verified caller as its token states it, with synthetic values.
fn requester(professional: &str, role: &str) -> Result<Requester, Box<dyn Error>> {
    Ok(Requester::new(
        professional.to_owned(),
        role.to_owned(),
        String::from("ura-test-0100"),
        String::from("V6"),
    )
    .ok_or("every value is set")?)
}

/// The default caller of these tests.
fn caller() -> Result<Requester, Box<dyn Error>> {
    requester("professional0001", "01.015")
}

/// A pre-filter over the stub `mitz`, node A and node B held by two URAs.
fn over(mitz: &Mitz) -> Result<MitzPrefilter, Box<dyn Error>> {
    let holders = [
        ("node-a", Some("ura-test-0001")),
        ("node-b", Some("ura-test-0002")),
    ];
    Ok(MitzPrefilter::from_config(
        config(&mitz.endpoint(), &holders)?,
        &registry(),
    )?)
}

#[tokio::test]
async fn the_question_names_the_callers_professional_role_and_organisation() -> TestResult {
    let mitz = Mitz::start().await;
    let prefilter = over(&mitz)?;
    prefilter
        .prefilter(
            &patient(BSN_ALIAS)?,
            Some(&caller()?),
            &candidates()?,
            soon(),
        )
        .await;
    let questions = mitz.questions().await;
    assert_eq!(2, questions.len());
    for question in &questions {
        assert!(
            question.contains(
                r#"<hl7:InstanceIdentifier root="2.16.528.1.1007.3.1" extension="professional0001"/>"#
            ),
            "§3.2.4.2: the professional by UZI number: {question}"
        );
        assert!(
            question.contains(
                r#"<hl7:CodedValue code="01.015" codeSystem="2.16.840.1.113883.2.4.15.111"/>"#
            ),
            "the professional's role: {question}"
        );
        assert!(
            question.contains(
                r#"<hl7:InstanceIdentifier root="2.16.528.1.1007.3.3" extension="ura-test-0100"/>"#
            ),
            "the caller's organisation by URA: {question}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_caller_whose_token_names_no_requester_asks_nothing() -> TestResult {
    let mitz = Mitz::start().await;
    mitz.deny(PATIENT_VALUE, "ura-test-0001");
    let prefilter = over(&mitz)?;
    let decision = prefilter
        .prefilter(&patient(BSN_ALIAS)?, None, &candidates()?, soon())
        .await;
    assert!(
        matches!(decision, ConsentDecision::NoSignal),
        "N27: nothing is filtered, and each node decides: {decision:?}"
    );
    assert!(mitz.questions().await.is_empty(), "Mitz is never asked");
    Ok(())
}

#[tokio::test]
async fn two_callers_with_different_roles_send_different_subjects() -> TestResult {
    let mitz = Mitz::start().await;
    let prefilter = over(&mitz)?;
    let first = requester("professional0001", "01.015")?;
    let second = requester("professional0002", "30.000")?;
    for asking in [&first, &second] {
        prefilter
            .prefilter(&patient(BSN_ALIAS)?, Some(asking), &candidates()?, soon())
            .await;
    }
    let questions = mitz.questions().await;
    assert_eq!(4, questions.len());
    let by = |professional: &str, role: &str| {
        questions
            .iter()
            .filter(|question| {
                question.contains(&format!("extension=\"{professional}\""))
                    && question.contains(&format!("code=\"{role}\""))
            })
            .count()
    };
    assert_eq!(
        2,
        by("professional0001", "01.015"),
        "the first caller's own subject"
    );
    assert_eq!(
        2,
        by("professional0002", "30.000"),
        "the second caller's own subject"
    );
    Ok(())
}

#[tokio::test]
async fn a_requester_the_question_does_not_take_is_no_decision() -> TestResult {
    let mitz = Mitz::start().await;
    let prefilter = over(&mitz)?;
    let refused = requester("with-hyphen", "01.015")?;
    let decision = prefilter
        .prefilter(&patient(BSN_ALIAS)?, Some(&refused), &candidates()?, soon())
        .await;
    assert!(
        matches!(decision, ConsentDecision::Unavailable(_)),
        "§5: a UZI number is alphanumeric: {decision:?}"
    );
    assert!(mitz.questions().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn members_of_one_holder_are_asked_about_once_and_denied_together() -> TestResult {
    let mitz = Mitz::start().await;
    mitz.deny(PATIENT_VALUE, "ura-test-0001");
    let holders = [
        ("node-a", Some("ura-test-0001")),
        ("node-b", Some("ura-test-0001")),
    ];
    let prefilter = MitzPrefilter::from_config(config(&mitz.endpoint(), &holders)?, &registry())?;
    assert_eq!(MITZ_MODE, prefilter.mode());
    let decision = prefilter
        .prefilter(
            &patient(BSN_ALIAS)?,
            Some(&caller()?),
            &candidates()?,
            soon(),
        )
        .await;
    match decision {
        ConsentDecision::Denied(denied) => {
            assert_eq!(
                BTreeSet::from([member("node-a")?, member("node-b")?]),
                denied
            );
        }
        other => return Err(format!("both members share the denied holder: {other:?}").into()),
    }
    assert_eq!(1, mitz.questions().await.len(), "one question per holder");
    Ok(())
}

#[tokio::test]
async fn a_permit_for_every_holder_carries_no_signal() -> TestResult {
    let mitz = Mitz::start().await;
    let holders = [
        ("node-a", Some("ura-test-0001")),
        ("node-b", Some("ura-test-0002")),
    ];
    let prefilter = MitzPrefilter::from_config(config(&mitz.endpoint(), &holders)?, &registry())?;
    let decision = prefilter
        .prefilter(
            &patient(BSN_ALIAS)?,
            Some(&caller()?),
            &candidates()?,
            soon(),
        )
        .await;
    assert!(
        matches!(decision, ConsentDecision::NoSignal),
        "§14.3: a permit clears nothing, it only rules nothing out: {decision:?}"
    );
    assert_eq!(2, mitz.questions().await.len());
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_patient_named_by_a_pseudonym_is_never_sent_to_mitz() -> TestResult {
    let mitz = Mitz::start().await;
    let holders = [
        ("node-a", Some("ura-test-0001")),
        ("node-b", Some("ura-test-0002")),
    ];
    let prefilter = MitzPrefilter::from_config(config(&mitz.endpoint(), &holders)?, &registry())?;
    let pseudonym = patient("http://fhir.nl/fhir/NamingSystem/pseudo-bsn")?;
    let decision = prefilter
        .prefilter(&pseudonym, Some(&caller()?), &candidates()?, soon())
        .await;
    assert!(
        matches!(decision, ConsentDecision::NoSignal),
        "{decision:?}"
    );
    assert!(
        mitz.questions().await.is_empty(),
        "Mitz is asked by BSN only"
    );
    Ok(())
}

#[tokio::test]
async fn a_passed_deadline_asks_nothing_and_is_unavailable() -> TestResult {
    let mitz = Mitz::start().await;
    let holders = [
        ("node-a", Some("ura-test-0001")),
        ("node-b", Some("ura-test-0002")),
    ];
    let prefilter = MitzPrefilter::from_config(config(&mitz.endpoint(), &holders)?, &registry())?;
    let decision = prefilter
        .prefilter(
            &patient(BSN_ALIAS)?,
            Some(&caller()?),
            &candidates()?,
            Instant::now(),
        )
        .await;
    assert!(
        matches!(
            decision,
            ConsentDecision::Unavailable(ConsentError::DeadlineExceeded)
        ),
        "{decision:?}"
    );
    assert!(mitz.questions().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_refused_answer_carries_mitz_status() -> TestResult {
    let mitz = Mitz::start().await;
    mitz.refuse();
    let holders = [
        ("node-a", Some("ura-test-0001")),
        ("node-b", Some("ura-test-0002")),
    ];
    let prefilter = MitzPrefilter::from_config(config(&mitz.endpoint(), &holders)?, &registry())?;
    let decision = prefilter
        .prefilter(
            &patient(BSN_ALIAS)?,
            Some(&caller()?),
            &candidates()?,
            soon(),
        )
        .await;
    match decision {
        ConsentDecision::Unavailable(error) => {
            assert_eq!(Some(http::StatusCode::SERVICE_UNAVAILABLE), error.status());
            assert!(!format!("{error} {error:?}").contains(PATIENT_VALUE));
            Ok(())
        }
        other => Err(format!("a 503 is no decision: {other:?}").into()),
    }
}

#[test]
fn the_nvi_custodians_give_a_holder_its_ura() -> TestResult {
    let mut config = config(
        "https://mitz.example.org/vraag",
        &[("node-a", None), ("node-b", Some("ura-test-0002"))],
    )?;
    config
        .custodians
        .insert(String::from("ura-test-0001"), member("node-a")?);
    MitzPrefilter::from_config(config, &registry())?;
    Ok(())
}

#[test]
fn a_ura_the_custodians_contradict_refuses_to_build() -> TestResult {
    let mut config = config(
        "https://mitz.example.org/vraag",
        &[
            ("node-a", Some("ura-test-0001")),
            ("node-b", Some("ura-test-0002")),
        ],
    )?;
    config
        .custodians
        .insert(String::from("ura-test-0009"), member("node-a")?);
    match MitzPrefilter::from_config(config, &registry()) {
        Err(MitzConfigError::UraDisagrees(disagreed)) => {
            assert_eq!(member("node-a")?, disagreed);
            Ok(())
        }
        other => Err(format!("the sources must agree: {other:?}").into()),
    }
}

/// The refusal of the configuration `edit` makes of a valid one.
fn refused(edit: impl FnOnce(&mut MitzConfig)) -> Result<MitzConfigError, Box<dyn Error>> {
    let mut config = config(
        "https://mitz.example.org/vraag",
        &[
            ("node-a", Some("ura-test-0001")),
            ("node-b", Some("ura-test-0002")),
        ],
    )?;
    edit(&mut config);
    MitzPrefilter::from_config(config, &registry())
        .err()
        .ok_or_else(|| "the configuration is refused".into())
}

#[test]
fn every_refused_configuration_is_typed() -> TestResult {
    assert!(matches!(
        refused(|config| {
            config.endpoint = SecretUrl::new("http://mitz.example.org/vraag");
            config.development = false;
        })?,
        MitzConfigError::Client(_)
    ));
    assert!(matches!(
        refused(|config| {
            config.namespaces.insert(
                IdentifierNamespace::new("http://fhir.nl/fhir/NamingSystem/pseudo-bsn")
                    .expect("a namespace"),
            );
        })?,
        MitzConfigError::PseudonymAsBsn(_)
    ));
    assert!(matches!(
        refused(|config| config.purpose = String::from("ETREAT"))?,
        MitzConfigError::Purpose(_)
    ));
    assert!(matches!(
        refused(|config| config.categories.push(String::from("GGC002")))?,
        MitzConfigError::Categories(_)
    ));
    assert!(matches!(
        refused(|config| {
            config
                .holders
                .remove(&"node-b".parse::<NodeId>().expect("an id"));
        })?,
        MitzConfigError::NoHolder(_)
    ));
    assert!(matches!(
        refused(|config| {
            if let Some(holder) = config.holders.values_mut().next() {
                holder.kind = String::new();
            }
        })?,
        MitzConfigError::Holder { .. }
    ));
    Ok(())
}
