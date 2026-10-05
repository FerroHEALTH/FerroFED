// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `CONTRIBUTION` that amends versions reaches only the CDR that controls
//! every one of them (§12.4, §12a.1, §10.3, N23, CP-15).
//!
//! `POST {base}/v1/ehr/{ehr_id}/contribution` names the versions it amends in
//! its body, as each version's `preceding_version_uid` (ITS-REST 1.1.0 EHR
//! API, `contribution_create`). The gateway routes it by its path `ehr_id`
//! (§12a.1 `route-ehr`, N41), reads the body with the ITS-REST
//! `NewContribution` type, and sends it only when the path node controls each
//! amended version; the body it sends is the one it received. Every
//! assertion on what a node received reads the node's own capture (§16,
//! track 6 and track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Method, Request, StatusCode, header};
use wiremock::ResponseTemplate;

use crate::facade::{EHR_A, EHR_B, PATIENT};
use crate::path_ehr_id::{answer, holder};
use crate::support::{asked, error_body, mount, without_minted};
use crate::versioned_write::{
    CREATED_AT_A, CREATED_BY_LEGACY, CREATED_ELSEWHERE, ENDPOINT_A, ENDPOINT_B, LEGACY_MAPPING,
    outside_bodies, over, refused_at_neither, versioned,
};

type TestResult = Result<(), Box<dyn Error>>;

/// The error body `text` with the request id the gateway minted for it
/// masked, which a search for a fragment of a version uid reads.
fn without_request_id(text: &str) -> Result<String, Box<dyn Error>> {
    without_minted(text, [error_body(text)?.request_id.as_str()])
}

/// A version node B created.
const CREATED_AT_B: &str = "3f2e1d0c-9b8a-4c7d-8e6f-5a4b3c2d1e0f::cdr-b.example.org::2";

/// The `CONTRIBUTION` of `ehr_id`, as ITS-REST 1.1.0 addresses it.
fn contribution_of(ehr_id: &str) -> String {
    format!("/v1/ehr/{ehr_id}/contribution")
}

/// A coded text of the openEHR terminology.
fn coded(value: &str, code: &str) -> String {
    format!(
        r#"{{"_type":"DV_CODED_TEXT","value":"{value}","defining_code":{{"_type":"CODE_PHRASE","terminology_id":{{"_type":"TERMINOLOGY_ID","value":"openehr"}},"code_string":"{code}"}}}}"#
    )
}

/// An `UPDATE_AUDIT` of `change`, committed by a synthetic clinician.
fn audit(change: &str) -> String {
    format!(
        r#"{{"_type":"UPDATE_AUDIT","change_type":{change},"committer":{{"_type":"PARTY_IDENTIFIED","name":"Synthetic clinician"}}}}"#
    )
}

/// A `COMPOSITION` whose composer carries the patient's identifier as a
/// `DV_IDENTIFIER`, spaced as a re-serialisation would not keep it.
fn composition() -> String {
    format!(
        "{{ \"_type\":\"COMPOSITION\",\n   \"name\":{{\"_type\":\"DV_TEXT\",\"value\":\"Synthetic note é\"}},\n   \"archetype_node_id\":\"openEHR-EHR-COMPOSITION.report.v1\",\n   \"language\":{{\"_type\":\"CODE_PHRASE\",\"terminology_id\":{{\"_type\":\"TERMINOLOGY_ID\",\"value\":\"ISO_639-1\"}},\"code_string\":\"en\"}},\n   \"territory\":{{\"_type\":\"CODE_PHRASE\",\"terminology_id\":{{\"_type\":\"TERMINOLOGY_ID\",\"value\":\"ISO_3166-1\"}},\"code_string\":\"NL\"}},\n   \"category\":{},\n   \"composer\":{{\"_type\":\"PARTY_IDENTIFIED\",\"name\":\"Synthetic clinician\",\"identifiers\":[{{\"_type\":\"DV_IDENTIFIER\",\"issuer\":\"urn:oid:2.999.1\",\"id\":\"{PATIENT}\",\"type\":\"MR\"}}]}}\n}}",
        coded("event", "433")
    )
}

