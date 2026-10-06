// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Who accessed the data, as client authentication verified them
//! (Regulation (EU) 2025/327 Annex II 3.1, 3.2(a), (b)): the record's
//! `agent:user` names the professional by the IHE IUA `subject_name` and
//! `national_provider_identifier` (BALP 1.1.4 §3:5.7.5.4), the assurance
//! level the issuer's declared mapping read from the token (Regulation (EU)
//! No 910/2014 Art 8(2)), and who acts; no other part of the record, and no
//! log line, carries them (§5.4, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;

use ferrofed_engine::conveyance::AssuranceLevel;
use ferrofed_server::config::auth::AuthSettings;
use ferrofed_server::config::auth::assurance::Assurance;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::issuer::Claims;
use http::StatusCode;
use serde_json::Value;

use super::{
    LAB_REPORT, TestResult, accesses, composition, details, gateway_and_state, node_with_rows,
    recorded_the_query,
};
use crate::auth::{bearing, minted, query};
use crate::contact_point;
use crate::facade::{EHR_A, EHR_B, PATIENT, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::{self, Logs, call};

const UID_A: &str = "6a7b8c9d-1e2f-4a3b-8c4d-5e6f7a8b9c0d::cdr-a.example.org::1";

/// The `acr` value the test issuer states for level substantial.
const SUBSTANTIAL: &str = "urn:example:loa:substantial";

/// The `acr` value the test issuer states for level high.
const HIGH: &str = "urn:example:loa:high";

/// A synthetic professional's name (IHE IUA `subject_name`).
const NAME: &str = "Qz7 Example Clinician";

/// A synthetic professional identifier (IHE IUA
/// `national_provider_identifier`).
const IDENTIFIER: &str = "urn:oid:2.999.7.1|hp-0742";

/// The synthetic `client_id` of a national contact point's connector.
const CONNECTOR: &str = "Qz7-national-connector";

/// The suite's `[auth]`, its issuer reading `acr` at the levels above and
/// requiring `minimum`, and declaring its client tokens as acting for a
/// professional when `clients` is set.
fn assured(minimum: AssuranceLevel, clients: bool) -> AuthSettings {
    let mut auth = support::auth();
    for issuer in &mut auth.issuers {
        issuer.assurance = Some(Assurance {
            claim: String::from("acr"),
            minimum,
            values: BTreeMap::from([
                (SUBSTANTIAL.to_owned(), AssuranceLevel::Substantial),
                (HIGH.to_owned(), AssuranceLevel::High),
            ]),
        });
        issuer.client_tokens_act_for_professional = clients;
    }
    auth
}

/// The suite's default claims stating `acr` and naming the professional
/// [`NAME`], [`IDENTIFIER`] in the IUA extension.
fn professional(acr: &str) -> Claims {
    let mut claims = support::claims();
    claims.other.insert(String::from("acr"), acr.to_owned());
    if let Some(extensions) = claims.extensions.as_mut() {
        extensions.ihe_iua.subject_name = Some(NAME.to_owned());
        extensions.ihe_iua.national_provider_identifier = Some(IDENTIFIER.to_owned());
    }
    claims
}

/// `claims` as a client token: its `sub` is its `client_id`.
fn as_client(mut claims: Claims) -> Claims {
    claims.sub.clone_from(&claims.client_id);
    claims
}

/// What one access left behind: its record, the log lines it wrote, and the
/// metrics exposition of the state that served it.
struct Recorded {
    record: Value,
    log: String,
    exposition: String,
}

/// Sends the suite's patient query with a token over `claims` to a gateway
/// authenticating with `auth`, and returns what the access left behind.
async fn recorded(auth: AuthSettings, claims: &Claims) -> Result<Recorded, Box<dyn Error>> {
    let node_a = node_with_rows(&[composition(LAB_REPORT, UID_A)]).await;
    let node_b = node_with_rows(&[]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let mut server = settings_with_room();
    server.auth = auth;
    let (app, state) = gateway_and_state(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", ""),
        &server,
    )?;
    let logs = Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "trace",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app, bearing(query()?, &minted(claims)?)?).await?;
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    let [record] = records.as_slice() else {
        return Err(format!("one record of the access, got {records:?}").into());
    };
    Ok(Recorded {
        record: record.clone(),
        log: logs.text(),
        exposition: state.metrics().render()?,
    })
}

/// Whether `agent` is the BALP `agent:user`, typed `IRCP`.
fn is_person(agent: &Value) -> bool {
    agent
        .pointer("/type/coding/0/code")
        .is_some_and(|code| code == "IRCP")
}

/// The `agent:user` of `record`.
fn person(record: &Value) -> Result<&Value, Box<dyn Error>> {
    record["agent"]
        .as_array()
        .and_then(|agents| agents.iter().find(|agent| is_person(agent)))
        .ok_or_else(|| format!("an agent:user in {record}").into())
}

/// The values of the extensions of `agent` whose `url` ends in `name`, at
/// `pointer` inside each.
fn extension(agent: &Value, name: &str, pointer: &str) -> Vec<String> {
    let url = format!("https://profiles.ihe.net/ITI/BALP/StructureDefinition/{name}");
    agent["extension"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|extension| extension["url"] == url.as_str())
        .filter_map(|extension| extension.pointer(pointer)?.as_str().map(str::to_owned))
        .collect()
}

/// The assurance levels `agent` carries.
fn levels(agent: &Value) -> Vec<String> {
    extension(
        agent,
        "ihe-assuranceLevel",
        "/valueCodeableConcept/coding/0/code",
    )
}

/// The provider identifiers `agent` carries.
fn identifiers(agent: &Value) -> Vec<String> {
    extension(agent, "ihe-otherId", "/valueIdentifier/value")
}

/// Who acts, as `agent`'s `role` says.
fn acting(agent: &Value) -> Option<&str> {
    agent
        .pointer("/role/0/coding/0/code")
        .and_then(Value::as_str)
}

/// The `who.identifier.value` of every agent of `record` but `agent:user`.
fn others(record: &Value) -> Vec<&str> {
    record["agent"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|agent| !is_person(agent))
        .filter_map(|agent| agent.pointer("/who/identifier/value")?.as_str())
        .collect()
}

/// Fails unless `values` appear in `record` only inside its `agent:user`.
fn only_in_the_person(record: &Value, values: &[&str]) {
    let mut rest = record.clone();
    if let Some(agents) = rest.get_mut("agent").and_then(Value::as_array_mut) {
        agents.retain(|agent| !is_person(agent));
    }
    let rest = rest.to_string();
    for value in values {
        assert!(
            !rest.contains(value),
            "N33: {value} outside agent:user: {rest}"
        );
    }
}

/// Annex II 3.2(b): a person's access names the professional and the level
/// their authentication reached, as the token stated them, in the person's
/// agent alone.
#[tokio::test]
async fn a_person_is_recorded_with_the_professional_and_the_verified_level() -> TestResult {
    let claims = professional(SUBSTANTIAL);
    let record = recorded(assured(AssuranceLevel::Substantial, false), &claims)
        .await?
        .record;
    let person = person(&record)?;
    assert_eq!(person["who"]["identifier"]["value"], claims.sub.as_str());
    assert_eq!(person["who"]["display"], NAME, "IUA subject_name");
    assert_eq!(identifiers(person), [IDENTIFIER], "IUA npi");
    assert_eq!(levels(person), ["substantial"], "Art 8(2)(b)");
    assert_eq!(acting(person), Some("person"));
    only_in_the_person(&record, &[NAME, IDENTIFIER, "ihe-assuranceLevel"]);
    Ok(())
}

/// Annex II 3.2(b): a client its issuer declares as acting for the
/// professional the token names is recorded as a client, with that
/// professional as the natural person.
#[tokio::test]
async fn a_declared_client_is_recorded_acting_for_the_professional() -> TestResult {
    let claims = as_client(professional(HIGH));
    let record = recorded(assured(AssuranceLevel::Substantial, true), &claims)
        .await?
        .record;
    let person = person(&record)?;
    assert_eq!(acting(person), Some("client"));
    assert_eq!(person["who"]["display"], NAME);
    assert_eq!(identifiers(person), [IDENTIFIER]);
    assert_eq!(levels(person), ["high"], "Art 8(2)(c)");
    assert!(
        others(&record).contains(&claims.client_id.as_str()),
        "the client application is its own agent"
    );
    only_in_the_person(&record, &[NAME, IDENTIFIER, "ihe-assuranceLevel"]);
    Ok(())
}

/// Annex II 3.2(a), (b), Implementing Regulation (EU) 2026/2099 Art 7: the
/// connector of an issuer declared a national contact point, a client
/// acting for a foreign professional, is recorded with the foreign provider
/// as the provider, the foreign professional as the natural person, the
/// connector as the client that relayed the request, and the Annex
/// attributes marked as the contact point's assertion.
#[tokio::test]
async fn a_contact_point_caller_names_the_foreign_professional_and_provider() -> TestResult {
    let mut claims = contact_point::relaying();
    CONNECTOR.clone_into(&mut claims.client_id);
    CONNECTOR.clone_into(&mut claims.sub);
    claims.other.insert(String::from("acr"), HIGH.to_owned());
    if let Some(extensions) = claims.extensions.as_mut() {
        extensions.ihe_iua.subject_name = Some(NAME.to_owned());
    }
    let mut auth = assured(AssuranceLevel::High, false);
    let declared = contact_point::declared()?;
    for (issuer, contact) in auth.issuers.iter_mut().zip(declared.issuers) {
        issuer.national_contact_point = contact.national_contact_point;
    }
    let record = recorded(auth, &claims).await?.record;
    let person = person(&record)?;
    assert_eq!(acting(person), Some("client"));
    assert_eq!(person["who"]["display"], NAME, "3.2(b)");
    assert_eq!(identifiers(person), [contact_point::HP_ID], "3.2(b)");
    assert_eq!(levels(person), ["high"]);
    let others = others(&record);
    assert!(
        others.contains(&contact_point::HCP_ID),
        "3.2(a): {others:?}"
    );
    assert!(
        others.contains(&CONNECTOR),
        "the relaying client: {others:?}"
    );
    assert_eq!(details(&record, "ehds-relayed", "asserted"), ["true"]);
    assert_eq!(
        details(&record, "ehds-relayed", "contact-point"),
        [support::ISSUER]
    );
    only_in_the_person(&record, &[NAME, "ihe-assuranceLevel"]);
    Ok(())
}

/// An issuer that declares no assurance mapping establishes no level, so
/// the record carries none, whatever the token states.
#[tokio::test]
async fn no_level_is_recorded_when_the_issuer_declares_none() -> TestResult {
    let record = recorded(support::auth(), &professional(HIGH)).await?.record;
    let person = person(&record)?;
    assert!(levels(person).is_empty(), "never inferred: {person}");
    assert_eq!(acting(person), Some("person"));
    assert_eq!(identifiers(person), [IDENTIFIER]);
    Ok(())
}

/// A token that states no professional is recorded with none.
#[tokio::test]
async fn a_token_naming_no_professional_records_none() -> TestResult {
    let mut claims = support::claims();
    claims
        .other
        .insert(String::from("acr"), SUBSTANTIAL.to_owned());
    let record = recorded(assured(AssuranceLevel::Substantial, false), &claims)
        .await?
        .record;
    let person = person(&record)?;
    assert!(identifiers(person).is_empty(), "{person}");
    assert!(person["who"].get("display").is_none(), "{person}");
    assert_eq!(levels(person), ["substantial"]);
    Ok(())
}

// conformance: track-10
#[tokio::test]
async fn the_professional_and_the_patient_reach_no_log_or_metric() -> TestResult {
    let claims = professional(SUBSTANTIAL);
    let Recorded {
        log, exposition, ..
    } = recorded(assured(AssuranceLevel::Substantial, false), &claims).await?;
    recorded_the_query(&exposition, "node-a-pub")?;
    for value in [NAME, IDENTIFIER, PATIENT, EHR_A, EHR_B] {
        assert!(!log.contains(value), "N33: {value} in the log: {log}");
        assert!(!exposition.contains(value), "{value} in a metric");
    }
    Ok(())
}
