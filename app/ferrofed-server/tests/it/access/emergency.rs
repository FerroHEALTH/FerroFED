// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Emergency access (Regulation (EU) 2025/327 Art 11(5)): an access whose
//! verified token declares a purpose of use the deployment names in
//! `[[access_log.emergency_purpose]]` is marked in its record, and nothing
//! else marks one; the purpose reaches each node in the conveyance, which
//! decides (Federation Tier §13.4, N26); a node's refusal stands as it
//! would without the mark; and no patient identifier reaches the log line
//! the mark writes or a node (§5.4, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use ehds_logging::emergency::EmergencyError;
use ferrofed_engine::conveyance::HEADER;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::issuer::{ACT_REASON, Claims, Coding, Extensions};
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{LAB_REPORT, RETENTION, TestResult, accesses, composition, details, named, settings};
use crate::auth::{bearing, minted, query};
use crate::conveyance::{published, verified};
use crate::facade::{EHR_A, EHR_B, PATIENT, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::run::binary;
use crate::support::conveyed::ConveyedPurpose;
use crate::support::{self, Logs, call};

const UID_A: &str = "7b8c9d0e-2f3a-4b4c-9d5e-6f7a8b9c0d1e::cdr-a.example.org::1";

/// The entity that marks an emergency access.
const MARK: &str = "ehds-emergency-access";

/// The log line the mark writes.
const LOGGED: &str = "the access was declared an emergency access by its purpose of use";

/// `[[access_log.emergency_purpose]]` naming break the glass.
fn break_the_glass() -> String {
    format!("\n[[access_log.emergency_purpose]]\nsystem = \"{ACT_REASON}\"\ncode = \"BTG\"\n")
}

/// The suite's claims declaring treatment and each code of `codes`.
fn declaring(codes: &[&str]) -> Claims {
    let mut claims = support::claims();
    let mut extensions = Extensions::treatment();
    extensions
        .ihe_iua
        .purpose_of_use
        .extend(codes.iter().map(|code| Coding {
            system: ACT_REASON.to_owned(),
            code: (*code).to_owned(),
        }));
    claims.extensions = Some(extensions);
    claims
}

/// What one access left behind.
struct Recorded {
    status: StatusCode,
    record: Value,
    log: String,
    node_a: Server,
    keys: jsonwebtoken::jwk::JwkSet,
}

/// Sends the suite's patient query with a token over `claims` to a gateway
/// whose `[access_log]` adds `emergency`, node A answering with `answer_a`,
/// and returns what the access left behind.
async fn recorded(
    emergency: &str,
    claims: &Claims,
    answer_a: ResponseTemplate,
) -> Result<Recorded, Box<dyn Error>> {
    let node_a = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(answer_a)
        .mount(&node_a)
        .await;
    let node_b = super::node_with_rows(&[]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = settings(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", ""),
        &format!("{RETENTION}{emergency}"),
    )?;
    let state = Arc::new(AppState::build(&settings)?);
    let mut server = settings_with_room();
    server.auth = support::auth();
    let app = ferrofed_server::router(state, &server);
    let keys = published(&app).await?;
    let logs = Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "trace",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, _) = call(app, bearing(query()?, &minted(claims)?)?).await?;
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    drop(guard);
    let [record] = records.as_slice() else {
        return Err(format!("one record of the access, got {records:?}").into());
    };
    Ok(Recorded {
        status,
        record: record.clone(),
        log: logs.text(),
        node_a,
        keys,
    })
}

/// Node A's answer of one lab report.
fn lab_report() -> ResponseTemplate {
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c"}}],"rows":[[{}]]}}"##,
        composition(LAB_REPORT, UID_A)
    );
    ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json")
}

/// Node A's refusal.
fn refusal() -> ResponseTemplate {
    ResponseTemplate::new(403)
}

/// Art 11(5): a token that declares a purpose the deployment names is
/// recorded as an emergency access, naming that purpose, and the gateway
/// writes one log line under its request id.
#[tokio::test]
async fn a_declared_emergency_purpose_marks_the_record() -> TestResult {
    let Recorded {
        status,
        record,
        log,
        ..
    } = recorded(&break_the_glass(), &declaring(&["BTG"]), lab_report()).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(named(&record, MARK).len(), 1, "{record}");
    assert_eq!(details(&record, MARK, "ehds-emergency-access"), ["true"]);
    assert_eq!(
        details(&record, MARK, "ehds-emergency-purpose"),
        [format!("{ACT_REASON}|BTG")]
    );
    assert_eq!(log.matches(LOGGED).count(), 1, "{log}");
    Ok(())
}

/// Never inferred: a deployment that names no emergency purpose marks
/// nothing, whatever the token declares.
#[tokio::test]
async fn no_access_is_marked_without_a_declared_purpose() -> TestResult {
    let Recorded { record, log, .. } = recorded("", &declaring(&["BTG"]), lab_report()).await?;
    assert!(named(&record, MARK).is_empty(), "{record}");
    assert!(!log.contains(LOGGED), "{log}");
    Ok(())
}

/// Never inferred: a token that declares no named purpose is not marked,
/// even where the deployment names one.
#[tokio::test]
async fn a_token_without_the_purpose_is_not_marked() -> TestResult {
    let Recorded { record, log, .. } =
        recorded(&break_the_glass(), &declaring(&["ETREAT"]), lab_report()).await?;
    assert!(named(&record, MARK).is_empty(), "{record}");
    assert!(!log.contains(LOGGED), "{log}");
    Ok(())
}