/// One version of a `CONTRIBUTION` committing `data`: an amendment of
/// `preceding` where given, a creation otherwise, written with its
/// `preceding_version_uid` as the literal `uid` JSON.
fn version_carrying(preceding: Option<&str>, data: &str) -> String {
    let (preceding, change) = match preceding {
        Some(uid) => (
            format!("\"preceding_version_uid\":{uid},\n     "),
            coded("modification", "251"),
        ),
        None => (String::new(), coded("creation", "249")),
    };
    format!(
        "{{ {preceding}\"lifecycle_state\":{},\n     \"commit_audit\":{},\n     \"data\":{data} }}",
        coded("complete", "532"),
        audit(&change),
    )
}

/// One version of a `CONTRIBUTION` committing a canonical `COMPOSITION`,
/// written with its `preceding_version_uid` as the literal `uid` JSON.
fn version_with(preceding: Option<&str>) -> String {
    version_carrying(preceding, &composition())
}

/// The `OBJECT_VERSION_ID` `uid` in canonical JSON.
fn object_version_id(uid: &str) -> String {
    format!(r#"{{"_type":"OBJECT_VERSION_ID","value":"{uid}"}}"#)
}

/// One version of a `CONTRIBUTION`, amending the version `preceding` names
/// where given.
fn version(preceding: Option<&str>) -> String {
    version_with(preceding.map(object_version_id).as_deref())
}

/// The Simplified Formats media types, each with the `data` of one version
/// in that format, whose composer carries the patient's identifier
/// (ITS-REST 1.1.0 overview, §Simplified Formats).
fn simplified() -> [(&'static str, String); 2] {
    [
        (
            "application/openehr.wt.flat+json",
            format!(
                "{{ \"report/composer|name\": \"Synthetic clinician é\",\n       \"report/composer|id\": \"{PATIENT}\" }}"
            ),
        ),
        (
            "application/openehr.wt.structured+json",
            format!(
                "{{ \"report\": {{ \"composer\": [ {{ \"|name\": \"Synthetic clinician é\",\n       \"|id\": \"{PATIENT}\" }} ] }} }}"
            ),
        ),
    ]
}

/// One version of a `CONTRIBUTION` whose `data` is `data`, in a Simplified
/// Format, amending the version `preceding` names where given.
fn simplified_version(preceding: Option<&str>, data: &str) -> String {
    version_carrying(preceding.map(object_version_id).as_deref(), data)
}

/// `sent` posted to `at` as `media`, naming `endpoint` as its target.
fn posted(at: &str, media: &str, endpoint: &str, sent: &str) -> Result<Request<Body>, http::Error> {
    Request::post(at)
        .header(header::CONTENT_TYPE, media)
        .header("openEHR-federation-endpoint", endpoint)
        .body(Body::from(sent.to_owned()))
}

/// A `CONTRIBUTION` committing `versions` (ITS-REST 1.1.0 `NewContribution`).
fn contribution(versions: &[String]) -> String {
    format!(
        "{{\n  \"versions\": [\n    {}\n  ],\n  \"audit\": {}\n}}\n",
        versions.join(",\n    "),
        audit(&coded("modification", "251"))
    )
}

// conformance: CP-15 CP-24 CP-26
#[tokio::test]
async fn a_contribution_its_path_node_controls_reaches_it_once_byte_identical() -> TestResult {
    let at = contribution_of(EHR_A);
    let a = Server::start().await;
    mount(&a, "POST", at.clone(), ResponseTemplate::new(201)).await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let sent = contribution(&[version(Some(CREATED_AT_A)), version(None)]);
    let mut request = versioned(&Method::POST, &at, Some(ENDPOINT_A), None, &sent)?;
    request.headers_mut().insert("x-patient", PATIENT.parse()?);
    let (status, acting, text) = answer(over(dir.path(), &a, &b, "")?, request).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(Some(ENDPOINT_A), acting.as_deref(), "N31");
    assert_eq!(
        vec![("POST".to_owned(), at)],
        asked(&a).await?,
        "the controlling CDR is sent the CONTRIBUTION once (§12.4, N23)"
    );
    let requests = a.received_requests().await.ok_or("recording is on")?;
    let received = requests.first().ok_or("node A was sent the CONTRIBUTION")?;
    assert_eq!(
        sent.as_bytes(),
        received.body.as_slice(),
        "the body lands byte-identical, its DV_IDENTIFIER included (N22, track 10)"
    );
    let composed = outside_bodies(&a).await?;
    assert!(!composed.contains(PATIENT), "N33: {composed}");
    assert!(asked(&b).await?.is_empty(), "no other node is contacted");
    Ok(())
}

// conformance: CP-15 CP-13
#[tokio::test]
async fn a_contribution_amending_a_registered_mapping_s_version_reaches_its_node() -> TestResult {
    let at = contribution_of(EHR_A);
    let a = Server::start().await;
    mount(&a, "POST", at.clone(), ResponseTemplate::new(201)).await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let sent = contribution(&[
        version(Some(CREATED_AT_A)),
        version(Some(CREATED_BY_LEGACY)),
    ]);
    let request = versioned(&Method::POST, &at, Some(ENDPOINT_A), None, &sent)?;
    let (status, acting, text) = answer(over(dir.path(), &a, &b, LEGACY_MAPPING)?, request).await?;
    assert_eq!(StatusCode::CREATED, status, "N21, §12.2: {text}");
    assert_eq!(Some(ENDPOINT_A), acting.as_deref());
    assert_eq!(vec![("POST".to_owned(), at)], asked(&a).await?);
    assert!(asked(&b).await?.is_empty());
    Ok(())
}

// conformance: CP-15
#[tokio::test]
async fn a_contribution_of_creations_alone_routes_by_its_path_ehr_id() -> TestResult {
    let at = contribution_of(EHR_B);
    let a = Server::start().await;
    let b = Server::start().await;
    mount(&b, "POST", at.clone(), ResponseTemplate::new(201)).await;
    let dir = tempfile::tempdir()?;
    let sent = contribution(&[version(None), version(None)]);
    let request = versioned(&Method::POST, &at, Some(ENDPOINT_B), None, &sent)?;
    let (status, acting, text) = answer(over(dir.path(), &a, &b, "")?, request).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(Some(ENDPOINT_B), acting.as_deref());
    assert_eq!(vec![("POST".to_owned(), at)], asked(&b).await?);
    let requests = b.received_requests().await.ok_or("recording is on")?;
    let received = requests.first().ok_or("node B was sent the CONTRIBUTION")?;
    assert_eq!(sent.as_bytes(), received.body.as_slice(), "N22");
    assert!(asked(&a).await?.is_empty());
    Ok(())
}

// conformance: CP-15
#[tokio::test]
async fn a_contribution_with_one_version_another_member_controls_is_409_and_reaches_no_node()
-> TestResult {
    for amended in [
        vec![version(Some(CREATED_AT_A)), version(Some(CREATED_AT_B))],
        vec![version(Some(CREATED_AT_B)), version(None)],
    ] {
        let a = Server::start().await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        let request = versioned(
            &Method::POST,
            &contribution_of(EHR_A),
            Some(ENDPOINT_A),
            None,
            &contribution(&amended),
        )?;
        let text = refused_at_neither(
            over(dir.path(), &a, &b, "")?,
            request,
            (StatusCode::CONFLICT, "controlling-system-unreachable"),
            (&a, &b),
        )
        .await?;
        assert!(
            text.contains("cdr-b.example.org")
                && text.contains("node-b")
                && text.contains(ENDPOINT_B),
            "the error identifies the controlling system (§10.3): {text}"
        );
        let searched = without_request_id(&text)?;
        assert!(
            !text.contains(EHR_A) && !searched.contains("3f2e1d0c") && !text.contains(PATIENT),
            "no value of the request is quoted: {text}"
        );
    }
    Ok(())
}

// conformance: CP-15
#[tokio::test]
async fn a_contribution_amending_a_version_no_member_is_known_to_control_is_409() -> TestResult {
    let a = Server::start().await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let sent = contribution(&[
        version(Some(CREATED_AT_A)),
        version(Some(CREATED_ELSEWHERE)),
    ]);
    let request = versioned(
        &Method::POST,
        &contribution_of(EHR_A),
        Some(ENDPOINT_A),
        None,
        &sent,
    )?;
    let text = refused_at_neither(
        over(dir.path(), &a, &b, "")?,
        request,
        (StatusCode::CONFLICT, "controlling-system-unreachable"),
        (&a, &b),
    )
    .await?;
    assert!(
        !text.contains("external.example.org"),
        "the client's creating_system_id is not quoted: {text}"
    );
    assert!(
        text.contains("the preceding_version_uid of version 2 of the CONTRIBUTION"),
        "the error points at the controlling system it cannot quote (§10.3): {text}"
    );
    Ok(())
}

// conformance: CP-15
#[tokio::test]
async fn a_body_that_is_no_contribution_naming_object_version_ids_is_a_400_before_any_node()
-> TestResult {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "a preceding_version_uid that is no OBJECT_VERSION_ID",
            contribution(&[version(Some("not a version"))]).into_bytes(),
        ),
        (
            "a preceding_version_uid that is a bare HIER_OBJECT_ID",
            contribution(&[version(Some("8849182c-82ad-4088-a07f-48ead4180515"))]).into_bytes(),
        ),
        (
            "a preceding_version_uid that is a string",
            contribution(&[version_with(Some(&format!("\"{CREATED_AT_A}\"")))]).into_bytes(),
        ),
        ("a COMPOSITION", composition().into_bytes()),
        ("an empty object", b"{}".to_vec()),
        ("no JSON", b"versions: none".to_vec()),
        ("no body", Vec::new()),
        ("no UTF-8", vec![0x7b, 0xff, 0xfe, 0x7d]),
    ];
    for (case, sent) in cases {
        let a = Server::start().await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        let request = Request::post(contribution_of(EHR_A))
            .header(header::CONTENT_TYPE, "application/json")
            .header("openEHR-federation-endpoint", ENDPOINT_A)
            .body(Body::from(sent))?;
        let text = refused_at_neither(
            over(dir.path(), &a, &b, "")?,
            request,
            (StatusCode::BAD_REQUEST, "preceding-version-invalid"),
            (&a, &b),
        )
        .await
        .map_err(|failed| format!("{case}: {failed}"))?;
        assert!(
            !text.contains("not a version") && !text.contains(PATIENT),
            "{case}: the body is not quoted: {text}"
        );
    }
    Ok(())
}

