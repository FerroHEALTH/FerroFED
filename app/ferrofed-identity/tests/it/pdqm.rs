// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PDQm demographics step against the harness PDQm Supplier and stub
//! Suppliers: one master identity resolved as the PIXm resolver resolves the
//! client's own identifier, no match, several matches refused, an outage
//! reported, the exchange audited, and neither identifier anywhere but the
//! Supplier and the PIX Manager (Annex A §A.2 and §A.7, §5.2, §5.4.1, N3,
//! N33; PDQm §2:3.78.4.1.3, §2:3.119.4.1.3).
#![allow(
    clippy::expect_used,
    reason = "fixture builders fail the test they serve on an impossible value"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ferrofed_identity::fhir::{Authentication, Tls};
use ferrofed_identity::ihe::pdqm::{PdqmConfig, PdqmConfigError, PdqmDemographics, Transaction};
use ferrofed_identity::ihe::pixm::{ManagerConfig, PixmResolver};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::demographics::{
    Ambiguity, Demographics, DemographicsError, Identification,
};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_identity::role::resolver::{Resolution, Resolver};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdq::PdqSupplier;
use ihe_iti::balp::{AuditError, AuditRecorder, Exchange};
use ihe_iti::pixm::Invocation;
use secrecy::SecretString;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use crate::support::registry;

type TestResult = Result<(), Box<dyn Error>>;

/// The client's identifier, in a namespace the cross-reference does not map.
const CLIENT_ID: &str = "SENTINEL-LOCAL-487";
const LOCAL_NAMESPACE: &str = "2.999.7";
const LOCAL_SYSTEM: &str = "urn:oid:2.999.7";

/// The master domain and the master identifier the Supplier knows the
/// patient under.
const MASTER: &str = "urn:oid:2.999.1";
const MASTER_ID: &str = "SENTINEL-MASTER-487";

/// The `ehr_id` domains of node A and node B at the PIX Manager.
const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";
const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";

const FHIR_JSON: &str = "application/fhir+json";
const MATCH: &str = "/fhir/Patient/$match";

fn client_patient() -> PatientRef {
    PatientRef::new(
        IdentifierNamespace::new(LOCAL_NAMESPACE).expect("a namespace"),
        SecretString::from(CLIENT_ID),
    )
    .expect("a patient reference")
}

fn config(base: &str, transaction: Transaction) -> PdqmConfig {
    PdqmConfig {
        tls: Tls::default(),
        base: SecretUrl::new(base.to_owned()),
        auth: Authentication::None,
        transaction,
        master: MASTER.to_owned(),
        namespaces: BTreeMap::from([(
            IdentifierNamespace::new(LOCAL_NAMESPACE).expect("a namespace"),
            LOCAL_SYSTEM.to_owned(),
        )]),
    }
}

fn step(base: &str, transaction: Transaction) -> PdqmDemographics {
    PdqmDemographics::from_config(config(base, transaction)).expect("a demographics step")
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

/// A harness Supplier knowing the patient under the client's identifier and
/// the master identifier.
async fn supplier() -> Result<PdqSupplier, Box<dyn Error>> {
    let supplier = PdqSupplier::start().await?;
    supplier.add(&[(LOCAL_SYSTEM, CLIENT_ID), (MASTER, MASTER_ID)], true)?;
    Ok(supplier)
}

/// A stub Supplier answering every `$match` with `status` and `body`.
async fn matcher(status: u16, body: String) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path(MATCH))
        .respond_with(ResponseTemplate::new(status).set_body_raw(body.into_bytes(), FHIR_JSON))
        .mount(&server)
        .await;
    server
}

