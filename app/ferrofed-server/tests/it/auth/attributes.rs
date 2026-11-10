// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a token states about its professional comes from the issuer's
//! `[[auth.attributes]]` table, which `ferrohealth-smart` reads: an issuer
//! with no table states none of it, and the per-issuer
//! `professional_issuing_authority` the gateway read before is refused,
//! naming its replacement.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;

use ferrofed_server::auth::refusal::Refusal;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::{AuthFault, Verification};
use ferrofed_server::config::error::Error as ConfigError;

use super::{Gateway, TestResult, assert_refused, bearing, claims, minted, query};
use crate::support;

/// §13.4: a token whose issuer has no `[[auth.attributes]]` table states no
/// purpose of use, whatever its IUA extension carries, so it reaches no node.
// conformance: CP-17
#[tokio::test]
async fn an_issuer_without_an_attributes_table_states_no_purpose() -> TestResult {
    let mut auth = support::caller::auth();
    for issuer in &mut auth.issuers {
        if let Verification::KeySet(settings) = &mut issuer.verification {
            settings.attributes = None;
        }
    }
    let gateway = Gateway::with(auth).await?;
    let request = bearing(query()?, &minted(&claims())?)?;
    assert_refused(&gateway, request, Refusal::PurposeOfUse).await
}

/// The removed `professional_issuing_authority` refuses to boot, naming the
/// `[[auth.attributes]]` source that replaces it.
#[test]
fn the_removed_issuing_authority_key_is_refused_naming_its_replacement() -> TestResult {
    let text = "[auth]\n\n[[auth.issuer]]\nissuer = \"https://issuer.example.test\"\nkind = \"persons\"\naudience = [\"urn:example:gateway\"]\n\n[[trust.issuer]]\nissuer = \"https://issuer.example.test\"\nprofessional_issuing_authority = \"hp_issuing_authority\"\n";
    let refused = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err()
        .ok_or("the removed key is refused")?;
    let ConfigError::Auth { key, fault } = &refused else {
        return Err(format!("expected an [auth] refusal, got {refused:?}").into());
    };
    assert_eq!(
        (
            "trust.issuer[0].professional_issuing_authority",
            AuthFault::IssuingAuthorityMoved
        ),
        (key.as_str(), *fault)
    );
    assert!(
        refused
            .to_string()
            .contains("\"professional.issuing_authority_name\""),
        "{refused}"
    );
    Ok(())
}