/// Asserts that the node `server` was sent `sent` once, byte-identical, as
/// `media`, and nothing composed by the gateway carries the patient's
/// identifier.
async fn sent_once_as(server: &Server, media: &str, sent: &str) -> TestResult {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let [received] = requests.as_slice() else {
        return Err(format!("{media}: one request, not {}", requests.len()).into());
    };
    assert_eq!(
        sent.as_bytes(),
        received.body.as_slice(),
        "{media}: the body lands byte-identical, the patient's identifier in its data included (N22, track 10)"
    );
    assert_eq!(
        Some(media),
        received
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        "{media}: the node is told the representation the client declared"
    );
    let composed = outside_bodies(server).await?;
    assert!(!composed.contains(PATIENT), "{media}: N33: {composed}");
    Ok(())
}

/// A `CONTRIBUTION` whose versions' `data` is FLAT or STRUCTURED keeps a
/// canonical envelope (ITS-REST 1.1.0 `contribution_create`), so one of
/// creations alone routes by its path `ehr_id` as a canonical one does.
// conformance: CP-15
#[tokio::test]
async fn a_simplified_contribution_of_creations_alone_routes_by_its_path_ehr_id() -> TestResult {
    for (media, data) in simplified() {
        let at = contribution_of(EHR_B);
        let a = Server::start().await;
        let b = Server::start().await;
        mount(&b, "POST", at.clone(), ResponseTemplate::new(201)).await;
        let dir = tempfile::tempdir()?;
        let sent = contribution(&[
            simplified_version(None, &data),
            simplified_version(None, &data),
        ]);
        let request = posted(&at, media, ENDPOINT_B, &sent)?;
        let (status, acting, text) = answer(over(dir.path(), &a, &b, "")?, request).await?;
        assert_eq!(StatusCode::CREATED, status, "{media}: {text}");
        assert_eq!(Some(ENDPOINT_B), acting.as_deref(), "{media}");
        assert_eq!(vec![("POST".to_owned(), at)], asked(&b).await?, "{media}");
        sent_once_as(&b, media, &sent).await?;
        assert!(asked(&a).await?.is_empty(), "{media}");
    }
    Ok(())
}

