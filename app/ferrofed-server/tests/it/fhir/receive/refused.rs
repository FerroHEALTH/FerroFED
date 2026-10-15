// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A receipt refused before anything reaches a member: the caller's scope,
//! the category, the patient, the document and the request path.

use axum::body::Body;
use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};

use super::{
    OTHER, OTHER_A, OTHER_NAMESPACE, SUBSTANCE, creating, document, gateway, gateway_with, issue,
    post, receiving, rows, untouched,
};
use crate::auth::{bearing, claims, minted};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, settings_with_room};
use crate::fhir::{FHIR, TestResult, UNREACHED_SUPPLIER, fhir_tables_with};
use crate::support::call;

#[tokio::test]
async fn a_read_scope_does_not_admit_a_write() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    for scope in [
        "user/composition-*.r user/aql-*.s",
        "user/composition-*.rus",
        "user/composition-ferrofed.eehrxf.allergy.v1.c",
    ] {
        let mut granted = claims();
        granted.scope = Some(scope.to_owned());
        let request = bearing(post(&document())?, &minted(&granted)?)?;
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{scope}: {text}");
        assert_eq!("forbidden", issue(&text)?.0, "{scope}");
    }
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_category_no_member_receives_is_refused_naming_it() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &fhir_tables_with(UNREACHED_SUPPLIER, "", "")),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("not-supported", code);
    assert!(diagnostics.contains("Patient-Summaries"), "{diagnostics}");
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_patient_the_member_holds_no_ehr_for_is_refused() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-b", EHR_B)]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &receiving("node-a", &[NAMESPACE])),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("not-found", code);
    assert!(diagnostics.contains("node-a"), "{diagnostics}");
    assert!(!text.contains(PATIENT_TAIL), "§5.4.3: never echoed: {text}");
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn identifiers_that_name_two_patients_are_refused() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (OTHER_NAMESPACE, OTHER, "node-a", OTHER_A),
    ]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (
            &crossref,
            &receiving("node-a", &[NAMESPACE, OTHER_NAMESPACE]),
        ),
        &settings_with_room(),
    )?;
    let mixed =
        eps::received_document(&[(NAMESPACE, PATIENT), (OTHER_NAMESPACE, OTHER)], SUBSTANCE);
    let (status, text) = call(app.clone(), post(&mixed)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("business-rule", issue(&text)?.0);
    let unknown_beside = eps::received_document(
        &[
            (NAMESPACE, PATIENT),
            (OTHER_NAMESPACE, "SENTINEL-NONE-55mm"),
        ],
        SUBSTANCE,
    );
    let (status, text) = call(app, post(&unknown_beside)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("business-rule", issue(&text)?.0);
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_patient_named_in_no_resolved_namespace_is_refused() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let elsewhere = eps::received_document(&[("urn:oid:2.999.7", PATIENT)], SUBSTANCE);
    let (status, text) = call(app, post(&elsewhere)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("required", issue(&text)?.0);
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_document_the_component_does_not_read_is_refused_before_any_member() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let collection = document().replacen("\"document\"", "\"collection\"", 1);
    let lab = document().replacen("60591-5", "11502-2", 1);
    let sectionless = document().replacen("\"47519-4\"", "\"00000-0\"", 1);
    for (text, status, code) in [
        ("not json", StatusCode::BAD_REQUEST, "invalid"),
        (collection.as_str(), StatusCode::BAD_REQUEST, "invalid"),
        (
            lab.as_str(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "not-supported",
        ),
        (
            sectionless.as_str(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "required",
        ),
    ] {
        let (answered, body) = call(app.clone(), post(text)?).await?;
        assert_eq!(status, answered, "{body}");
        assert_eq!(code, issue(&body)?.0, "{body}");
    }
    let (status, text) = call(
        app,
        Request::post(format!("{FHIR}/Bundle"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(document()))?,
    )
    .await?;
    assert_eq!(StatusCode::UNSUPPORTED_MEDIA_TYPE, status, "{text}");
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_variant_of_the_bundle_path_is_refused_at_the_gate() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    for path in [
        format!("{FHIR}/bundle"),
        format!("{FHIR}/Bundle/"),
        format!("{FHIR}//Bundle"),
        format!("{FHIR}/%42undle"),
        format!("{FHIR}/Bundle/synthetic"),
    ] {
        let request = Request::post(&path)
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(document()))?;
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{path}: {text}");
    }
    let (status, text) = call(
        app,
        Request::get(format!("{FHIR}/Bundle")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(
        StatusCode::FORBIDDEN,
        status,
        "a read of the end-point: {text}"
    );
    assert!(untouched(&a, &b).await?);
    Ok(())
}