/// A `$match` answer of one Patient entry per `(master values, grade)`.
fn match_answer(entries: &[(&[&str], &str)]) -> String {
    let entries: Vec<String> = entries
        .iter()
        .enumerate()
        .map(|(index, (values, grade))| {
            let identifiers: Vec<String> = values
                .iter()
                .map(|value| format!(r#"{{"system":"{MASTER}","value":"{value}"}}"#))
                .collect();
            format!(
                r#"{{"fullUrl":"http://pdq.example.org/fhir/Patient/p{index}","resource":{{"resourceType":"Patient","id":"p{index}","identifier":[{}]}},"search":{{"mode":"match","score":0.9,"extension":[{{"url":"http://hl7.org/fhir/StructureDefinition/match-grade","valueCode":"{grade}"}}]}}}}"#,
                identifiers.join(",")
            )
        })
        .collect();
    format!(
        r#"{{"resourceType":"Bundle","type":"searchset","entry":[{}]}}"#,
        entries.join(",")
    )
}

/// The master identity of `identification`, or an error naming what it was
/// instead.
fn identified(identification: Identification) -> Result<PatientRef, String> {
    match identification {
        Identification::Identified(master) => Ok(master),
        other => Err(format!("one master identity, got {other:?}")),
    }
}

#[tokio::test]
async fn the_master_identity_of_an_iti_78_search_is_resolved_by_the_pix_manager() -> TestResult {
    let supplier = supplier().await?;
    let master = identified(
        step(&supplier.base_url(), Transaction::Search)
            .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
            .await,
    )?;
    assert_eq!(MASTER, master.namespace().as_str());
    let pix = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$ihe-pix"))
        .and(query_param("sourceIdentifier", format!("{MASTER}|{MASTER_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{EHR_A}"}}}}]}}"#
            )
            .into_bytes(),
            FHIR_JSON,
        ))
        .expect(1)
        .mount(&pix)
        .await;
    let resolver = PixmResolver::from_config(
        vec![ManagerConfig {
            tls: Tls::default(),
            base: SecretUrl::new(format!("{}/fhir/", pix.uri())),
            auth: Authentication::None,
            members: BTreeMap::from([
                (NodeId::new("node-a")?, DOMAIN_A.to_owned()),
                (NodeId::new("node-b")?, DOMAIN_B.to_owned()),
            ]),
            invocation: Invocation::Get,
        }],
        BTreeMap::new(),
        &registry(),
    )?;
    let members = [NodeId::new("node-a")?, NodeId::new("node-b")?];
    let resolutions = resolver
        .resolve(&master, &members, &OnBehalfOf::Gateway, soon())
        .await;
    assert!(matches!(
        resolutions.get(&members[0]),
        Some(Resolution::Resolved(ehr_id)) if ehr_id.as_str() == EHR_A
    ));
    assert!(matches!(
        resolutions.get(&members[1]),
        Some(Resolution::Unknown)
    ));
    let bodies = supplier.bodies();
    assert_eq!(1, supplier.searches());
    assert!(
        bodies[0].contains(CLIENT_ID),
        "the client's identifier reaches the Supplier"
    );
    assert!(
        bodies[0].contains(&format!("identifier={}", "urn%3Aoid%3A2.999.1%7C")),
        "the search asks for the master domain only (§2:3.78.4.1.2.3): {}",
        bodies[0]
    );
    Ok(())
}

