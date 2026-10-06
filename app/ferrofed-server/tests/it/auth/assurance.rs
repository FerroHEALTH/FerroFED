// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth.issuer.assurance]` and `client_tokens_act_for_professional` as
//! written: every value the gateway cannot read a level with refuses to
//! boot, naming its key, and `config check` notes each issuer that requires
//! no level (Regulation (EU) 2025/327 Annex II 3.1; the levels of Regulation
//! (EU) No 910/2014 Art 8(2)).
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;

use ferrofed_engine::conveyance::AssuranceLevel;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::AuthFault;
use ferrofed_server::config::error::Error as ConfigError;

use crate::run::binary;

/// One trusted issuer by its key set URL.
fn issuer() -> String {
    String::from(
        "[auth]\naudience = \"urn:example:gateway\"\n\n[[auth.issuer]]\nissuer = \"https://issuer.example.test\"\njwks_uri = \"https://issuer.example.test/jwks\"\n",
    )
}

/// [`issuer`] with an `[auth.issuer.assurance]` table of `body`.
fn assuring(body: &str) -> String {
    format!("{}\n[auth.issuer.assurance]\n{body}\n", issuer())
}

/// The refusal of `text`, or `None` when it resolves.
fn refusal(text: &str) -> Result<Option<ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err())
}

/// Asserts that `text` is refused for `fault` at `key`.
fn refused_for(text: &str, key: &str, fault: AuthFault) -> Result<(), Box<dyn Error>> {
    match refusal(text)? {
        Some(ConfigError::Auth {
            key: named,
            fault: found,
        }) => {
            assert_eq!((key, fault), (named.as_str(), found), "{text}");
            Ok(())
        }
        other => Err(format!("{text}: expected {fault:?} at {key}, got {other:?}").into()),
    }
}

/// An issuer's assurance mapping resolves each value to its level and reads
/// `acr` (RFC 9068 §2.2.1) unless it names another claim; an issuer without
/// one requires no level, and declares its client tokens as acting for no
/// professional.
#[test]
fn an_assurance_table_resolves_each_value_to_its_level() -> Result<(), Box<dyn Error>> {
    let text = assuring(
        "minimum = \"substantial\"\nlow = [\"urn:example:loa:1\"]\nsubstantial = [\"urn:example:loa:2\"]\nhigh = [\"urn:example:loa:3\"]",
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let [trusted] = settings.server.auth.issuers.as_slice() else {
        return Err("one issuer".into());
    };
    let assurance = trusted.assurance.as_ref().ok_or("an assurance mapping")?;
    assert_eq!("acr", assurance.claim);
    assert_eq!(AssuranceLevel::Substantial, assurance.minimum);
    assert_eq!(
        (Some(AssuranceLevel::Low), Some(AssuranceLevel::High), None),
        (
            assurance.level_of("urn:example:loa:1"),
            assurance.level_of("urn:example:loa:3"),
            assurance.level_of("urn:example:loa:4")
        )
    );
    assert!(
        !trusted.client_tokens_act_for_professional,
        "off by default"
    );
    let plain = Config::from_sources(Some(&issuer()), &BTreeMap::new())?.resolve()?;
    assert!(
        plain
            .server
            .auth
            .issuers
            .iter()
            .all(|issuer| issuer.assurance.is_none()),
        "no level is required by default"
    );
    Ok(())
}

/// An issuer declares its client tokens as acting for a professional with
/// one key.
#[test]
fn an_issuer_declares_its_client_tokens_with_one_key() -> Result<(), Box<dyn Error>> {
    let text = format!("{}client_tokens_act_for_professional = true\n", issuer());
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    assert!(
        settings
            .server
            .auth
            .issuers
            .iter()
            .all(|issuer| issuer.client_tokens_act_for_professional)
    );
    Ok(())
}

/// An assurance table names its least level and a claim; either missing
/// refuses to boot, naming its key.
#[test]
fn an_assurance_table_without_a_minimum_or_a_claim_is_refused() -> Result<(), Box<dyn Error>> {
    for (body, key) in [
        (
            "high = [\"urn:example:loa:3\"]",
            "auth.issuer[0].assurance.minimum",
        ),
        (
            "claim = \"\"\nminimum = \"high\"\nhigh = [\"urn:example:loa:3\"]",
            "auth.issuer[0].assurance.claim",
        ),
    ] {
        match refusal(&assuring(body))? {
            Some(ConfigError::Missing { key: named }) => assert_eq!(key, named, "{body}"),
            other => return Err(format!("{body}: expected {key} missing, got {other:?}").into()),
        }
    }
    Ok(())
}

/// A value listed at two levels, or an empty one, would make a level
/// ambiguous, and is refused.
#[test]
fn an_assurance_value_at_two_levels_or_empty_is_refused() -> Result<(), Box<dyn Error>> {
    refused_for(
        &assuring(
            "minimum = \"low\"\nlow = [\"urn:example:loa:2\"]\nhigh = [\"urn:example:loa:2\"]",
        ),
        "auth.issuer[0].assurance.high",
        AuthFault::AssuranceValue,
    )?;
    refused_for(
        &assuring("minimum = \"low\"\nlow = [\"\"]"),
        "auth.issuer[0].assurance.low",
        AuthFault::AssuranceValue,
    )
}

/// A least level no declared value reaches would refuse every patient-data
/// request, and is refused at boot.
#[test]
fn a_least_level_no_value_reaches_is_refused() -> Result<(), Box<dyn Error>> {
    refused_for(
        &assuring(
            "minimum = \"high\"\nlow = [\"urn:example:loa:1\"]\nsubstantial = [\"urn:example:loa:2\"]",
        ),
        "auth.issuer[0].assurance.minimum",
        AuthFault::AssuranceUnreachable,
    )
}

/// A level outside the three of Regulation (EU) No 910/2014 Art 8(2) does
/// not load.
#[test]
fn a_level_outside_the_three_does_not_load() {
    let text = assuring("minimum = \"medium\"\nhigh = [\"urn:example:loa:3\"]");
    assert!(
        Config::from_sources(Some(&text), &BTreeMap::new())
            .and_then(|config| config.resolve())
            .is_err(),
        "medium is no level"
    );
}

/// `config check` names each issuer that declares no assurance mapping, and
/// none that declares one.
#[test]
fn config_check_notes_an_issuer_without_an_assurance_mapping() -> Result<(), Box<dyn Error>> {
    let plain = binary(&["config", "check"], &issuer())?;
    assert_eq!(
        Some(0),
        plain.status.code(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    let stdout = String::from_utf8_lossy(&plain.stdout);
    assert!(
        stdout.contains(
            "ferrofed: note: auth.issuer[0] (https://issuer.example.test) declares no [auth.issuer.assurance]"
        ),
        "{stdout}"
    );
    let assured = binary(
        &["config", "check"],
        &assuring("minimum = \"substantial\"\nsubstantial = [\"urn:example:loa:2\"]"),
    )?;
    assert_eq!(
        Some(0),
        assured.status.code(),
        "{}",
        String::from_utf8_lossy(&assured.stderr)
    );
    let stdout = String::from_utf8_lossy(&assured.stdout);
    assert!(!stdout.contains("[auth.issuer.assurance]"), "{stdout}");
    Ok(())
}
