// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The update hazard of §10.3 end to end (N36, CP-29): a version node A
//! created is held as an imported copy at node B, under B's own `ehr_id`, and
//! a federated query under `openEHR-federation-dedup: version-identity` keeps
//! A's row and names B's endpoint as suppressed. The client then writes
//! against the surviving row.
//!
//! A write through B's `ehr_id`, routed by the `ehr_id` index or by a header
//! naming B, is refused `409` naming the controlling system, whether node A
//! is down or up, and neither node is sent it (§10.3 `copy-write-reject`). A
//! write through A's own `ehr_id` reaches A alone, and its answer names A and
//! nothing of the copies (§10.3, "Copies do not converge"). Node A sits behind
//! the testkit's capturing proxy, which refuses its connections while it is
//! down and counts every one it refused, so "A is never tried" is read on the
//! wire; every other assertion on what a node received reads that node's own
//! capture (§16, track 5 and track 6).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use ferrofed_testkit::proxy::{CapturingProxy, Fault};
use http::{HeaderMap, Method, StatusCode};
use tempfile::TempDir;
use wiremock::{MockServer, ResponseTemplate};

use crate::dedup::{Answer, Dedup, request};
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, crossref, gateway, node_answering, registry, schema,
};
use crate::path_ehr_id::answer;
use crate::support::{asked, error_body, mount, send};
use crate::versioned_write::{
    CREATED_ELSEWHERE, ENDPOINT_A, ENDPOINT_B, LEGACY_MAPPING, outside_bodies, over,
    refused_at_neither, versioned,
};

type TestResult = Result<(), Box<dyn Error>>;

/// The versioned object node A created, which node B holds as an import.
const OBJECT: &str = "6a1f0c2e-4b7d-4e3a-9c5f-2d8e7b6a5c41";

/// The version node A created; an import keeps its uid, so node B's copy
/// carries it too (§10.3, the scenario).
const ORIGINAL: &str = "6a1f0c2e-4b7d-4e3a-9c5f-2d8e7b6a5c41::cdr-a.example.org::1";

/// The version node A answers the write of [`ORIGINAL`] with.
const SECOND: &str = "6a1f0c2e-4b7d-4e3a-9c5f-2d8e7b6a5c41::cdr-a.example.org::2";

/// Node A's `system_id`, the version's `creating_system_id`.
const OWNER_SYSTEM: &str = "cdr-a.example.org";

/// What node A answers the write with.
const WRITTEN: &str = r#"{"_type":"COMPOSITION","uid":{"_type":"OBJECT_VERSION_ID","value":"6a1f0c2e-4b7d-4e3a-9c5f-2d8e7b6a5c41::cdr-a.example.org::2"}}"#;

/// The body the client commits, carrying a marker no error may echo.
const SENT: &str =
    r#"{"_type":"COMPOSITION","name":{"_type":"DV_TEXT","value":"synthetic-amendment-k7"}}"#;

/// The marker inside [`SENT`].
const SENT_MARKER: &str = "synthetic-amendment-k7";

/// The query: the endpoint each row was read from, and the version uid, for
/// the patient both nodes hold (§9.3, §7.2).
fn query() -> String {
    format!(
        r#"SELECT p/id AS endpoint_id, c/uid/value FROM ENDPOINT p ["{ENDPOINT_A}", "{ENDPOINT_B}"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"#
    )
}

/// The composition at node A, addressed by A's own `ehr_id`.
fn at_owner() -> String {
    format!("/v1/ehr/{EHR_A}/composition/{OBJECT}")
}

/// The composition at node B, addressed by B's own `ehr_id`.
fn at_copy() -> String {
    format!("/v1/ehr/{EHR_B}/composition/{OBJECT}")
}

/// The federated query, as a node records it.
fn queried() -> (String, String) {
    ("POST".to_owned(), "/v1/query/aql".to_owned())
}

/// Two nodes holding [`ORIGINAL`] after the de-duplicated query: node A, its
/// creator, behind a capturing proxy, and node B, which holds a copy.
struct Scenario {
    app: Router,
    owner: CapturingProxy,
    a: MockServer,
    b: MockServer,
    _dir: TempDir,
}

