// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access record of a request a national contact point relays
//! (Regulation (EU) 2025/327 Annex II 3.2(a), (b); Implementing Regulation
//! (EU) 2026/2099 Art 7, Annex Tables 1 and 2): the professional and the
//! provider of another Member State with their country and issuing
//! authorities, marked as asserted by the contact point it names, and the
//! correlation identifier the connector sent.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::atna_feed::FeedRepository;
use http::StatusCode;
use serde_json::Value;

use super::{
    LAB_REPORT, TestResult, accesses, composition, details, gateway_under, node_with_rows,
};
use crate::auth::{bearing, minted, query};
use crate::contact_point::{
    CORRELATION, CORRELATION_HEADER, COUNTRY, FAMILY, GIVEN, HCP_ADDRESS, HCP_AUTHORITY, HCP_ID,
    HCP_NAME, HP_AUTHORITY, HP_ID, ROLE, ROLE_SYSTEM, declared, relaying,
};
use crate::facade::settings_with_room;
use crate::feed_audit::SETTLE;
use crate::support::{ISSUER, call};

const UID_A: &str = "6a7b8c9d-1e2f-4a3b-8c4d-5e6f7a8b9c0d::cdr-a.example.org::1";

/// Sends the suite's patient query as the test contact point's caller,
/// with `correlation` in its correlation header when given, and returns the
/// one access record.
async fn recorded(correlation: Option<&str>) -> Result<Value, Box<dyn Error>> {
    let node_a = node_with_rows(&[composition(LAB_REPORT, UID_A)]).await;
    let node_b = node_with_rows(&[]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let mut server = settings_with_room();
    server.auth = declared()?;
    let app = gateway_under(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        "",
        &server,
    )?;
    let mut request = bearing(query()?, &minted(&relaying())?)?;
    if let Some(correlation) = correlation {
        request
            .headers_mut()
            .insert(CORRELATION_HEADER, correlation.parse()?);
    }
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the access, got {records:?}").into());
    };
    Ok(record.clone())
}

/// Annex II 3.2(a), (b), 2026/2099 Annex Tables 1 and 2: the record names
/// every attribute the contact point relays, marked as its assertion.
#[tokio::test]
async fn the_record_names_the_relayed_professional_and_provider_as_asserted() -> TestResult {
    let record = recorded(Some(CORRELATION)).await?;
    let relayed = |kind: &str| details(&record, "ehds-relayed", kind);
    for (kind, value) in [
        ("contact-point", ISSUER),
        ("asserted", "true"),
        ("country-code", COUNTRY),
        ("hp-family-name", FAMILY),
        ("hp-given-name", GIVEN),
        ("hp-identifier", HP_ID),
        ("hp-issuing-authority", HP_AUTHORITY),
        ("provider-identifier", HCP_ID),
        ("provider-issuing-authority", HCP_AUTHORITY),
        ("provider-name", HCP_NAME),
        ("provider-address", HCP_ADDRESS),
    ] {
        assert_eq!(relayed(kind), [value], "{kind}");
    }
    assert_eq!(
        relayed("hp-professional-role"),
        [format!("{ROLE_SYSTEM}|{ROLE}")]
    );
    Ok(())
}

/// The correlation identifier the connector sent is recorded with the
/// request id, so the record joins the contact point's own log.
#[tokio::test]
async fn the_record_carries_the_correlation_identifier() -> TestResult {
    let record = recorded(Some(CORRELATION)).await?;
    let transaction = record["entity"]
        .as_array()
        .and_then(|entities| {
            entities
                .iter()
                .find(|entity| entity["type"]["code"] == "XrequestId")
        })
        .ok_or("entity:transaction")?;
    let correlations: Vec<&str> = transaction["detail"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|detail| detail["type"] == "correlation-id")
        .filter_map(|detail| detail["valueString"].as_str())
        .collect();
    assert_eq!(correlations, [CORRELATION]);
    Ok(())
}

/// A request that sends no correlation identifier is recorded with none.
#[tokio::test]
async fn a_request_without_a_correlation_identifier_records_none() -> TestResult {
    let record = recorded(None).await?;
    assert!(!record.to_string().contains("correlation-id"), "{record}");
    assert_eq!(details(&record, "ehds-relayed", "asserted"), ["true"]);
    Ok(())
}
