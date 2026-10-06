// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An access record written as a BALP `AuditEvent` (BALP 1.1.4): the pattern
//! of its action, the person with the professional, the assurance level and
//! who acts, the client and the provider, the patient, and the Annex II 3.2
//! points in entities no pattern slice bounds; the patient and the person
//! reach the record and no `Debug`.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the written record is read as a JSON value"
)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

use ehds_logging::balp::{BalpSink, EMERGENCY_DESCRIPTION, EMERGENCY_ENTITY, exchange};
use ehds_logging::classify::{Basis, Evidence, RootObject};
use ehds_logging::emergency::EmergencyPurposes;
use ehds_logging::record::{
    AccessRecord, Accessor, Acting, Action, AssuranceLevel, Coded, DataSubject, EhrAt, Origin,
    Outcome, PatientIdentifier, PatientLookup, Professional, Purpose, Relayed, RelayedProfessional,
    RelayedProvider, Request,
};
use ehds_logging::retention::RetentionPolicy;
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
const NAME: &str = "Qz7-name-47";
const IDENTIFIER: &str = "Qz7-identifier-48";
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";
const QUERY: &str = "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE \
                     e/ehr_status/subject/external_ref/id/value = 'Qz7-patient-41'";

fn record(action: Action, outcome: Outcome) -> AccessRecord {
    let categories = map().classify(&Evidence::reached(
        Basis::Returned,
        vec![RootObject {
            template_id: Some(LAB_REPORT.to_owned()),
            ..RootObject::default()
        }],
    ));
    let retention =
        RetentionPolicy::default().retain(jiff::Timestamp::UNIX_EPOCH, &categories, ["node-a"]);
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
            acting: Acting::Person,
            assurance: Some(AssuranceLevel::Substantial),
            professional: Professional {
                name: Some(NAME.to_owned()),
                identifier: Some(IDENTIFIER.to_owned()),
            },
            alt_id: Some(PROFESSIONAL.to_owned()),
            purposes: vec![Purpose {
                system: Some("http://terminology.hl7.org/CodeSystem/v3-ActReason".to_owned()),
                code: "TREAT".to_owned(),
            }],
            relayed: None,
        },
        subject: DataSubject {
            patient: Some(PatientIdentifier {
                namespace: "urn:oid:2.999.1".to_owned(),
                value: SecretString::from(PATIENT),
            }),
            ehrs: vec![EhrAt {
                endpoint: "node-a".to_owned(),
                ehr_id: EHR_ID.to_owned(),
                patient: PatientLookup::RequestNamed,
            }],
        },
        categories,
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
            correlation: None,
        },
        retention,
        emergency: None,
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

/// A routed read by `ehr_id` whose patient the identity service found:
/// `found` patients, the `ehr_id` at `node-a`.
fn read_by_ehr_id(found: PatientLookup) -> AccessRecord {
    let mut record = record(Action::Read, Outcome::Success);
    record.subject.patient = None;
    record.subject.ehrs = vec![EhrAt {
        endpoint: "node-a".to_owned(),
        ehr_id: EHR_ID.to_owned(),
        patient: found,
    }];
    record
}

fn patient(namespace: &str, value: &str) -> PatientIdentifier {
    PatientIdentifier {
        namespace: namespace.to_owned(),
        value: SecretString::from(value),
    }
}

/// The `entity:patient` identifier values of `written`.
fn patient_values(written: &Value) -> Vec<&str> {
    written["entity"]
        .as_array()
        .expect("entities")
        .iter()
        .filter(|entity| entity["role"]["code"] == "1" && entity["type"]["code"] == "1")
        .filter_map(|entity| entity["what"]["identifier"]["value"].as_str())
        .collect()
}

/// Regulation (EU) 2025/327 Art 9(1); IHE `RESTful` ATNA §3.81.4.1.2.2: the
/// patient found behind the `ehr_id` is written as the request's patient is,
/// so a `patient.identifier` search finds the access, and the read claims
/// the `PatientRead` pattern.
#[test]
fn a_patient_found_behind_the_ehr_id_is_the_records_patient() {
    let written = written(&read_by_ehr_id(PatientLookup::Found(vec![patient(
        "urn:oid:2.999.1",
        PATIENT,
    )])));
    assert_eq!(patient_values(&written), [PATIENT]);
    let patients: Vec<&Value> = written["entity"]
        .as_array()
        .expect("entities")
        .iter()
        .filter(|entity| entity["what"]["identifier"]["value"] == PATIENT)
        .collect();
    assert_eq!(
        patients[0]["what"]["identifier"]["system"],
        "urn:oid:2.999.1"
    );
    assert_eq!(
        profile(&written),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientRead")
    );
    let ehrs = entity(&written, "ehr");
    assert_eq!(details(ehrs[0], "patient-lookup"), ["found"]);
}

