// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the FHIR face refuses: a caller the gate does not admit, a caller
//! whose scopes do not cover the section queries, a request that names the
//! patient by anything but an identifier with its system, and every path
//! the face does not serve; each answered with an `OperationOutcome`, and
//! none reaching a node.

use axum::body::Body;
use http::{Request, StatusCode, header};
use serde_json::Value;

use super::{FHIR, TestResult, UID_A, UID_B, gateway, node_holding, summary, supplier};
use crate::facade::{NAMESPACE, PATIENT, PATIENT_TAIL, received};
use crate::support::{call, claims, exchange, field, issuer, send_as_is};

/// The issue type of the `OperationOutcome` `text` holds.
fn issue(text: &str) -> Result<String, Box<dyn std::error::Error>> {
    let outcome: Value = serde_json::from_str(text)?;
    if outcome.get("resourceType") != Some(&Value::from("OperationOutcome")) {
        return Err(format!("no OperationOutcome: {text}").into());
    }
    Ok(outcome
        .pointer("/issue/0/code")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

#[tokio::test]
async fn a_request_with_no_credential_is_refused_with_a_challenge() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let response = send_as_is(app, summary("")?).await?;
    let status = response.status();
    let challenge = response
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "§13.1, N25: {text}");
    assert!(challenge.is_some_and(|value| value.starts_with("Bearer")));
    assert_eq!("login", issue(&text)?);
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_caller_whose_scopes_miss_the_section_queries_is_forbidden() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let mut claims = claims();
    claims.scope = Some(String::from(
        "user/composition-*.cruds user/aql-org.example::*.s",
    ));
    let token = issuer().mint(&claims)?;
    let mut request = summary("")?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!("forbidden", issue(&text)?);
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_scope_over_the_reserved_namespace_covers_the_summary() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let mut claims = claims();
    claims.scope = Some(String::from("user/aql-eu.ferrofed.eehrxf::*.s"));
    let token = issuer().mint(&claims)?;
    let mut request = summary("")?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn a_patient_named_by_demographics_or_no_system_is_refused() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    for query in [
        String::from("?family=Synthetic&birthdate=1970-01-01"),
        format!("?identifier={PATIENT}"),
        format!(
            "?identifier={}%7C{PATIENT}&profile=http://hl7.org/fhir/uv/ips/StructureDefinition/Composition-uv-ips",
            NAMESPACE.replace(':', "%3A")
        ),
    ] {
        let request = Request::get(format!("{FHIR}/Patient/$summary{query}")).body(Body::empty())?;
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {text}");
        assert_eq!("invalid", issue(&text)?, "{query}");
        assert!(!text.contains(PATIENT_TAIL), "§5.4.3: never echoed: {text}");
    }
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_parameters_body_asks_as_the_query_string_does() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let body = format!(
        r#"{{"resourceType":"Parameters","parameter":[{{"name":"identifier","valueString":"{NAMESPACE}|{PATIENT}"}}]}}"#
    );
    let post = |media: &str| {
        Request::post(format!("{FHIR}/Patient/$summary"))
            .header(header::CONTENT_TYPE, media)
            .body(Body::from(body.clone()))
    };
    let (status, text) = call(app.clone(), post("application/fhir+json")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (status, text) = call(app, post("text/plain")?).await?;
    assert_eq!(StatusCode::UNSUPPORTED_MEDIA_TYPE, status, "{text}");
    assert_eq!("not-supported", issue(&text)?);
    Ok(())
}

#[tokio::test]
async fn a_path_the_face_does_not_serve_is_refused_at_the_gate() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    for path in [
        format!("{FHIR}/Patient/{PATIENT}"),
        format!("{FHIR}/Patient/{PATIENT}/$summary"),
        format!("{FHIR}/Observation"),
    ] {
        let (status, headers, body) =
            exchange(app.clone(), Request::get(&path).body(Body::empty())?).await?;
        let text = String::from_utf8(body)?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
        assert_eq!(
            field(&headers, header::CONTENT_TYPE.as_str()),
            Some("application/fhir+json")
        );
        assert_eq!("forbidden", issue(&text)?);
        assert!(!text.contains(PATIENT_TAIL), "§5.4.3: never echoed: {text}");
    }
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

/// The summary request for the test patient with `claims`, its credential
/// minted by the test issuer.
fn summary_as(
    claims: &ferrofed_testkit::issuer::Claims,
) -> Result<Request<Body>, Box<dyn std::error::Error>> {
    let mut request = summary("")?;
    let token = issuer().mint(claims)?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
    Ok(request)
}

#[tokio::test]
async fn a_token_with_no_purpose_of_use_is_refused() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let mut purposeless = claims();
    purposeless.extensions = None;
    let (status, text) = call(app, summary_as(&purposeless)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "§13.4: {text}");
    assert!(text.contains("purpose-of-use-required"), "{text}");
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_client_that_names_no_professional_is_refused() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let mut client = claims();
    client.sub.clone_from(&client.client_id);
    let (status, text) = call(app, summary_as(&client)?).await?;
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status,
        "Annex II 3.1, RFC 9470 §3: {text}"
    );
    assert!(text.contains("natural-person-required"), "{text}");
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn no_variant_of_the_summary_path_or_method_passes_the_gate() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let identifier = format!("identifier={}%7C{PATIENT}", NAMESPACE.replace(':', "%3A"));
    let mut requests = Vec::new();
    for path in [
        "/fhir/Patient/$summary/",
        "/fhir/Patient/%24summary",
        "/fhir/patient/$summary",
        "/fhir/Patient/$SUMMARY",
        "/FHIR/Patient/$summary",
        "/Fhir/Patient/$summary",
        "/fhir//Patient/$summary",
        "/fhir/%50atient/$summary",
        "/%66hir/Patient/$summary",
        "/fhir%2FPatient/$summary",
        "/fhir/Patient/$summary;x",
        "/fhir/./Patient/$summary",
        "/fhir/metadata/../Patient/$summary",
    ] {
        requests.push(Request::get(format!("{path}?{identifier}")).body(Body::empty())?);
    }
    for verb in [
        http::Method::HEAD,
        http::Method::PUT,
        http::Method::DELETE,
        http::Method::PATCH,
    ] {
        requests.push(
            Request::builder()
                .method(verb)
                .uri(format!("{FHIR}/Patient/$summary?{identifier}"))
                .body(Body::empty())?,
        );
    }
    for format in ["xml", "application/fhir%2Bxml", "turtle"] {
        requests.push(
            Request::get(format!(
                "{FHIR}/Patient/$summary?{identifier}&_format={format}"
            ))
            .body(Body::empty())?,
        );
    }
    requests.push(
        Request::get(format!("{FHIR}/Patient/$summary?{identifier}&{identifier}"))
            .body(Body::empty())?,
    );
    for request in requests {
        let shown = format!("{} {}", request.method(), request.uri().path());
        let (status, text) = call(app.clone(), request).await?;
        assert!(
            status.is_client_error(),
            "{shown}: refused, never served: {status} {text}"
        );
        assert!(!text.contains(PATIENT_TAIL), "§5.4.3: never echoed: {text}");
    }
    assert!(
        received(&a).await?.is_empty() && received(&b).await?.is_empty(),
        "no variant reached a node"
    );
    Ok(())
}

#[tokio::test]
async fn every_path_under_the_face_is_refused_with_no_credential() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    for path in [
        "/fhir/Patient/$summary",
        "/fhir/Patient/$summary/",
        "/fhir/Patient/%24summary",
        "/FHIR/Patient/$summary",
        "/fhir/metadata",
        "/fhir/anything",
    ] {
        let response = send_as_is(app.clone(), Request::get(path).body(Body::empty())?).await?;
        assert!(
            matches!(
                response.status(),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
            ),
            "{path}: {}",
            response.status()
        );
    }
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}