#[tokio::test]
async fn an_iti_119_certain_match_names_the_master_identity() -> TestResult {
    let supplier = supplier().await?;
    let master = identified(
        step(&supplier.base_url(), Transaction::Match)
            .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
            .await,
    )?;
    assert_eq!(MASTER, master.namespace().as_str());
    assert_eq!(1, supplier.matches());
    assert!(
        supplier.bodies()[0].contains(r#""name":"onlyCertainMatches","valueBoolean":true"#),
        "§2:3.119.4.1.2: {}",
        supplier.bodies()[0]
    );
    Ok(())
}

#[tokio::test]
async fn an_identifier_the_supplier_does_not_know_is_no_match() -> TestResult {
    let supplier = PdqSupplier::start().await?;
    supplier.add(
        &[(LOCAL_SYSTEM, "SENTINEL-OTHER"), (MASTER, MASTER_ID)],
        true,
    )?;
    for transaction in [Transaction::Search, Transaction::Match] {
        let answer = step(&supplier.base_url(), transaction)
            .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
            .await;
        assert!(
            matches!(answer, Identification::NoMatch),
            "{transaction}: {answer:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_match_without_a_master_identifier_is_no_match() -> TestResult {
    let supplier = PdqSupplier::start().await?;
    supplier.add(
        &[(LOCAL_SYSTEM, CLIENT_ID), ("urn:oid:2.999.3", "X-1")],
        true,
    )?;
    supplier.add(&[(MASTER, "SENTINEL-UNRELATED")], true)?;
    let answer = step(&supplier.base_url(), Transaction::Search)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    assert!(matches!(answer, Identification::NoMatch), "{answer:?}");
    Ok(())
}

#[tokio::test]
async fn several_matched_patients_are_refused_and_none_is_picked() -> TestResult {
    let supplier = supplier().await?;
    supplier.add(
        &[(LOCAL_SYSTEM, CLIENT_ID), (MASTER, "SENTINEL-MASTER-2")],
        true,
    )?;
    let answer = step(&supplier.base_url(), Transaction::Search)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    assert!(
        matches!(
            answer,
            Identification::Ambiguous(Ambiguity::SeveralPatients)
        ),
        "§2:3.78.4.1.3 Case 1 counts them in total: {answer:?}"
    );
    let stub = matcher(
        200,
        match_answer(&[
            (&[MASTER_ID], "certain"),
            (&["SENTINEL-MASTER-2"], "certain"),
        ]),
    )
    .await;
    let answer = step(&format!("{}/fhir/", stub.uri()), Transaction::Match)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    assert!(
        matches!(
            answer,
            Identification::Ambiguous(Ambiguity::SeveralPatients)
        ),
        "§2:3.119.4.1.3 Case 2 returns one entry each: {answer:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_deprecated_record_beside_the_active_one_is_no_second_match() -> TestResult {
    let supplier = supplier().await?;
    supplier.add(
        &[(LOCAL_SYSTEM, CLIENT_ID), (MASTER, "SENTINEL-DEPRECATED")],
        false,
    )?;
    let answer = step(&supplier.base_url(), Transaction::Search)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    let master = identified(answer)?;
    assert_eq!(MASTER, master.namespace().as_str());
    Ok(())
}

#[tokio::test]
async fn a_patient_with_two_master_identifiers_is_refused() -> TestResult {
    let supplier = PdqSupplier::start().await?;
    supplier.add(
        &[
            (LOCAL_SYSTEM, CLIENT_ID),
            (MASTER, MASTER_ID),
            (MASTER, "SENTINEL-MASTER-2"),
        ],
        true,
    )?;
    let answer = step(&supplier.base_url(), Transaction::Search)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    assert!(
        matches!(
            answer,
            Identification::Ambiguous(Ambiguity::SeveralIdentifiers)
        ),
        "{answer:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_iti_119_match_that_is_not_certain_is_refused() -> TestResult {
    let stub = matcher(200, match_answer(&[(&[MASTER_ID], "probable")])).await;
    let answer = step(&format!("{}/fhir/", stub.uri()), Transaction::Match)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    assert!(
        matches!(answer, Identification::Ambiguous(Ambiguity::Uncertain)),
        "{answer:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_outage_is_unavailable_never_no_match() -> TestResult {
    let unreachable = step(
        &format!("{}/fhir/", ferrofed_testkit::unreachable::BASE),
        Transaction::Search,
    )
    .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
    .await;
    assert!(
        matches!(
            unreachable,
            Identification::Unavailable(DemographicsError::Backend(_))
        ),
        "{unreachable:?}"
    );
    let failing = matcher(503, String::from(r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"transient"}]}"#)).await;
    let answer = step(&format!("{}/fhir/", failing.uri()), Transaction::Match)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    let Identification::Unavailable(error) = answer else {
        panic!("a failure, got {answer:?}");
    };
    assert_eq!(Some(http::StatusCode::SERVICE_UNAVAILABLE), error.status());
    let late = step(&format!("{}/fhir/", failing.uri()), Transaction::Match)
        .identify(&client_patient(), &OnBehalfOf::Gateway, Instant::now())
        .await;
    assert!(
        matches!(
            late,
            Identification::Unavailable(DemographicsError::DeadlineExceeded)
        ),
        "{late:?}"
    );
    Ok(())
}

/// A recorder that keeps every exchange, or refuses every one.
#[derive(Default)]
struct Recorder {
    kept: Mutex<Vec<Exchange>>,
    refuse: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("the spool is full")]
struct Full;

#[async_trait::async_trait]
impl AuditRecorder for Recorder {
    async fn record(&self, exchange: Exchange) -> Result<(), AuditError> {
        if self.refuse {
            return Err(AuditError(Box::new(Full)));
        }
        self.kept.lock().expect("the kept exchanges").push(exchange);
        Ok(())
    }
}

#[tokio::test]
async fn each_exchange_is_audited_and_one_whose_record_is_refused_fails() -> TestResult {
    let supplier = supplier().await?;
    for (transaction, profile) in [
        (
            Transaction::Search,
            ihe_iti::pdqm::audit::QUERY_CONSUMER.profile,
        ),
        (
            Transaction::Match,
            ihe_iti::pdqm::audit::MATCH_CONSUMER.profile,
        ),
    ] {
        let kept = Arc::new(Recorder::default());
        let recorder: Arc<dyn AuditRecorder> = kept.clone();
        let answer = step(&supplier.base_url(), transaction)
            .audited(&recorder)
            .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
            .await;
        identified(answer)?;
        let exchanges = kept.kept.lock().expect("the kept exchanges");
        let [exchange] = exchanges.as_slice() else {
            panic!("{transaction}: one record, got {}", exchanges.len());
        };
        assert_eq!(profile, exchange.kind.profile, "{transaction}");
    }
    let refusing: Arc<dyn AuditRecorder> = Arc::new(Recorder {
        refuse: true,
        ..Recorder::default()
    });
    let answer = step(&supplier.base_url(), Transaction::Search)
        .audited(&refusing)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    assert!(
        matches!(
            answer,
            Identification::Unavailable(DemographicsError::AuditFailed(_))
        ),
        "PDQm §2:3.78.5.1: {answer:?}"
    );
    Ok(())
}

#[test]
fn a_configuration_the_step_cannot_use_is_refused() {
    let base = "https://pdq.example.org/fhir/";
    let mut none = config(base, Transaction::Search);
    none.namespaces.clear();
    assert!(matches!(
        PdqmDemographics::from_config(none),
        Err(PdqmConfigError::NoNamespace)
    ));
    let mut master = config(base, Transaction::Search);
    master.master = String::from("not a uri");
    assert!(matches!(
        PdqmDemographics::from_config(master),
        Err(PdqmConfigError::Master)
    ));
    let mut system = config(base, Transaction::Search);
    system.namespaces = BTreeMap::from([(
        IdentifierNamespace::new(LOCAL_NAMESPACE).expect("a namespace"),
        String::from("not a uri"),
    )]);
    assert!(matches!(
        PdqmDemographics::from_config(system),
        Err(PdqmConfigError::Namespace(_))
    ));
    let mut itself = config(base, Transaction::Search);
    itself.namespaces = BTreeMap::from([(
        IdentifierNamespace::new(MASTER).expect("a namespace"),
        MASTER.to_owned(),
    )]);
    assert!(matches!(
        PdqmDemographics::from_config(itself),
        Err(PdqmConfigError::MasterNamespace(_))
    ));
    assert!(matches!(
        PdqmDemographics::from_config(config("urn:oid:2.999.4", Transaction::Search)),
        Err(PdqmConfigError::Base(_))
    ));
    let handled = step(base, Transaction::Search);
    assert!(handled.handles(&IdentifierNamespace::new(LOCAL_NAMESPACE).expect("a namespace")));
    assert!(!handled.handles(&IdentifierNamespace::new(MASTER).expect("a namespace")));
}

#[tokio::test]
async fn no_rendering_names_the_client_or_the_master_identifier() -> TestResult {
    let supplier = supplier().await?;
    supplier.add(
        &[(LOCAL_SYSTEM, CLIENT_ID), (MASTER, "SENTINEL-MASTER-2")],
        true,
    )?;
    let searching = step(&supplier.base_url(), Transaction::Search);
    let mut text = format!("{searching:?}");
    let answer = searching
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    write!(text, "{answer:?}")?;
    if let Identification::Ambiguous(ambiguity) = &answer {
        text.push_str(&ambiguity.to_string());
    }
    let failing = matcher(503, format!(r#"{{"resourceType":"OperationOutcome","issue":[{{"severity":"error","code":"transient","diagnostics":"{CLIENT_ID} {MASTER_ID}"}}]}}"#)).await;
    let failed = step(&format!("{}/fhir/", failing.uri()), Transaction::Match)
        .identify(&client_patient(), &OnBehalfOf::Gateway, soon())
        .await;
    if let Identification::Unavailable(error) = &failed {
        text.push_str(&chain(error));
    }
    write!(text, "{failed:?}")?;
    for value in [CLIENT_ID, MASTER_ID, "SENTINEL-MASTER-2"] {
        assert!(!text.contains(value), "{value} in {text}");
    }
    Ok(())
}

/// `error` and every error in its source chain, with their `Debug`.
fn chain(error: &dyn Error) -> String {
    let mut text = format!("{error} {error:?}");
    let mut cause = error.source();
    while let Some(inner) = cause {
        let _written: std::fmt::Result = write!(text, " {inner} {inner:?}");
        cause = inner.source();
    }
    text
}