/// §13.4, N26: the emergency purpose reaches each node in the conveyance,
/// as the token declared it, so the node decides on it.
// conformance: CP-17
#[tokio::test]
async fn the_emergency_purpose_is_conveyed_to_the_node() -> TestResult {
    let Recorded { node_a, keys, .. } =
        recorded(&break_the_glass(), &declaring(&["BTG"]), lab_report()).await?;
    let requests = node_a.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("one request at node A, got {}", requests.len()).into());
    };
    let token = request
        .headers
        .get(HEADER)
        .ok_or("the conveyance")?
        .to_str()?;
    let conveyed = verified(token, &keys, "node-a-pub")?;
    let mut codes: Vec<(Option<&str>, &str)> = conveyed
        .purpose_of_use
        .iter()
        .map(|ConveyedPurpose { system, code }| (system.as_deref(), code.as_str()))
        .collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        [(Some(ACT_REASON), "BTG"), (Some(ACT_REASON), "TREAT")],
        "every declared purpose, the emergency one included, in any order"
    );
    Ok(())
}

/// N26, Art 11(5): the gateway never overrides a node's refusal. A node
/// that refuses an emergency access is asked once and answered as it would
/// be without the mark, and the record is marked and names the refusal.
#[tokio::test]
async fn a_nodes_refusal_of_an_emergency_access_stands() -> TestResult {
    let plain = recorded("", &declaring(&[]), refusal()).await?;
    let marked = recorded(&break_the_glass(), &declaring(&["BTG"]), refusal()).await?;
    assert_eq!(plain.status, marked.status, "the mark changes no answer");
    for at in [&plain.node_a, &marked.node_a] {
        let asked = at.received_requests().await.ok_or("recording is on")?;
        assert_eq!(asked.len(), 1, "asked once, never again");
    }
    let status = |record: &Value| {
        named(record, "origin")
            .iter()
            .find(|origin| {
                origin
                    .pointer("/what/identifier/value")
                    .and_then(Value::as_str)
                    == Some("node-a-pub")
            })
            .and_then(|origin| {
                origin["detail"]
                    .as_array()?
                    .iter()
                    .find(|detail| detail["type"] == "status")?["valueString"]
                    .as_str()
                    .map(str::to_owned)
            })
    };
    assert_eq!(status(&plain.record), status(&marked.record));
    assert!(status(&marked.record).is_some(), "{}", marked.record);
    assert_eq!(named(&marked.record, MARK).len(), 1, "{}", marked.record);
    Ok(())
}

/// The resolution of `table` in a development configuration.
fn resolved(table: &str) -> Result<(), ferrofed_server::config::error::Error> {
    let text = format!("profile = \"development\"\n\n{table}");
    Config::from_sources(Some(&text), &BTreeMap::new())
        .and_then(|config| config.resolve().map(|_| ()))
}

#[test]
fn an_emergency_purpose_with_an_empty_code_or_system_is_refused() {
    for (table, expected) in [
        (
            "[[access_log.emergency_purpose]]\nsystem = \"urn:oid:2.999.5\"\ncode = \"\"\n",
            EmergencyError::EmptyCode { index: 0 },
        ),
        (
            "[[access_log.emergency_purpose]]\ncode = \"BTG\"\n\n\
             [[access_log.emergency_purpose]]\nsystem = \" \"\ncode = \"BTG\"\n",
            EmergencyError::EmptySystem { index: 1 },
        ),
    ] {
        let refused = resolved(table);
        assert!(
            matches!(&refused, Err(ferrofed_server::config::error::Error::AccessLogEmergency(error)) if *error == expected),
            "{table}: {refused:?}"
        );
    }
}

#[test]
fn an_emergency_purpose_without_a_code_or_with_an_unknown_key_does_not_load() {
    for table in [
        "[[access_log.emergency_purpose]]\nsystem = \"urn:oid:2.999.5\"\n",
        "[[access_log.emergency_purpose]]\ncode = \"BTG\"\ndisplay = \"break the glass\"\n",
    ] {
        assert!(resolved(table).is_err(), "{table}");
    }
}

/// `config check` notes an access log that names no emergency purpose, and
/// none that names one.
#[test]
fn config_check_notes_an_access_log_without_an_emergency_purpose() -> TestResult {
    let note = "ferrofed: note: [access_log] declares no [[access_log.emergency_purpose]]";
    let plain = binary(&["config", "check"], "profile = \"development\"\n")?;
    assert_eq!(
        Some(0),
        plain.status.code(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    let stdout = String::from_utf8_lossy(&plain.stdout);
    assert!(stdout.contains(note), "{stdout}");
    let declared = binary(
        &["config", "check"],
        &format!("profile = \"development\"\n{}", break_the_glass()),
    )?;
    assert_eq!(
        Some(0),
        declared.status.code(),
        "{}",
        String::from_utf8_lossy(&declared.stderr)
    );
    let stdout = String::from_utf8_lossy(&declared.stdout);
    assert!(!stdout.contains("emergency_purpose"), "{stdout}");
    Ok(())
}

/// §5.4, N33: the mark's log line names no patient and no `ehr_id`, and no
/// node receives the patient identifier.
// conformance: track-10
#[tokio::test]
async fn the_mark_carries_no_patient_identifier_to_a_log_or_a_node() -> TestResult {
    let Recorded { log, node_a, .. } =
        recorded(&break_the_glass(), &declaring(&["BTG"]), lab_report()).await?;
    assert!(log.contains(LOGGED), "{log}");
    for value in [PATIENT, EHR_A, EHR_B] {
        assert!(!log.contains(value), "N33: {value} in the log: {log}");
    }
    let wire = crate::facade::wire(&node_a).await?;
    assert!(
        !wire.contains(PATIENT),
        "N33: the patient at the node: {wire}"
    );
    Ok(())
}