/// A FLAT or STRUCTURED `CONTRIBUTION` whose amended versions the path node
/// controls reaches it once, as a canonical one does (§12.4, §12a.1
/// `route-write`, N23).
// conformance: CP-15 CP-26
#[tokio::test]
async fn a_simplified_contribution_its_path_node_controls_reaches_it_once_byte_identical()
-> TestResult {
    for (media, data) in simplified() {
        let at = contribution_of(EHR_A);
        let a = Server::start().await;
        mount(&a, "POST", at.clone(), ResponseTemplate::new(201)).await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        let sent = contribution(&[
            simplified_version(Some(CREATED_AT_A), &data),
            simplified_version(None, &data),
        ]);
        let request = posted(&at, media, ENDPOINT_A, &sent)?;
        let (status, acting, text) = answer(over(dir.path(), &a, &b, "")?, request).await?;
        assert_eq!(StatusCode::CREATED, status, "{media}: {text}");
        assert_eq!(Some(ENDPOINT_A), acting.as_deref(), "{media}: N31");
        assert_eq!(
            vec![("POST".to_owned(), at)],
            asked(&a).await?,
            "{media}: the controlling CDR is sent the CONTRIBUTION once (§12.4, N23)"
        );
        sent_once_as(&a, media, &sent).await?;
        assert!(
            asked(&b).await?.is_empty(),
            "{media}: no other node is contacted"
        );
    }
    Ok(())
}