impl Scenario {
    /// Sets the federation up and runs the query under
    /// `openEHR-federation-dedup: version-identity`, asserting the one row it
    /// keeps and the copy it suppresses (§10.2, §10.3).
    async fn after_the_query() -> Result<Self, Box<dyn Error>> {
        let a = node_answering(ORIGINAL).await;
        mount(
            &a,
            "PUT",
            at_owner(),
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("\"{SECOND}\"").as_str())
                .set_body_raw(WRITTEN.as_bytes().to_vec(), "application/json"),
        )
        .await;
        let owner = CapturingProxy::start(a.uri()).await?;
        let b = node_answering(ORIGINAL).await;
        let dir = tempfile::tempdir()?;
        let app = gateway(
            dir.path(),
            &registry(owner.origin(), &b.uri(), ""),
            "profile = \"development\"",
            &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        )?;
        let (status, _, text) =
            answer(app.clone(), request(&query(), &["version-identity"])?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        schema::validate(&text)?;
        let answered: Answer = serde_json::from_str(&text)?;
        assert_eq!(
            answered.rows,
            [vec![ENDPOINT_A.to_owned(), ORIGINAL.to_owned()]],
            "§10.2, §10.3 dedup-write-target: one row, read from the originating endpoint"
        );
        assert_eq!(
            answered.meta.federation.dedup,
            Dedup {
                mode: "version-identity".to_owned(),
                suppressed_rows: Some(1),
                suppressed_endpoints: Some(vec![ENDPOINT_B.to_owned()]),
            },
            "§10.3 suppressed-visible, N36: the copy at node B stays visible"
        );
        assert_eq!(
            vec![queried()],
            asked(&a).await?,
            "node A answered the query"
        );
        assert_eq!(
            vec![queried()],
            asked(&b).await?,
            "node B answered the query"
        );
        Ok(Self {
            app,
            owner,
            a,
            b,
            _dir: dir,
        })
    }

    /// Writes against the surviving row through node B's `ehr_id`, routed by
    /// `endpoint` when given and by the `ehr_id` index the query's resolution
    /// taught otherwise, and asserts the `409` naming the controlling system.
    async fn refused_at_the_copy(&self, endpoint: Option<&str>) -> TestResult {
        let how = endpoint.map_or("the ehr_id index", |_| "the targeting header");
        let write = versioned(&Method::PUT, &at_copy(), endpoint, Some(ORIGINAL), SENT)?;
        let (status, acting, text) = answer(self.app.clone(), write).await?;
        assert_eq!(StatusCode::CONFLICT, status, "{how}: {text}");
        let refused = error_body(&text)?;
        assert_eq!(
            "controlling-system-unreachable", refused.code,
            "§10.3 copy-write-reject, {how}"
        );
        assert!(acting.is_none(), "no endpoint acted: {text}");
        names_the_controller_alone(&refused.message);
        Ok(())
    }

    /// Asserts that neither node received anything after the query, and that
    /// nobody tried to reach node A.
    async fn nothing_sent_after_the_query(&self) -> TestResult {
        assert_eq!(
            vec![queried()],
            asked(&self.b).await?,
            "the holder of the copy is never written (§10.3 copy-write-reject)"
        );
        assert_eq!(
            vec![queried()],
            asked(&self.a).await?,
            "node A is sent no write"
        );
        assert_eq!(1, self.owner.journal().len(), "node A saw the query alone");
        assert_eq!(0, self.owner.refused(), "node A is never tried");
        Ok(())
    }
}

/// Asserts that `message` names the controlling system, by its
/// `creating_system_id` and the endpoint the registry reaches it through, and
/// quotes nothing else of the request (§10.3 copy-write-reject, §5.4.3).
fn names_the_controller_alone(message: &str) {
    assert!(
        message.contains(OWNER_SYSTEM) && message.contains(ENDPOINT_A),
        "the error identifies the controlling system: {message}"
    );
    for quoted in [EHR_B, EHR_A, OBJECT, PATIENT, SENT_MARKER] {
        assert!(!message.contains(quoted), "{quoted} is quoted: {message}");
    }
}

// conformance: CP-29
#[tokio::test]
async fn a_write_against_the_surviving_row_while_its_owner_is_down_is_a_409_and_reaches_no_node()
-> TestResult {
    let scenario = Scenario::after_the_query().await?;
    scenario.owner.set_fault(Fault::Refuse);
    scenario.refused_at_the_copy(None).await?;
    scenario.refused_at_the_copy(Some(ENDPOINT_B)).await?;
    scenario.nothing_sent_after_the_query().await
}

// conformance: CP-29
#[tokio::test]
async fn with_the_owner_up_a_write_through_the_copys_ehr_id_is_still_a_409() -> TestResult {
    let scenario = Scenario::after_the_query().await?;
    scenario.refused_at_the_copy(None).await?;
    scenario.refused_at_the_copy(Some(ENDPOINT_B)).await?;
    scenario.nothing_sent_after_the_query().await
}

