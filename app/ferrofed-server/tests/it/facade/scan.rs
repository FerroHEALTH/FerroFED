// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node-wire scan itself: [`wire`] records the `x-request-id` the
//! gateway mints as [`MINTED_REQUEST_ID`], and every other value raw, so a
//! search for a synthetic identifier sees it in any other carrier (§5.4.1,
//! N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::mock::Server;

use super::{MINTED_REQUEST_ID, wire};

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic identifier made of digits only, as the scans search for.
const SYNTHETIC: &str = "12345";

/// A version 4 UUID in the form the gateway mints, holding [`SYNTHETIC`] by
/// chance.
const MINTED_BY_CHANCE: &str = "0a12345b-7c3d-4e5f-9a1b-2c3d4e5f6a7b";

/// A mock node that has received one request carrying `headers`.
async fn sent(headers: &[(&str, &str)]) -> Result<Server, Box<dyn Error>> {
    let server = Server::start().await;
    let mut request = reqwest::Client::new()
        .post(format!("{}/v1/query/aql", server.uri()))
        .body("{}");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.send().await?;
    Ok(server)
}

#[tokio::test]
async fn a_minted_request_id_holding_a_synthetic_id_by_chance_is_not_a_match() -> TestResult {
    let server = sent(&[("x-request-id", MINTED_BY_CHANCE)]).await?;
    let captured = wire(&server).await?;
    assert!(
        !captured.contains(SYNTHETIC),
        "a random UUID is no identifier: {captured}"
    );
    assert!(
        captured.contains(&format!("x-request-id{MINTED_REQUEST_ID}")),
        "the request was recorded, its minted id as the placeholder: {captured}"
    );
    Ok(())
}

#[tokio::test]
async fn a_synthetic_id_in_any_other_header_value_is_still_seen() -> TestResult {
    let carriers: [&[(&str, &str)]; 9] = [
        &[("x-request-id", "req-12345")],
        &[("x-request-id", SYNTHETIC)],
        &[("x-request-id", "0A12345B-7C3D-4E5F-9A1B-2C3D4E5F6A7B")],
        &[("x-request-id", "0a12345b-7c3d-1e5f-9a1b-2c3d4e5f6a7b")],
        &[("x-request-id", "0a12345b7c3d4e5f9a1b2c3d4e5f6a7b")],
        &[(
            "x-request-id",
            "urn:uuid:0a12345b-7c3d-4e5f-9a1b-2c3d4e5f6a7b",
        )],
        &[
            ("x-request-id", "4f6b8f8e-0d7c-4b1a-9e2f-3c5d7e9fa1b3"),
            ("x-request-id", "req-12345"),
        ],
        &[("x-patient", SYNTHETIC), ("authorization", "Bearer 12345")],
        &[
            ("x-correlation-id", MINTED_BY_CHANCE),
            ("x-request-ids", MINTED_BY_CHANCE),
        ],
    ];
    for headers in carriers {
        let server = sent(headers).await?;
        let captured = wire(&server).await?;
        assert!(
            captured.contains(SYNTHETIC),
            "{headers:?}: the scan sees the synthetic id: {captured}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_header_but_a_minted_request_id_is_recorded_raw() -> TestResult {
    let headers = [
        ("x-patient", SYNTHETIC),
        ("x-correlation-id", MINTED_BY_CHANCE),
    ];
    for (name, value) in headers {
        let server = sent(&[(name, value)]).await?;
        let captured = wire(&server).await?;
        assert!(
            captured.contains(&format!("{name}{value}")),
            "{name} is recorded with its raw value: {captured}"
        );
        assert!(!captured.contains(MINTED_REQUEST_ID), "{captured}");
    }
    Ok(())
}
