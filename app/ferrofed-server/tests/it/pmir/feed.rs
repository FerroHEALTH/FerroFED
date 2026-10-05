// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-93 route: an authenticated merge drops the resolution bindings it
//! could have made stale and is answered with the feed response, and an
//! unauthenticated or malformed message changes nothing (track 8 of §16.3;
//! PMIR §2:3.93.4.1.2, §2:3.93.4.2, §2:3.93.5). The identifiers it carries
//! reach no log line, metric, or answer.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_testkit::pmir::merge_message;
use http::{Request, StatusCode, header};
use serde::Deserialize;

use super::{DOMAIN_A, EHR_A, EHR_A2, Gateway, PATH, TOKEN, gateway, text};
use crate::support::{Logs, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// A value that stands for a patient identifier in another domain.
const SENTINEL: &str = "SENTINEL-4711";

/// A value that stands for the Registry's id of the merged Patient.
const SENTINEL_ID: &str = "sentinel-id-4712";

fn feed_gateway() -> Result<(tempfile::TempDir, Gateway), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let text = text(
        dir.path(),
        "http://127.0.0.1:9/fhir/",
        "http://127.0.0.1:9/pmir/feed",
        "",
    )?;
    let gateway = gateway(&text)?;
    Ok((dir, gateway))
}

/// The ITI-93 request carrying `body`, with `authorization` when given.
fn message(body: String, authorization: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::post(PATH).header(header::CONTENT_TYPE, "application/fhir+json");
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
    request.body(Body::from(body))
}

/// Sends `request` to the gateway's router, with no client credential added,
/// and reads the status and the body.
async fn deliver(
    gateway: &Gateway,
    request: Request<Body>,
) -> Result<(StatusCode, http::HeaderMap, String), Box<dyn Error>> {
    let response = send_as_is(gateway.app.clone(), request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, headers, String::from_utf8(bytes.to_vec())?))
}

/// What the tests read of the feed response.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    r#type: String,
    entry: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    resource: Header,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    event_uri: String,
    response: Answer,
}

#[derive(Deserialize)]
struct Answer {
    identifier: String,
    code: String,
}

#[tokio::test]
async fn an_authenticated_merge_drops_the_bindings_of_the_merged_ehr_ids() -> TestResult {
    let (_dir, gateway) = feed_gateway()?;
    gateway.bind("caller-1", &[EHR_A, EHR_A2])?;
    gateway.bind("caller-2", &[EHR_A])?;
    let body = merge_message("patient-old", &[(DOMAIN_A, EHR_A)], "patient-new")?;
    let (status, _, text) =
        deliver(&gateway, message(body, Some(&format!("Bearer {TOKEN}")))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, gateway.bound()?, "only the other ehr_id stays bound");
    let response: Response = serde_json::from_str(&text)?;
    assert_eq!("message", response.r#type);
    let [entry] = response.entry.as_slice() else {
        return Err("the response holds one MessageHeader (§2:3.93.4.2.2)".into());
    };
    assert_eq!(
        "urn:ihe:iti:pmir:2019:patient-feed-response",
        entry.resource.event_uri
    );
    assert_eq!("message-1", entry.resource.response.identifier);
    assert_eq!("ok", entry.resource.response.code);
    Ok(())
}

#[tokio::test]
async fn a_merge_that_names_no_member_ehr_id_drops_every_binding() -> TestResult {
    let (_dir, gateway) = feed_gateway()?;
    gateway.bind("caller-1", &[EHR_A, EHR_A2])?;
    let body = merge_message(
        "patient-old",
        &[("urn:oid:2.999.1.999", "SYNTHETIC-1")],
        "patient-new",
    )?;
    let (status, _, _) =
        deliver(&gateway, message(body, Some(&format!("Bearer {TOKEN}")))?).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(0, gateway.bound()?);
    Ok(())
}

#[tokio::test]
async fn an_unauthenticated_message_changes_nothing() -> TestResult {
    let (_dir, gateway) = feed_gateway()?;
    gateway.bind("caller-1", &[EHR_A])?;
    let body = merge_message("patient-old", &[(DOMAIN_A, EHR_A)], "patient-new")?;
    for authorization in [
        None,
        Some("Bearer Qz7forged"),
        Some(&*format!("Basic {TOKEN}")),
        Some(&*format!("Bearer {TOKEN}x")),
        Some("Bearer"),
    ] {
        let (status, headers, _) = deliver(&gateway, message(body.clone(), authorization)?).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, status, "{authorization:?}");
        assert!(headers.contains_key(header::WWW_AUTHENTICATE));
        assert_eq!(1, gateway.bound()?, "{authorization:?} applied nothing");
    }
    let mut twice = message(body, Some(&format!("Bearer {TOKEN}")))?;
    twice
        .headers_mut()
        .append(header::AUTHORIZATION, format!("Bearer {TOKEN}").parse()?);
    let (status, _, _) = deliver(&gateway, twice).await?;
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status,
        "one credential, sent once"
    );
    assert_eq!(1, gateway.bound()?);
    Ok(())
}

