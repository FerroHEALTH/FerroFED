// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's own stored queries, the patient summary's section queries,
//! held read-only under the reserved namespace at one immutable version
//! (§12.7, N44): listed and read beside a deployment's definitions, refused
//! to every `PUT`, never shadowed by a store, and run by name as an ordinary
//! fan-out that carries no patient identifier to a node (§5.4.1, N33).

use std::error::Error;

use axum::Router;
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::reserved;
use ferrofed_registry::definition::store::{DefinitionStore, Insertion};
use ferrofed_registry::definition::{QueryName, StoredDefinition};
use ferrofed_server::stored::embedded::RedbStore;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use jiff::Timestamp;
use openehr_its::rest::generated::definition::StoredQuery;
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{EHR_A, EHR_B, Meta, NAMESPACE, PATIENT, PATIENT_TAIL, received, wire};
use crate::support::{call, chain, error_body};

use super::{TestResult, get, invoke, parameterised, put, store_file, two_members};

/// The name of the problems section's query.
fn problems() -> Result<QueryName, Box<dyn Error>> {
    Ok(Section::Problems.name()?)
}

/// A node answering `POST /v1/query/aql` with one composition of the
/// template `template`, its uid `uid`, as the section queries select it.
async fn node_holding(uid: &str, template: &str) -> Server {
    let server = Server::start().await;
    let answer = format!(
        r#"{{"q":"node","columns":[{{"name":"composition","path":"c"}},{{"name":"uid","path":"c/uid/value"}},{{"name":"template_id","path":"c/archetype_details/template_id/value"}}],"rows":[[{{"_type":"COMPOSITION","name":{{"_type":"DV_TEXT","value":"Synthetic summary"}},"uid":{{"_type":"OBJECT_VERSION_ID","value":"{uid}"}},"archetype_node_id":"openEHR-EHR-COMPOSITION.health_summary.v1","archetype_details":{{"_type":"ARCHETYPED","archetype_id":{{"_type":"ARCHETYPE_ID","value":"openEHR-EHR-COMPOSITION.health_summary.v1"}},"template_id":{{"_type":"TEMPLATE_ID","value":"{template}"}},"rm_version":"1.1.0"}}}},"{uid}","{template}"]]}}"#
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// The uid and template of the composition node A holds.
const A_UID: &str = "uid-at-a::cdr-a.example.org::1";
const A_TEMPLATE: &str = "Synthetic Summary A";

/// The uid and template of the composition node B holds.
const B_UID: &str = "uid-at-b::cdr-b.example.org::1";
const B_TEMPLATE: &str = "Synthetic Summary B";

/// A section query's answer: the composition, its uid and its template id
/// per row, and the members' statuses.
#[derive(Debug, Deserialize)]
struct Selected {
    rows: Vec<(Composition, String, String)>,
    meta: Meta,
}

/// The uid a composition cell carries.
#[derive(Debug, Deserialize)]
struct Composition {
    uid: Uid,
}

/// An `OBJECT_VERSION_ID`.
#[derive(Debug, Deserialize)]
struct Uid {
    value: String,
}

/// The `Query` body binding the patient and the namespace that issued it.
fn bound() -> String {
    format!(r#"{{"query_parameters":{{"patient":"{PATIENT}","namespace":"{NAMESPACE}"}}}}"#)
}

/// A registry gateway over two members holding one composition each, the
/// patient known at both.
async fn gateway(dir: &std::path::Path) -> Result<(Router, Server, Server), Box<dyn Error>> {
    let a = node_holding(A_UID, A_TEMPLATE).await;
    let b = node_holding(B_UID, B_TEMPLATE).await;
    let app = two_members(dir, &a, &b)?;
    Ok((app, a, b))
}

// conformance: CP-40
#[tokio::test]
async fn the_section_queries_are_listed_and_read_at_their_immutable_version() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, a, b) = gateway(dir.path()).await?;
    super::stored(&app, "1.0.0", &parameterised()).await?;

    let (status, text) = call(app.clone(), get(&format!("{}::", reserved::NAMESPACE))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let listed: Vec<StoredQuery> = serde_json::from_str(&text)?;
    assert_eq!(Section::ALL.len(), listed.len(), "§12.7: one per section");
    for section in Section::ALL {
        let name = section.name()?;
        let held = listed
            .iter()
            .find(|held| held.name == name.as_str())
            .ok_or_else(|| format!("{name} is listed"))?;
        assert_eq!("1.0.0", held.version, "N44: the one immutable version");
        assert_eq!(section.aql(), held.q);
        assert_eq!("AQL", held.r#type);
    }

    let (status, text) = call(app.clone(), get(&format!("{}/1.0.0", problems()?))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let read: StoredQuery = serde_json::from_str(&text)?;
    assert_eq!(Section::Problems.aql(), read.q);
    assert!(
        read.q.contains("c/archetype_details/template_id/value"),
        "§12.7: whole compositions with their template id: {}",
        read.q
    );

    let (status, text) = call(app.clone(), get("org.example::")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let deployment: Vec<StoredQuery> = serde_json::from_str(&text)?;
    assert_eq!(
        1,
        deployment.len(),
        "the deployment's own definition beside"
    );
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_put_into_the_reserved_namespace_is_refused_and_the_held_text_stands() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, _a, _b) = gateway(dir.path()).await?;
    let problems = problems()?;
    for (name, version) in [
        (problems.as_str().to_owned(), "1.0.0"),
        (problems.as_str().to_owned(), "1.0.1"),
        (problems.as_str().to_owned(), "2.0.0"),
        (
            format!("{}::patient-summary-alerts", reserved::NAMESPACE),
            "1.0.0",
        ),
        (problems.as_str().to_uppercase(), "1.0.0"),
        (format!("{}.nested::anything", reserved::NAMESPACE), "1.0.0"),
    ] {
        let (status, text) = call(app.clone(), put(&name, version, &parameterised())?).await?;
        assert_eq!(
            StatusCode::CONFLICT,
            status,
            "§12.7, N44: {name}/{version}: {text}"
        );
        let refused = error_body(&text)?;
        assert_eq!("stored-query-reserved", refused.code, "{name}/{version}");
        assert!(!text.contains(PATIENT_TAIL), "{text}");
    }
    let (status, text) = call(app.clone(), get(&format!("{}::", reserved::NAMESPACE))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let listed: Vec<StoredQuery> = serde_json::from_str(&text)?;
    assert_eq!(Section::ALL.len(), listed.len(), "nothing was added");
    let (status, text) = call(app, get(&format!("{problems}/1.0.0"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let read: StoredQuery = serde_json::from_str(&text)?;
    assert_eq!(Section::Problems.aql(), read.q, "N44: the held text stands");
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_store_holding_a_definition_in_the_reserved_namespace_refuses_the_start() -> TestResult {
    for name in [
        format!("{}::patient-summary-problems", reserved::NAMESPACE),
        format!("{}::added", reserved::NAMESPACE),
    ] {
        let dir = tempfile::tempdir()?;
        {
            let store = RedbStore::open(&store_file(dir.path()))?;
            let shadow = StoredDefinition::new(
                QueryName::new(&name)?,
                "1.0.0".parse()?,
                parameterised(),
                Timestamp::UNIX_EPOCH,
            );
            assert_eq!(Insertion::Stored, store.insert_if_absent(&shadow)?);
        }
        let a = node_holding(A_UID, A_TEMPLATE).await;
        let b = node_holding(B_UID, B_TEMPLATE).await;
        let refused = two_members(dir.path(), &a, &b)
            .err()
            .ok_or("a store never shadows a reserved query")?;
        let line = chain(refused.as_ref());
        assert!(line.contains(&name), "names the definition: {line}");
        assert!(line.contains("reserves"), "says why: {line}");
        assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    }
    Ok(())
}

// conformance: CP-40 track-10
#[tokio::test]
async fn each_section_query_runs_by_name_and_no_patient_identifier_reaches_a_node() -> TestResult {
    for section in Section::ALL {
        let dir = tempfile::tempdir()?;
        let (app, a, b) = gateway(dir.path()).await?;
        let name = section.name()?;
        let (status, text) = call(app, invoke(name.as_str(), &bound(), &[])?).await?;
        assert_eq!(StatusCode::OK, status, "{section:?}: {text}");
        let answer: Selected = serde_json::from_str(&text)?;
        assert_eq!(
            vec![("node-a-pub", "active"), ("node-b-pub", "active")],
            answer
                .meta
                .federation
                .endpoints
                .iter()
                .map(|endpoint| (endpoint.id.as_str(), endpoint.status.as_str()))
                .collect::<Vec<_>>(),
            "{section:?}: every member asked"
        );
        let mut rows: Vec<(&str, &str, &str)> = answer
            .rows
            .iter()
            .map(|(composition, uid, template)| {
                (
                    composition.uid.value.as_str(),
                    uid.as_str(),
                    template.as_str(),
                )
            })
            .collect();
        rows.sort_unstable();
        assert_eq!(
            vec![(A_UID, A_UID, A_TEMPLATE), (B_UID, B_UID, B_TEMPLATE),],
            rows,
            "§12.7: {section:?}: the whole composition, its uid and its template id"
        );
        for (node, ehr_id) in [(&a, EHR_A), (&b, EHR_B)] {
            let captured = wire(node).await?;
            assert!(!captured.is_empty(), "{section:?}: the member was asked");
            for withheld in [PATIENT, PATIENT_TAIL] {
                assert!(
                    !captured.contains(withheld),
                    "N33: {section:?}: no identifier in the query, path or headers: {captured}"
                );
            }
            let dispatched = received(node).await?.concat();
            assert!(
                dispatched.contains(ehr_id),
                "§7.1: {section:?}: scoped to the member's ehr_id: {dispatched}"
            );
            assert!(
                !dispatched.contains("external_ref"),
                "§7.1: {section:?}: the subject predicates are consumed: {dispatched}"
            );
            for archetype in section.archetypes() {
                assert!(
                    dispatched.contains(archetype.id),
                    "{section:?}: {dispatched}"
                );
            }
        }
    }
    Ok(())
}
