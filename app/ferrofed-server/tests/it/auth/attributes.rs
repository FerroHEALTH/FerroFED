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
use ferrofed_server::config::auth::Verification;
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

/// The `professional_issuing_authority` 0.0.20 retired refuses the load,
/// naming the `[[auth.attributes]]` source that replaces it (the family's
/// one-release rule, `ferrohealth_config::removed`).
#[test]
fn the_retired_issuing_authority_key_is_refused_naming_its_replacement() -> TestResult {
    let text = "[[trust.issuer]]\nissuer = \"https://issuer.example.test\"\nprofessional_issuing_authority = \"hp_issuing_authority\"\n";
    let refused = Config::from_sources(Some(text), &BTreeMap::new())
        .err()
        .ok_or("the retired key is refused at load")?;
    let ConfigError::Retired(retired) = &refused else {
        return Err(format!("expected a retired-key refusal, got {refused:?}").into());
    };
    let named: Vec<(&str, &str)> = retired
        .removals()
        .iter()
        .map(|removal| (removal.key(), removal.replacement().unwrap_or_default()))
        .collect();
    assert_eq!(1, named.len(), "{named:?}");
    assert!(
        named.iter().any(|(key, replacement)| {
            *key == "trust.issuer[0].professional_issuing_authority"
                && replacement.contains("professional.issuing_authority_name")
        }),
        "{named:?}"
    );
    Ok(())
}