#[tokio::test]
async fn a_malformed_message_is_refused_and_changes_nothing() -> TestResult {
    let (_dir, gateway) = feed_gateway()?;
    gateway.bind("caller-1", &[EHR_A])?;
    let bearer = format!("Bearer {TOKEN}");
    let merge = merge_message("patient-old", &[(DOMAIN_A, EHR_A)], "patient-new")?;
    let off_profile = merge.replacen("patient-feed", "patient-feed-response", 1);
    let (status, headers, text) = deliver(&gateway, message(off_profile, Some(&bearer))?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status);
    assert_eq!(
        Some("application/fhir+json"),
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
    );
    assert!(text.contains("OperationOutcome"), "{text}");
    let mut xml = message(merge, Some(&bearer))?;
    xml.headers_mut()
        .insert(header::CONTENT_TYPE, "application/fhir+xml".parse()?);
    let (status, _, _) = deliver(&gateway, xml).await?;
    assert_eq!(StatusCode::UNSUPPORTED_MEDIA_TYPE, status);
    assert_eq!(1, gateway.bound()?, "nothing was applied");
    Ok(())
}

#[tokio::test]
async fn no_identifier_reaches_a_log_line_a_metric_or_an_answer() -> TestResult {
    let (_dir, gateway) = feed_gateway()?;
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "trace", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let bearer = format!("Bearer {TOKEN}");
    let merge = merge_message(
        SENTINEL_ID,
        &[("urn:oid:2.999.1.999", SENTINEL)],
        "patient-new",
    )?;
    let malformed = merge.replacen(
        r#""status":"200""#,
        &format!(r#""status":"500 {SENTINEL}""#),
        1,
    );
    let mut answers = Vec::new();
    for (body, authorization) in [
        (merge.clone(), Some(bearer.as_str())),
        (malformed, Some(bearer.as_str())),
        (merge, None),
    ] {
        answers.push(deliver(&gateway, message(body, authorization)?).await?.2);
    }
    drop(guard);
    let metrics = gateway.state.metrics().render()?;
    assert!(
        metrics.contains("ferrofed_identity_feed_messages_total"),
        "the feed is counted"
    );
    for (surface, text) in [("the log", logs.text()), ("the metrics", metrics)]
        .into_iter()
        .chain(answers.into_iter().map(|answer| ("an answer", answer)))
    {
        for sentinel in [SENTINEL, SENTINEL_ID, TOKEN] {
            assert!(!text.contains(sentinel), "{sentinel} in {surface}: {text}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn the_feed_counts_each_message_by_result() -> TestResult {
    let (_dir, gateway) = feed_gateway()?;
    let merge = merge_message("patient-old", &[(DOMAIN_A, EHR_A)], "patient-new")?;
    deliver(
        &gateway,
        message(merge.clone(), Some(&format!("Bearer {TOKEN}")))?,
    )
    .await?;
    deliver(&gateway, message(merge, None)?).await?;
    let metrics = gateway.state.metrics().render()?;
    for result in ["applied", "unauthenticated"] {
        let line = metrics
            .lines()
            .find(|line| {
                line.starts_with("ferrofed_identity_feed_messages_total")
                    && line.contains(&format!("result=\"{result}\""))
            })
            .ok_or(format!("a {result} sample: {metrics}"))?;
        assert!(line.ends_with(" 1"), "{line}");
    }
    Ok(())
}
