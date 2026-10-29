// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A document a person inserts in their own EHR under their own grant
//! (Regulation (EU) 2025/327 Art 5): the composition names the record's
//! subject, a `PARTY_SELF` with no identifier, as every entry's provider
//! (openEHR RM Release 1.1.0, EHR IM, `ENTRY.provider`), a professional's
//! receipt is written as before, and the mark carries no patient identifier
//! to the member or into a log line (§5.4.1, N33).

use std::error::Error;
use std::path::Path;

use axum::Router;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::{EHR_SYSTEM, bound, creating, document, gateway_with, post, receiving, rows};
use crate::auth::{bearing, claims, minted};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, received};
use crate::fhir::TestResult;
use crate::support::{Logs, call};

/// The synthetic person who inserts the document: the token's `sub`.
pub(super) const PERSON: &str = "synthetic-person-8k2q";

/// The entry classes of the openEHR RM, each of which has a `provider`.
const ENTRIES: [&str; 5] = [
    "OBSERVATION",
    "EVALUATION",
    "INSTRUCTION",
    "ACTION",
    "ADMIN_ENTRY",
];

/// The claims of a person's own grant: confined to [`PATIENT`], whose EHR
/// at node A the token names, with a scope that creates compositions.
pub(super) fn own_grant() -> Claims {
    let mut granted = claims();
    PERSON.clone_into(&mut granted.sub);
    granted.scope = Some(String::from("patient/composition-*.c"));
    granted.ehr_id = Some(EHR_A.to_owned());
    granted
}

/// The gateway receiving patient summaries at node A, its issuer's patient
/// tokens bound to node A, with the tables `tables` added.
pub(super) fn person_gateway(
    dir: &Path,
    (a, b): (&Server, &Server),
    tables: &str,
) -> Result<Router, Box<dyn Error>> {
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (EHR_SYSTEM, EHR_A, "node-a", EHR_A),
    ]);
    let fhir = format!("{}{tables}", receiving("node-a", &[NAMESPACE]));
    gateway_with(dir, (a, b), (&crossref, &fhir), &bound()?)
}

/// The `provider` of every entry the composition `body` holds, in its
/// sections included, `Value::Null` for an entry that names none.
pub(super) fn providers(body: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    fn walk(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(members) => {
                let entry = members
                    .get("_type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| ENTRIES.contains(&kind));
                if entry {
                    found.push(members.get("provider").cloned().unwrap_or(Value::Null));
                }
                for (name, member) in members {
                    if name != "feeder_audit" {
                        walk(member, found);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, found)),
            _ => {}
        }
    }
    let composition: Value = serde_json::from_str(body)?;
    let mut found = Vec::new();
    walk(&composition, &mut found);
    Ok(found)
}

/// The one composition `server` was sent.
pub(super) async fn written(server: &Server) -> Result<String, Box<dyn Error>> {
    let [body] = received(server)
        .await?
        .try_into()
        .map_err(|bodies| format!("one write, got {bodies:?}"))?;
    Ok(body)
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_persons_own_document_is_marked_as_inserted_by_them_with_no_identifier_added()
-> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = person_gateway(dir.path(), (&a, &b), "")?;
    let token = minted(&own_grant())?;
    let logs = Logs::default();
    let capture = crate::support::console::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "Art 5: {text}");
    let body = written(&a).await?;
    let providers = providers(&body)?;
    assert!(!providers.is_empty(), "the composition holds an entry");
    for provider in &providers {
        assert_eq!(
            provider,
            &serde_json::json!({ "_type": "PARTY_SELF" }),
            "RM ENTRY.provider: the record's subject, with no external_ref: {body}"
        );
    }
    let captured = super::write::composed(&a).await?;
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !captured.contains(withheld),
            "N33: no identifier in the path or the headers: {captured}"
        );
    }
    let professional = {
        let (c, d) = (creating().await, Server::start().await);
        let dir = tempfile::tempdir()?;
        let app = person_gateway(dir.path(), (&c, &d), "")?;
        let (status, text) = call(app, post(&document())?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        written(&c).await?
    };
    assert_eq!(
        professional.matches(PATIENT).count(),
        body.matches(PATIENT).count(),
        "§5.4 scope note: the mark adds no identifier beyond the document the composition keeps"
    );
    let logged = logs.text();
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !logged.contains(withheld),
            "N33: no identifier in a log line: {logged}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_professionals_document_names_no_provider() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = person_gateway(dir.path(), (&a, &b), "")?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let body = written(&a).await?;
    let providers = providers(&body)?;
    assert!(!providers.is_empty(), "the composition holds an entry");
    assert!(
        providers.iter().all(Value::is_null),
        "no entry is marked as inserted by the person: {body}"
    );
    Ok(())
}
