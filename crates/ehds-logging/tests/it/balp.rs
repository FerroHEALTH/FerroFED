// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An access record written as a BALP `AuditEvent` (BALP 1.1.4): the pattern
//! of its action, the person, the client and the provider, the patient, and
//! the Annex II 3.2 points in entities no pattern slice bounds; the patient
//! and the person reach the record and no `Debug`.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the written record is read as a JSON value"
)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

use ehds_logging::balp::{BalpSink, exchange};
use ehds_logging::classify::{Basis, Evidence, RootObject};
use ehds_logging::record::{
    AccessRecord, Accessor, Action, DataSubject, EhrAt, Origin, Outcome, PatientIdentifier,
    Purpose, Request,
};
use ehds_logging::sink::AccessSink;
use ihe_iti::balp::{AuditError, AuditRecorder, Exchange, NetworkAddress, Observer};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::Value;
use url::Url;

use super::support::{LAB_REPORT, map};

/// The synthetic values a record carries, each unlike any other text.
const PATIENT: &str = "Qz7-patient-41";
const SUBJECT: &str = "Qz7-subject-42";
const CLIENT: &str = "Qz7-client-43";
const PROVIDER: &str = "Qz7-provider-44";
const PROFESSIONAL: &str = "Qz7-professional-45";
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";
const QUERY: &str = "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE \
                     e/ehr_status/subject/external_ref/id/value = 'Qz7-patient-41'";

fn record(action: Action, outcome: Outcome) -> AccessRecord {
    AccessRecord {
        action,
        recorded: jiff::Timestamp::UNIX_EPOCH,
        outcome,
        accessor: Accessor {
            issuer: "https://issuer.example.org".to_owned(),
            subject: SUBJECT.to_owned(),
            client_id: CLIENT.to_owned(),
            audience: Some("https://gateway.example.org".to_owned()),
            provider: Some(PROVIDER.to_owned()),
            professional: Some(PROFESSIONAL.to_owned()),
            purposes: vec![Purpose {
                system: Some("http://terminology.hl7.org/CodeSystem/v3-ActReason".to_owned()),
                code: "TREAT".to_owned(),
            }],
        },
        subject: DataSubject {
            patient: Some(PatientIdentifier {
                namespace: "urn:oid:2.999.1".to_owned(),
                value: SecretString::from(PATIENT),
            }),
            ehrs: vec![EhrAt {
                endpoint: "node-a".to_owned(),
                ehr_id: EHR_ID.to_owned(),
            }],
        },
        categories: map().classify(&Evidence::reached(
            Basis::Returned,
            vec![RootObject {
                template_id: Some(LAB_REPORT.to_owned()),
                ..RootObject::default()
            }],
        )),
        delivered: Some(1),
        origins: vec![Origin {
            endpoint: "node-a".to_owned(),
            node: Some("cdr-a".to_owned()),
            system_id: Some("cdr-a.example.org".to_owned()),
            status: "active".to_owned(),
            rows: Some(1),
            categories: None,
        }],
        request: Request {
            id: "Qz7-request-46".to_owned(),
            operation: "query_execute_adhoc_query".to_owned(),
            resource: None,
            query: Some(SecretString::from(QUERY)),
            stored_query: None,
            client_address: Some(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 7))),
        },
    }
}

fn gateway() -> Url {
    Url::parse("https://gateway.example.org/").expect("a URL")
}

fn written(record: &AccessRecord) -> Value {
    let observer = Observer {
        source_id: "gateway.example.org".to_owned(),
        site: None,
        host: NetworkAddress::host("gateway.example.org"),
    };
    let bytes = exchange(record, &gateway())
        .audit_event(&observer)
        .expect("a record")
        .into_bytes();
    serde_json::from_slice(bytes.expose_secret()).expect("JSON")
}

fn profile(record: &Value) -> Option<&str> {
    record["meta"]["profile"][0].as_str()
}

fn entity<'a>(record: &'a Value, name: &str) -> Vec<&'a Value> {
    record["entity"]
        .as_array()
        .expect("entities")
        .iter()
        .filter(|entity| entity["name"] == name)
        .collect()
}