/// A FLAT or STRUCTURED `CONTRIBUTION` with one amended version another
/// member controls is refused as a canonical one is (§10.3
/// `copy-write-reject`, §12a.1 `route-write`, N23).
// conformance: CP-15
#[tokio::test]
async fn a_simplified_contribution_with_one_version_another_member_controls_is_409() -> TestResult {
    for (media, data) in simplified() {
        let a = Server::start().await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        let sent = contribution(&[
            simplified_version(Some(CREATED_AT_A), &data),
            simplified_version(Some(CREATED_AT_B), &data),
        ]);
        let request = posted(&contribution_of(EHR_A), media, ENDPOINT_A, &sent)?;
        let text = refused_at_neither(
            over(dir.path(), &a, &b, "")?,
            request,
            (StatusCode::CONFLICT, "controlling-system-unreachable"),
            (&a, &b),
        )
        .await
        .map_err(|failed| format!("{media}: {failed}"))?;
        assert!(
            text.contains("cdr-b.example.org")
                && text.contains("node-b")
                && text.contains(ENDPOINT_B),
            "{media}: the error identifies the controlling system (§10.3): {text}"
        );
        let searched = without_request_id(&text)?;
        assert!(
            !text.contains(EHR_A) && !searched.contains("3f2e1d0c") && !text.contains(PATIENT),
            "{media}: no value of the request is quoted: {text}"
        );
    }
    Ok(())
}