/// A patient the service names in two namespaces is written twice, and the
/// read claims the plain pattern, whose slices bound no patient.
#[test]
fn several_patients_are_each_written_under_the_plain_pattern() {
    let written = written(&read_by_ehr_id(PatientLookup::Found(vec![
        patient("urn:oid:2.999.1", PATIENT),
        patient("urn:oid:2.999.2", "Qz7-other-61"),
        patient("urn:oid:2.999.1", PATIENT),
    ])));
    let mut values = patient_values(&written);
    values.sort_unstable();
    assert_eq!(values, ["Qz7-other-61", PATIENT]);
    assert_eq!(
        profile(&written),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Read")
    );
}

/// A patient not named is said so on the `ehr` entity, which keeps the
/// `ehr_id` an `entity.identifier` search finds; the access is recorded all
/// the same.
#[test]
fn a_patient_not_named_is_said_so_beside_the_ehr_id() {
    for (lookup, code) in [
        (PatientLookup::NotFound, "not-found"),
        (PatientLookup::Unavailable, "unavailable"),
        (PatientLookup::NotConfigured, "not-configured"),
        (PatientLookup::Unsupported, "unsupported"),
    ] {
        let written = written(&read_by_ehr_id(lookup));
        assert!(patient_values(&written).is_empty(), "{code}");
        let ehrs = entity(&written, "ehr");
        let [ehr] = ehrs.as_slice() else {
            panic!("one ehr entity, got {ehrs:?}");
        };
        assert_eq!(ehr["what"]["identifier"]["value"], EHR_ID, "{code}");
        assert_eq!(details(ehr, "patient-lookup"), [code]);
        assert_eq!(
            profile(&written),
            Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Read"),
            "{code}"
        );
    }
}

#[test]
fn a_patient_lookup_shows_no_identifier_in_debug() {
    let lookup = PatientLookup::Found(vec![patient("urn:oid:2.999.1", PATIENT)]);
    let shown = format!(
        "{lookup:?} {:?}",
        read_by_ehr_id(lookup.clone()).subject.ehrs
    );
    assert!(!shown.contains(PATIENT), "{shown}");
    assert!(!shown.contains(EHR_ID), "{shown}");
    assert!(shown.contains("Found(1)"), "{shown}");
}

/// Regulation (EU) 2025/327 Art 9(2) and Annex II 3.4: the record states the
/// years it is kept, the first date it may be deleted on and what called for
/// the period, so the repository that stores it can apply it.
#[test]
fn the_record_states_how_long_it_is_kept() {
    let written = written(&record(Action::Query, Outcome::Success));
    let categories = entity(&written, "ehds-categories");
    let [categories] = categories.as_slice() else {
        panic!("one categories entity, got {categories:?}");
    };
    assert_eq!(details(categories, "ehds-retention-years"), ["3"]);
    assert_eq!(details(categories, "ehds-retention-ends"), ["1973-01-02"]);
    assert_eq!(details(categories, "ehds-retention-ground"), ["default"]);
}

/// The `agent:user` of `written`.
fn person(written: &Value) -> &Value {
    written["agent"]
        .as_array()
        .expect("agents")
        .iter()
        .find(|agent| agent["type"]["coding"][0]["code"] == "IRCP")
        .expect("agent:user")
}

