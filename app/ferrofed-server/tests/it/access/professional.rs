// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access log behind client authentication: a request refused because
//! its token names no natural person, or states no authentication assurance
//! at the issuer's least level (Regulation (EU) 2025/327 Annex II 3.1),
//! reaches no node and writes no access record, and a request the gate
//! admits is recorded (Annex II 3.2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;

use ferrofed_engine::conveyance::AssuranceLevel;
use ferrofed_server::config::auth::assurance::Assurance;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::issuer::Claims;
use http::StatusCode;

use super::{LAB_REPORT, TestResult, accesses, composition, gateway_under, node_with_rows};
use crate::auth::{bearing, minted, query};
use crate::facade::settings_with_room;
use crate::feed_audit::SETTLE;
use crate::support::{self, call, error_body};

const UID_A: &str = "4c1d8e2f-7a3b-4e5c-9d6f-1a2b3c4d5e6f::cdr-a.example.org::1";

/// The `acr` value the test issuer states for level low.
const LOW: &str = "urn:example:loa:low";

/// The `acr` value the test issuer states for level substantial.
const SUBSTANTIAL: &str = "urn:example:loa:substantial";

/// The suite's default claims stating `acr`.
fn stating(acr: &str) -> Claims {
    let mut stated = support::claims();
    stated.other.insert(String::from("acr"), acr.to_owned());
    stated
}

#[tokio::test]
async fn a_request_refused_for_its_person_or_assurance_writes_no_access_record() -> TestResult {
    let node_a = node_with_rows(&[composition(LAB_REPORT, UID_A)]).await;
    let node_b = node_with_rows(&[]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let mut server = settings_with_room();
    server.auth = support::auth();
    for issuer in &mut server.auth.issuers {
        issuer.assurance = Some(Assurance {
            claim: String::from("acr"),
            minimum: AssuranceLevel::Substantial,
            values: BTreeMap::from([
                (LOW.to_owned(), AssuranceLevel::Low),
                (SUBSTANTIAL.to_owned(), AssuranceLevel::Substantial),
            ]),
        });
    }
    let app = gateway_under(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        "",
        &server,
    )?;

    let admitted = bearing(query()?, &minted(&stating(SUBSTANTIAL))?)?;
    let (status, text) = call(app.clone(), admitted).await?;
    assert_eq!(StatusCode::OK, status, "admitted past the gate: {text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    assert_eq!(1, records.len(), "the admitted access is recorded");
    let reached = node_a.received_requests().await.unwrap_or_default().len();

    let mut client = stating(SUBSTANTIAL);
    client.sub.clone_from(&client.client_id);
    let low = stating(LOW);
    for (token, code) in [
        (minted(&client)?, "natural-person-required"),
        (minted(&low)?, "authentication-assurance-insufficient"),
    ] {
        let (status, text) = call(app.clone(), bearing(query()?, &token)?).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, status, "{code}: {text}");
        assert_eq!(code, error_body(&text)?.code, "{text}");
    }
    assert_eq!(
        reached,
        node_a.received_requests().await.unwrap_or_default().len(),
        "a refused request reaches no node"
    );
    // NOTE: Regulation (EU) 2025/327 Annex II 3.2: a record is stored before its answer
    // leaves, so the repository holds every record of the refused requests by now.
    let records = accesses(&repository.records())?;
    assert_eq!(1, records.len(), "a refused request writes no record");
    Ok(())
}