/// A body declared FLAT that is no `CONTRIBUTION` with a canonical envelope
/// names no versions the gateway can route by, so it is refused before any
/// node (§12.4, N23).
// conformance: CP-15
#[tokio::test]
async fn a_malformed_flat_contribution_is_a_400_before_any_node() -> TestResult {
    let [(flat, data), _] = simplified();
    let cases: Vec<(&str, String)> = vec![
        (
            "a preceding_version_uid that is no OBJECT_VERSION_ID",
            contribution(&[simplified_version(Some("not a version"), &data)]),
        ),
        (
            "a preceding_version_uid that is a string",
            contribution(&[version_carrying(
                Some(&format!("\"{CREATED_AT_A}\"")),
                &data,
            )]),
        ),
        ("FLAT data with no envelope", data.clone()),
        (
            "an envelope with no audit",
            format!(
                "{{\"versions\":[{}]}}",
                simplified_version(Some(CREATED_AT_A), &data)
            ),
        ),
        ("no JSON", "versions: none".to_owned()),
        ("no body", String::new()),
    ];
    for (case, sent) in cases {
        let a = Server::start().await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        let request = posted(&contribution_of(EHR_A), flat, ENDPOINT_A, &sent)?;
        let text = refused_at_neither(
            over(dir.path(), &a, &b, "")?,
            request,
            (StatusCode::BAD_REQUEST, "preceding-version-invalid"),
            (&a, &b),
        )
        .await
        .map_err(|failed| format!("{case}: {failed}"))?;
        assert!(
            !text.contains("not a version") && !text.contains(PATIENT),
            "{case}: the body is not quoted: {text}"
        );
    }
    Ok(())
}

/// A `CONTRIBUTION` in canonical XML is one the gateway does not read, so it
/// cannot name the versions it amends, and refuses a write it cannot
/// unambiguously route (§12.4, N23).
// TODO(#308): this refusal holds until openehr-its reads a CONTRIBUTION in canonical XML.
// conformance: CP-15
#[tokio::test]
async fn a_contribution_in_canonical_xml_is_never_forwarded() -> TestResult {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<contribution xmlns=\"http://schemas.openehr.org/v1\">\n  <versions>\n    <preceding_version_uid><value>{CREATED_AT_A}</value></preceding_version_uid>\n  </versions>\n</contribution>\n"
    );
    let a = Server::start().await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let request = posted(&contribution_of(EHR_A), "application/xml", ENDPOINT_A, &xml)?;
    let text = refused_at_neither(
        over(dir.path(), &a, &b, "")?,
        request,
        (StatusCode::BAD_REQUEST, "preceding-version-invalid"),
        (&a, &b),
    )
    .await?;
    assert!(
        text.contains("canonical XML"),
        "the refusal says which form it does not read: {text}"
    );
    assert!(
        !without_request_id(&text)?.contains("8849182c"),
        "the body is not quoted: {text}"
    );
    Ok(())
}

// conformance: CP-15 CP-33
#[tokio::test]
async fn a_contribution_without_a_target_is_never_probed_for() -> TestResult {
    let a = holder().await;
    let b = holder().await;
    let dir = tempfile::tempdir()?;
    let sent = contribution(&[version(Some(CREATED_AT_A))]);
    let request = versioned(&Method::POST, &contribution_of(EHR_A), None, None, &sent)?;
    refused_at_neither(
        over(dir.path(), &a, &b, "")?,
        request,
        (StatusCode::BAD_REQUEST, "target-required"),
        (&a, &b),
    )
    .await?;
    Ok(())
}