/// The extensions of `agent` whose `url` ends in `name`.
fn extensions<'a>(agent: &'a Value, name: &str) -> Vec<&'a Value> {
    let url = format!("https://profiles.ihe.net/ITI/BALP/StructureDefinition/{name}");
    agent["extension"]
        .as_array()
        .map(|extensions| {
            extensions
                .iter()
                .filter(|extension| extension["url"] == url.as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// Annex II 3.2(b) and BALP 1.1.4 §3:5.7.5.4: the person's agent names the
/// professional by `who.display` and an `ihe-otherId` typed `NPI`, the
/// assurance level in `ihe-assuranceLevel`, and who acts as its `role`.
#[test]
fn the_persons_agent_carries_the_professional_the_level_and_who_acts() {
    let written = written(&record(Action::Query, Outcome::Success));
    let person = person(&written);
    assert_eq!(person["who"]["identifier"]["value"], SUBJECT);
    assert_eq!(person["who"]["display"], NAME);
    let identifiers = extensions(person, "ihe-otherId");
    let [identifier] = identifiers.as_slice() else {
        panic!("one otherId, got {identifiers:?}");
    };
    assert_eq!(identifier["valueIdentifier"]["value"], IDENTIFIER);
    assert_eq!(
        identifier["valueIdentifier"]["type"]["coding"][0]["code"],
        "NPI"
    );
    let levels = extensions(person, "ihe-assuranceLevel");
    let [level] = levels.as_slice() else {
        panic!("one assurance level, got {levels:?}");
    };
    assert_eq!(
        level["valueCodeableConcept"]["coding"][0]["code"], "substantial",
        "Regulation (EU) No 910/2014 Art 8(2)(b)"
    );
    assert_eq!(person["role"][0]["coding"][0]["code"], "person");
}

#[test]
fn a_client_acting_for_the_professional_is_written_as_a_client() {
    let mut record = record(Action::Query, Outcome::Success);
    record.accessor.acting = Acting::Client;
    let written = written(&record);
    let person = person(&written);
    assert_eq!(person["role"][0]["coding"][0]["code"], "client");
    assert_eq!(
        person["who"]["display"], NAME,
        "the professional it acts for"
    );
}

#[test]
fn an_access_with_no_established_level_writes_none() {
    let mut record = record(Action::Query, Outcome::Success);
    record.accessor.assurance = None;
    record.accessor.professional = Professional::default();
    let written = written(&record);
    let person = person(&written);
    assert!(
        person.get("extension").is_none(),
        "no level and no identifier is inferred: {person}"
    );
    assert!(person["who"].get("display").is_none(), "{person}");
    assert_eq!(person["role"][0]["coding"][0]["code"], "person");
}

/// N33 and Annex II 3.2(b): the professional and the level are in the
/// person's agent and in no other part of the record.
#[test]
fn no_other_part_of_the_record_carries_the_professional_or_the_level() {
    let mut written = written(&record(Action::Query, Outcome::Success));
    let agents = written["agent"].as_array_mut().expect("agents");
    agents.retain(|agent| agent["type"]["coding"][0]["code"] != "IRCP");
    let rest = written.to_string();
    for value in [
        NAME,
        IDENTIFIER,
        "substantial",
        "ihe-assuranceLevel",
        "ihe-otherId",
    ] {
        assert!(!rest.contains(value), "{value} outside agent:user: {rest}");
    }
}

/// The synthetic values of a relayed professional and provider.
const CONTACT_POINT: &str = "https://ncp.example.org";
const FAMILY: &str = "Qz7-family-51";
const GIVEN: &str = "Qz7-given-52";
const HP_ID: &str = "Qz7-hp-53";
const HP_AUTHORITY: &str = "Qz7-hp-authority-54";
const ROLE: &str = "Qz7-role-55";
const HCP_ID: &str = "Qz7-hcp-56";
const HCP_AUTHORITY: &str = "Qz7-hcp-authority-57";
const HCP_NAME: &str = "Qz7-hcp-name-58";
const HCP_ADDRESS: &str = "Qz7-hcp-address-59";
const CORRELATION: &str = "Qz7-correlation-60";

/// A record of an access a national contact point relayed, with the
/// correlation identifier it sent.
fn relayed_record() -> AccessRecord {
    let mut record = record(Action::Query, Outcome::Success);
    record.accessor.relayed = Some(Relayed {
        contact_point: CONTACT_POINT.to_owned(),
        country_code: "XA".to_owned(),
        professional: RelayedProfessional {
            family_name: FAMILY.to_owned(),
            given_name: GIVEN.to_owned(),
            identifier: HP_ID.to_owned(),
            issuing_authority: HP_AUTHORITY.to_owned(),
            roles: vec![Coded {
                system: Some("urn:oid:2.999.9".to_owned()),
                code: ROLE.to_owned(),
            }],
        },
        provider: RelayedProvider {
            identifier: HCP_ID.to_owned(),
            issuing_authority: HCP_AUTHORITY.to_owned(),
            name: HCP_NAME.to_owned(),
            address: HCP_ADDRESS.to_owned(),
        },
    });
    record.request.correlation = Some(CORRELATION.to_owned());
    record
}

/// Implementing Regulation (EU) 2026/2099 Art 7, Annex Tables 1 and 2: every
/// relayed attribute is written, beside the contact point that asserted it
/// and the mark that it is asserted.
#[test]
fn a_relayed_professional_and_provider_ride_in_their_own_entity_marked_asserted() {
    let written = written(&relayed_record());
    let relayed = entity(&written, "ehds-relayed");
    let [relayed] = relayed.as_slice() else {
        panic!("one relayed entity, got {relayed:?}");
    };
    assert_eq!(relayed["type"]["code"], "4");
    for (kind, value) in [
        ("contact-point", CONTACT_POINT),
        ("asserted", "true"),
        ("country-code", "XA"),
        ("hp-family-name", FAMILY),
        ("hp-given-name", GIVEN),
        ("hp-identifier", HP_ID),
        ("hp-issuing-authority", HP_AUTHORITY),
        ("provider-identifier", HCP_ID),
        ("provider-issuing-authority", HCP_AUTHORITY),
        ("provider-name", HCP_NAME),
        ("provider-address", HCP_ADDRESS),
    ] {
        assert_eq!(details(relayed, kind), [value], "{kind}");
    }
    assert_eq!(
        details(relayed, "hp-professional-role"),
        [format!("urn:oid:2.999.9|{ROLE}")]
    );
}

/// The correlation identifier the client sent is a detail of the request
/// id's entity, so the record joins the client's own log.
#[test]
fn the_correlation_identifier_rides_with_the_request_id() {
    let written = written(&relayed_record());
    let transaction = written["entity"]
        .as_array()
        .expect("entities")
        .iter()
        .find(|entity| entity["type"]["code"] == "XrequestId")
        .expect("entity:transaction");
    assert_eq!(details(transaction, "correlation-id"), [CORRELATION]);
}

#[test]
fn a_record_no_contact_point_relayed_has_no_relayed_entity() {
    let written = written(&record(Action::Query, Outcome::Success));
    assert!(entity(&written, "ehds-relayed").is_empty());
    assert!(!written.to_string().contains("correlation-id"));
}

#[test]
fn no_debug_shows_a_relayed_value() {
    let record = relayed_record();
    let shown = format!("{record:?} {:?}", exchange(&record, &gateway()));
    for value in [
        FAMILY,
        GIVEN,
        HP_ID,
        HP_AUTHORITY,
        ROLE,
        HCP_ID,
        HCP_AUTHORITY,
        HCP_NAME,
        HCP_ADDRESS,
        CORRELATION,
    ] {
        assert!(!shown.contains(value), "{value} in {shown}");
    }
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
        NAME,
        IDENTIFIER,
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

/// The HL7 v3 `ActReason` code system the purposes of use are drawn from.
const ACT_REASON: &str = "http://terminology.hl7.org/CodeSystem/v3-ActReason";

/// A record of an access whose accessor declared treatment and break the
/// glass, the deployment declaring break the glass an emergency purpose.
fn emergency_record() -> AccessRecord {
    let mut record = record(Action::Query, Outcome::Success);
    let glass = Purpose {
        system: Some(ACT_REASON.to_owned()),
        code: "BTG".to_owned(),
    };
    record.accessor.purposes.push(glass.clone());
    let declared = EmergencyPurposes::declare(&[glass]).expect("the declared purposes");
    record.emergency = declared.mark(&record.accessor.purposes);
    record
}

/// Regulation (EU) 2025/327 Art 11(5): an emergency access is marked in an
/// entity of its own, stated in words and naming the purpose that marked
/// it, and its purposes stay in `agent:user` `purposeOfUse`, where BALP
/// 1.1.4 puts every purpose.
#[test]
fn an_emergency_access_is_marked_in_its_own_entity() {
    let written = written(&emergency_record());
    let marks = entity(&written, EMERGENCY_ENTITY);
    let [mark] = marks.as_slice() else {
        panic!("one emergency entity: {written}");
    };
    assert_eq!(mark["type"]["code"], "4");
    assert_eq!(mark["description"], EMERGENCY_DESCRIPTION);
    assert_eq!(details(mark, "ehds-emergency-access"), ["true"]);
    assert_eq!(
        details(mark, "ehds-emergency-purpose"),
        [format!("{ACT_REASON}|BTG")],
        "the purpose that marked it, and not TREAT"
    );
    let codes: Vec<&str> = person(&written)["purposeOfUse"]
        .as_array()
        .expect("purposeOfUse")
        .iter()
        .filter_map(|purpose| purpose["coding"][0]["code"].as_str())
        .collect();
    assert_eq!(codes, ["TREAT", "BTG"], "BALP agent:user purposeOfUse");
    assert_eq!(
        profile(&written),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientQuery"),
        "the mark changes no pattern"
    );
}

/// A record with no mark has no emergency entity.
#[test]
fn an_access_with_no_mark_has_no_emergency_entity() {
    let written = written(&record(Action::Query, Outcome::Success));
    assert!(entity(&written, EMERGENCY_ENTITY).is_empty(), "{written}");
    assert!(!written.to_string().contains("ehds-emergency"), "{written}");
}

/// A refused emergency access is marked too: the mark records what the
/// accessor asserted, whatever the origins answered.
#[test]
fn a_refused_emergency_access_is_still_marked() {
    let mut record = emergency_record();
    record.outcome = Outcome::MinorFailure;
    let written = written(&record);
    assert_eq!(written["outcome"], "4");
    assert_eq!(entity(&written, EMERGENCY_ENTITY).len(), 1, "{written}");
}
