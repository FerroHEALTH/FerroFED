// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A commit routed to one FerroEHR node through the gateway: it lands
//! byte-identical, its `DV_IDENTIFIER` included, with the node's `Location`
//! and `ETag` and the acting endpoint's headers on the answer (§7a.3, N22,
//! N31, track 10).

use axum::body::Body;
use ferrofed_testkit::containers::{self, API_PATH};
use ferrofed_testkit::proxy::Capture;
use ferrofed_testkit::seed::{self, EhrSeed, SeedPlan};
use http::{Request, StatusCode, header};

use crate::e2e::{EHR_A, PATIENT, TestResult, composition_carrying, gateway};

/// A request of `verb` to `uri` naming node A in the endpoint header.
fn routed_to_a(verb: http::Method, uri: &str, body: Body) -> Result<Request<Body>, http::Error> {
    Request::builder()
        .method(verb)
        .uri(uri)
        .header("openEHR-federation-endpoint", "node-a-pub")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer synthetic-client-token")
        .body(body)
}

/// The text of response header `name`.
fn field<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Whether `capture` carries `needle` outside its body: in the path, the
/// query or a header.
fn outside_the_body(capture: &Capture, needle: &[u8]) -> bool {
    let found = |haystack: &[u8]| haystack.windows(needle.len()).any(|w| w == needle);
    found(capture.path.as_bytes())
        || capture
            .query
            .as_deref()
            .is_some_and(|query| found(query.as_bytes()))
        || capture
            .headers
            .iter()
            .any(|(name, value)| found(name.as_bytes()) || found(value))
}

/// Asserts that node A's journal holds the one commit, byte-identical to
/// `sent`, that no request carried the identifier, the client's credential or
/// a federation header outside its body, and that node B was never asked.
fn assert_landed_as_sent(nodes: &containers::TwoNodes, sent: &str) -> TestResult {
    let journal = nodes.a.proxy.journal();
    let commits: Vec<_> = journal
        .iter()
        .filter(|capture| capture.method == "POST")
        .collect();
    assert_eq!(1, commits.len(), "one commit reached node A");
    let landed = commits.first().ok_or("one commit")?;
    assert_eq!(
        format!("{API_PATH}/v1/ehr/{EHR_A}/composition"),
        landed.path,
        "the client's path, under the node's own base (N28)"
    );
    assert_eq!(
        sent.as_bytes(),
        landed.body.as_slice(),
        "byte-identical, the DV_IDENTIFIER included (track 10)"
    );
    for capture in &journal {
        assert!(
            !outside_the_body(capture, PATIENT.value().as_bytes()),
            "no identifier outside the body (N33)"
        );
        assert!(
            !outside_the_body(capture, b"synthetic-client-token"),
            "the client's credential stays at the gateway"
        );
        assert!(
            !outside_the_body(capture, b"openehr-federation"),
            "the federation's own headers stay at the gateway"
        );
    }
    assert!(nodes.b.proxy.journal().is_empty(), "node B is never asked");
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn a_composition_committed_through_the_gateway_lands_byte_identical_at_one_node() -> TestResult
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    let only_the_ehr = SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id: EHR_A,
            subject: Some(PATIENT),
        }],
        template: true,
        compositions: Vec::new(),
    };
    seed::seed(&nodes.a.api_root(), &only_the_ehr).await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &nodes.a, &nodes.b)?;

    let sent = composition_carrying(PATIENT)?;
    let commit = routed_to_a(
        http::Method::POST,
        &format!("/v1/ehr/{EHR_A}/composition"),
        Body::from(sent.clone()),
    )?;
    let response = crate::support::send(app.clone(), commit).await?;
    let (status, headers) = (response.status(), response.headers().clone());
    let body = axum::body::to_bytes(response.into_body(), 256 * 1024).await?;
    assert_eq!(
        StatusCode::CREATED,
        status,
        "{}",
        String::from_utf8_lossy(&body)
    );
    assert_eq!(
        Some("node-a-pub"),
        field(&headers, "openEHR-federation-endpoint"),
        "N31"
    );
    assert_eq!(
        Some(containers::NODE_A_SYSTEM_ID),
        field(&headers, "openEHR-federation-system-id"),
        "§9.6"
    );
    let etag = field(&headers, "etag").ok_or("the node's ETag")?.to_owned();
    let version_uid = etag.trim_start_matches("W/").trim_matches('"');
    assert!(
        version_uid.contains(&format!("::{}::", containers::NODE_A_SYSTEM_ID)),
        "the ETag is node A's OBJECT_VERSION_ID, never rewritten (N22): {etag}"
    );
    let location = field(&headers, "location").ok_or("the node's Location")?;
    assert!(
        location.starts_with(API_PATH) && location.ends_with(version_uid),
        "the Location is the node's own, unmodified (N31): {location}"
    );

    assert_landed_as_sent(&nodes, &sent)?;

    let read = routed_to_a(
        http::Method::GET,
        &format!("/v1/ehr/{EHR_A}/composition/{version_uid}"),
        Body::empty(),
    )?;
    let response = crate::support::send(app, read).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some(etag.as_str()),
        field(response.headers(), "etag"),
        "the same version, read back"
    );
    assert_eq!(
        Some("node-a-pub"),
        field(response.headers(), "openEHR-federation-endpoint")
    );
    Ok(())
}