fn details<'a>(entity: &'a Value, kind: &str) -> Vec<&'a str> {
    entity["detail"]
        .as_array()
        .map(|details| {
            details
                .iter()
                .filter(|detail| detail["type"] == kind)
                .filter_map(|detail| detail["valueString"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_patient_query_is_the_patient_query_pattern_with_its_query() {
    let written = written(&record(Action::Query, Outcome::Success));
    assert_eq!(
        profile(&written),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientQuery")
    );
    assert_eq!(written["action"], "E");
    assert_eq!(written["subtype"][0]["code"], "search");
    let entities = written["entity"].as_array().expect("entities");
    assert!(
        entities
            .iter()
            .any(|entity| entity["role"]["code"] == "24" && entity["query"].is_string()),
        "entity:query"
    );
    assert!(
        entities
            .iter()
            .any(|entity| entity["what"]["identifier"]["value"] == PATIENT),
        "entity:patient"
    );
    assert!(
        entities
            .iter()
            .any(|entity| entity["type"]["code"] == "XrequestId"),
        "entity:transaction"
    );
}

#[test]
fn each_action_is_written_as_its_pattern() {
    for (action, code, subtype, pattern) in [
        (Action::Read, "R", "read", "PatientRead"),
        (Action::Create, "C", "create", "PatientCreate"),
        (Action::Update, "U", "update", "PatientUpdate"),
        (Action::Delete, "D", "delete", "PatientDelete"),
    ] {
        let written = written(&record(action, Outcome::Success));
        assert_eq!(written["action"], code, "{action:?}");
        assert_eq!(written["subtype"][0]["code"], subtype, "{action:?}");
        assert_eq!(
            profile(&written).map(|url| url.rsplit('.').next()),
            Some(Some(pattern)),
            "{action:?}"
        );
    }
}

#[test]
fn a_failed_delete_claims_no_pattern() {
    let written = written(&record(Action::Delete, Outcome::MinorFailure));
    assert!(
        written.get("meta").is_none(),
        "the Delete pattern fixes outcome 0"
    );
    assert_eq!(written["outcome"], "4");
}

#[test]
fn a_delete_names_one_application_agent() {
    let written = written(&record(Action::Delete, Outcome::Success));
    let applications = written["agent"]
        .as_array()
        .expect("agents")
        .iter()
        .filter(|agent| agent["type"]["coding"][0]["code"] == "110150")
        .count();
    assert_eq!(applications, 1, "IHE.BasicAudit.Delete agent:client 1..1");
}

#[test]
fn the_person_the_provider_and_the_client_are_named() {
    let written = written(&record(Action::Read, Outcome::Success));
    let agents = written["agent"].as_array().expect("agents");
    let person = agents
        .iter()
        .find(|agent| agent["type"]["coding"][0]["code"] == "IRCP")
        .expect("agent:user");
    assert_eq!(person["who"]["identifier"]["value"], SUBJECT);
    assert_eq!(person["altId"], PROFESSIONAL);
    assert_eq!(person["purposeOfUse"][0]["coding"][0]["code"], "TREAT");
    assert!(
        agents
            .iter()
            .any(|agent| agent["who"]["identifier"]["value"] == PROVIDER),
        "Annex II 3.2(a): the provider"
    );
    assert!(
        agents
            .iter()
            .any(|agent| agent["network"]["address"] == "192.0.2.7"),
        "the client's address"
    );
}

#[test]
fn the_categories_and_the_origins_ride_in_their_own_entities() {
    let written = written(&record(Action::Query, Outcome::Success));
    let categories = entity(&written, "ehds-categories");
    let [categories] = categories.as_slice() else {
        panic!("one categories entity, got {categories:?}");
    };
    assert_eq!(categories["type"]["code"], "4");
    assert_eq!(
        details(categories, "ehds-category"),
        ["medical-test-result"]
    );
    assert_eq!(
        details(categories, "ehds-category-basis"),
        ["medical-test-result:returned"]
    );
    assert_eq!(details(categories, "template-id"), [LAB_REPORT]);
    assert_eq!(details(categories, "category-map-digest"), ["sha256:test"]);
    assert_eq!(details(categories, "delivered"), ["1"]);
    let origins = entity(&written, "origin");
    let [origin] = origins.as_slice() else {
        panic!("one origin, got {origins:?}");
    };
    assert_eq!(origin["what"]["identifier"]["value"], "node-a");
    assert_eq!(details(origin, "node"), ["cdr-a"]);
    assert_eq!(details(origin, "status"), ["active"]);
    let ehrs = entity(&written, "ehr");
    assert_eq!(ehrs.len(), 1);
    assert_eq!(ehrs[0]["what"]["identifier"]["value"], EHR_ID);
}

#[test]
fn no_debug_shows_the_patient_the_person_or_an_id() {
    let record = record(Action::Query, Outcome::Success);
    let shown = format!("{record:?} {:?}", exchange(&record, &gateway()));
    for value in [
        PATIENT,
        SUBJECT,
        CLIENT,
        PROVIDER,
        PROFESSIONAL,
        LAB_REPORT,
        EHR_ID,
    ] {
        assert!(!shown.contains(value), "{value} in {shown}");
    }
}

/// A recorder that keeps every exchange, or refuses every one.
#[derive(Default)]
struct Kept {
    refuse: bool,
    kept: Mutex<Vec<Exchange>>,
}

#[derive(Debug, thiserror::Error)]
#[error("the spool is full")]
struct Full;

#[async_trait::async_trait]
impl AuditRecorder for Kept {
    async fn record(&self, exchange: Exchange) -> Result<(), AuditError> {
        if self.refuse {
            return Err(AuditError(Box::new(Full)));
        }
        self.kept.lock().expect("kept").push(exchange);
        Ok(())
    }
}

#[tokio::test]
async fn the_sink_hands_the_record_to_the_recorder_and_reports_its_refusal() {
    let kept = Arc::new(Kept::default());
    let recorder: Arc<dyn AuditRecorder> = kept.clone();
    let sink = BalpSink::new(recorder, gateway());
    sink.store(record(Action::Read, Outcome::Success))
        .await
        .expect("stored");
    assert_eq!(kept.kept.lock().expect("kept").len(), 1);
    let refusing = BalpSink::new(
        Arc::new(Kept {
            refuse: true,
            ..Kept::default()
        }),
        gateway(),
    );
    assert!(
        refusing
            .store(record(Action::Read, Outcome::Success))
            .await
            .is_err(),
        "a record the recorder refuses is an error, never a success"
    );
}
