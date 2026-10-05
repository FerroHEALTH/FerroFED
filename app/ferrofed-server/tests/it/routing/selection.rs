// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The endpoint the routing headers select, and requests naming none or two (CP-28).

use axum::body::Body;
use http::{Method, Request, StatusCode};
use wiremock::ResponseTemplate;

use crate::declared::composition_at;
use crate::facade::{EHR_A, wire};
use crate::support::{error_body, send};

use super::{
    ENDPOINT_A, TestResult, VERSION_A, composition, gateway_over, names_node_a, node, parts,
    refused, silent,
};

#[tokio::test]
async fn a_write_that_names_no_node_is_a_400_target_required_and_probes_nobody() -> TestResult {
    for verb in [Method::POST, Method::PUT, Method::DELETE] {
        let at = if verb == Method::POST {
            format!("/v1/ehr/{EHR_A}/composition")
        } else {
            composition_at(&verb, VERSION_A)
        };
        let request = Request::builder()
            .method(verb.clone())
            .uri(at)
            .body(Body::from(composition()))?;
        assert_eq!(
            (StatusCode::BAD_REQUEST, "target-required".to_owned()),
            refused(request, "").await?,
            "{verb} (§12.5.1, N41)"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_endpoint_header_names_exactly_one_endpoint_the_registry_holds() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    for (named, code) in [
        ("node-c-pub", "endpoint-unknown"),
        ("node-a", "endpoint-unknown"),
        ("", "endpoint-unknown"),
        ("node-a-pub, node-b-pub", "endpoint-several"),
    ] {
        let request = Request::get(&resource)
            .header("openEHR-federation-endpoint", named)
            .body(Body::empty())?;
        assert_eq!(
            (StatusCode::BAD_REQUEST, code.to_owned()),
            refused(request, "").await?,
            "{named:?} (§8.4.1)"
        );
    }
    let repeated = Request::get(&resource)
        .header("openEHR-federation-endpoint", "node-a-pub")
        .header("openEHR-federation-endpoint", "node-b-pub")
        .body(Body::empty())?;
    assert_eq!(
        (StatusCode::BAD_REQUEST, "endpoint-several".to_owned()),
        refused(repeated, "").await?
    );
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn the_organisation_header_routes_to_the_one_endpoint_it_manages() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let a = node(
        "GET",
        resource.clone(),
        ResponseTemplate::new(200).set_body_raw(b"{}".to_vec(), "application/json"),
    )
    .await;
    let b = silent().await;
    for fields in [
        vec![("openEHR-federation-organisation", "org-a")],
        vec![
            ("openEHR-federation-endpoint", ENDPOINT_A),
            ("openEHR-federation-organisation", "org-a"),
        ],
    ] {
        let dir = tempfile::tempdir()?;
        let mut request = Request::get(&resource);
        for (name, value) in &fields {
            request = request.header(*name, *value);
        }
        let (status, headers, _) = parts(
            send(
                gateway_over(dir.path(), &a.uri(), &b.uri(), "")?,
                request.body(Body::empty())?,
            )
            .await?,
        )
        .await?;
        assert_eq!(StatusCode::OK, status, "§8.4: {fields:?}");
        names_node_a(&headers, "GET by organisation");
    }
    let captured = wire(&a).await?;
    for absent in [
        "openEHR-federation-endpoint",
        "openEHR-federation-organisation",
        "org-a",
        ENDPOINT_A,
    ] {
        assert!(
            !captured.contains_ignoring_ascii_case(absent),
            "§8.4: the node received {absent:?}: {captured}"
        );
    }
    assert!(b.received_requests().await.ok_or("recording")?.is_empty());
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn the_routing_headers_select_one_endpoint_and_never_two_sets() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let more = "\n[[organisation]]\nid = \"org-c\"\n\n[[endpoint]]\nid = \"node-a-two\"\nnode = \"node-a\"\nurl = \"http://127.0.0.1:9\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n";
    for (fields, answer) in [
        (
            vec![
                ("openEHR-federation-endpoint", ENDPOINT_A),
                ("openEHR-federation-organisation", "org-b"),
            ],
            (StatusCode::BAD_REQUEST, "targeting-conflict"),
        ),
        (
            vec![("openEHR-federation-organisation", "org-a")],
            (StatusCode::BAD_REQUEST, "endpoint-several"),
        ),
        (
            vec![("openEHR-federation-organisation", "org-z")],
            (StatusCode::BAD_REQUEST, "organisation-unknown"),
        ),
        (
            vec![("openEHR-federation-organisation", "org-c")],
            (StatusCode::NOT_FOUND, "no-destination"),
        ),
    ] {
        let mut request = Request::get(&resource);
        for (name, value) in &fields {
            request = request.header(*name, *value);
        }
        let (status, code) = refused(request.body(Body::empty())?, more).await?;
        assert_eq!(
            (answer.0, answer.1.to_owned()),
            (status, code),
            "{fields:?}"
        );
    }
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn a_node_named_in_a_query_parameter_is_refused_and_never_routed() -> TestResult {
    let at = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    for query in ["endpoint=node-b-pub", "organisation=org-b"] {
        let request = Request::get(format!("{at}?{query}"))
            .header("openEHR-federation-endpoint", ENDPOINT_A)
            .body(Body::empty())?;
        let a = silent().await;
        let b = silent().await;
        let dir = tempfile::tempdir()?;
        let (status, _, body) =
            parts(send(gateway_over(dir.path(), &a.uri(), &b.uri(), "")?, request).await?).await?;
        assert_eq!(
            (
                StatusCode::BAD_REQUEST,
                "query-parameter-refused".to_owned()
            ),
            (status, error_body(&String::from_utf8(body)?)?.code),
            "§8.4: ?{query} is no targeting mechanism"
        );
        for server in [&a, &b] {
            assert!(
                server
                    .received_requests()
                    .await
                    .ok_or("recording")?
                    .is_empty(),
                "nothing is sent for ?{query}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_suspended_endpoint_is_never_contacted() -> TestResult {
    let suspended = "\n[[endpoint]]\nid = \"node-a-old\"\nnode = \"node-a\"\nurl = \"http://127.0.0.1:9\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\nstatus = \"suspended\"\n";
    let request = Request::get(format!("/v1/ehr/{EHR_A}"))
        .header("openEHR-federation-endpoint", "node-a-old")
        .body(Body::empty())?;
    assert_eq!(
        (StatusCode::NOT_FOUND, "no-destination".to_owned()),
        refused(request, suspended).await?
    );
    Ok(())
}