// conformance: CP-29
#[tokio::test]
async fn a_write_through_the_owners_ehr_id_reaches_the_owner_alone_and_claims_no_copy() -> TestResult
{
    let scenario = Scenario::after_the_query().await?;
    let write = versioned(&Method::PUT, &at_owner(), None, Some(ORIGINAL), SENT)?;
    let response = send(scenario.app.clone(), write).await?;
    assert_eq!(StatusCode::OK, response.status());
    let fields = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    assert_eq!(
        WRITTEN.as_bytes(),
        bytes.as_ref(),
        "the owner's answer comes back as it sent it (N22, N31)"
    );
    names_the_owner_alone(&fields);
    let body = String::from_utf8(bytes.to_vec())?;
    for copy in [ENDPOINT_B, "node-b", "cdr-b.example.org", EHR_B] {
        assert!(!body.contains(copy), "§10.3: the body names {copy}: {body}");
    }
    assert_eq!(
        vec![queried(), ("PUT".to_owned(), at_owner())],
        asked(&scenario.a).await?,
        "the controlling CDR is sent the write once (§12.4, §12a.1, N23)"
    );
    let received = scenario.a.received_requests().await.ok_or("recording")?;
    let put = received.last().ok_or("node A was sent the write")?;
    assert_eq!(SENT.as_bytes(), put.body.as_slice(), "byte-identical (N22)");
    assert!(!outside_bodies(&scenario.a).await?.contains(PATIENT), "N33");
    assert_eq!(
        vec![queried()],
        asked(&scenario.b).await?,
        "the copy is not updated, and nothing says it was (§10.3)"
    );
    Ok(())
}

/// Asserts that `fields` name node A as the one endpoint written, and that
/// no field names node B or its copy (§10.3, §7a.3, N31).
fn names_the_owner_alone(fields: &HeaderMap) {
    let values = |name: &str| -> Vec<String> {
        fields
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(
        vec![ENDPOINT_A.to_owned()],
        values("openEHR-federation-endpoint"),
        "N31: the one endpoint written"
    );
    assert_eq!(
        vec![OWNER_SYSTEM.to_owned()],
        values("openEHR-federation-system-id"),
        "§9.6"
    );
    assert_eq!(vec![format!("\"{SECOND}\"")], values("etag"), "N31");
    for (name, value) in fields {
        let named = name.as_str();
        assert!(
            !named.starts_with("openehr-federation-")
                || named == "openehr-federation-endpoint"
                || named == "openehr-federation-system-id",
            "§10.3: no federation field reports other copies: {named}"
        );
        let text = String::from_utf8_lossy(value.as_bytes());
        for copy in [ENDPOINT_B, "node-b", "cdr-b.example.org", EHR_B] {
            assert!(!text.contains(copy), "§10.3: {named} names {copy}: {text}");
        }
    }
}

// conformance: CP-29
#[tokio::test]
async fn every_refusal_names_the_controlling_system_and_quotes_nothing_of_the_request() -> TestResult
{
    let mapped = "7a6b5c4d-3e2f-4a1b-8c9d-0e1f2a3b4c5d::LEGACY-A.Example.Org::4";
    let member = "6a1f0c2e-4b7d-4e3a-9c5f-2d8e7b6a5c41::CDR-A.Example.Org::1";
    let cases: [(&str, &str, &[&str], &[&str]); 3] = [
        (
            member,
            "",
            &[OWNER_SYSTEM, "node-a", ENDPOINT_A],
            &["CDR-A.Example.Org"],
        ),
        (
            mapped,
            LEGACY_MAPPING,
            &["legacy-a.example.org", "node-a", ENDPOINT_A],
            &["LEGACY-A.Example.Org", "7a6b5c4d"],
        ),
        (
            CREATED_ELSEWHERE,
            "",
            &["If-Match"],
            &["external.example.org", "5c3e9b1a"],
        ),
    ];
    for (version, extra, named, unquoted) in cases {
        let a = MockServer::start().await;
        let b = MockServer::start().await;
        let dir = tempfile::tempdir()?;
        let write = versioned(
            &Method::PUT,
            &at_copy(),
            Some(ENDPOINT_B),
            Some(version),
            SENT,
        )?;
        let text = refused_at_neither(
            over(dir.path(), &a, &b, extra)?,
            write,
            (StatusCode::CONFLICT, "controlling-system-unreachable"),
            (&a, &b),
        )
        .await?;
        let message = error_body(&text)?.message;
        for name in named {
            assert!(
                message.contains(name),
                "§10.3 names the controlling system ({name}): {message}"
            );
        }
        for quoted in unquoted.iter().chain(&[EHR_B, OBJECT, SENT_MARKER]) {
            assert!(
                !message.contains(quoted),
                "§5.4.3: {quoted} is the request's: {message}"
            );
        }
    }
    Ok(())
}
