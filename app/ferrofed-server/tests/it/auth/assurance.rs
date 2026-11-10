// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An issuer's assurance table, `[auth.issuer.assurance]`, which
//! `ferrohealth-smart` reads, and the per-issuer keys it replaces: the
//! removed `[trust.issuer.assurance]`, `[trust.issuer.requester]` and
//! `client_tokens_act_for_professional` each refuse to boot, naming their
//! replacement, and `config check` notes each issuer that requires no level
//! (Regulation (EU) 2025/327 Annex II 3.1; the levels of Regulation (EU) No
//! 910/2014 Art 8(2)).
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrohealth_smart::assurance::AssuranceLevel;

use crate::run::binary;

/// One trusted issuer and its `[[trust.issuer]]` entry, last, so a key
/// appended to the text is the entry's.
fn issuer() -> String {
    String::from(
        "[auth]\n\n[[auth.issuer]]\nissuer = \"https://issuer.example.test\"\nkind = \"persons\"\naudience = [\"urn:example:gateway\"]\n\n[[trust.issuer]]\nissuer = \"https://issuer.example.test\"\n",
    )
}

/// One trusted issuer with an `[auth.issuer.assurance]` table of `body`.
fn assuring(body: &str) -> String {
    format!(
        "[auth]\n\n[[auth.issuer]]\nissuer = \"https://issuer.example.test\"\nkind = \"persons\"\naudience = [\"urn:example:gateway\"]\n\n[auth.issuer.assurance]\n{body}\n"
    )
}

/// The refusal of `text`, or `None` when it resolves.
fn refusal(text: &str) -> Result<Option<ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err())
}

/// An issuer's assurance table resolves to the crate's: its claim, the
/// level of each value and the least level; an issuer without one requires
/// no level.
#[test]
fn an_assurance_table_resolves_to_the_issuers_verification() -> Result<(), Box<dyn Error>> {
    let text = assuring(
        "minimum = \"substantial\"\n[auth.issuer.assurance.levels]\n\"urn:example:loa:1\" = \"low\"\n\"urn:example:loa:3\" = \"substantial\"\n",
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let trusted = settings.server.auth.issuers.first().ok_or("one issuer")?;
    let assurance = trusted
        .verification
        .assurance()
        .ok_or("an assurance table")?;
    assert_eq!("acr", assurance.claim);
    assert_eq!(Some(AssuranceLevel::Substantial), assurance.minimum);
    assert_eq!(
        Some(&AssuranceLevel::Low),
        assurance.levels.get("urn:example:loa:1")
    );
    let plain = Config::from_sources(Some(&issuer()), &BTreeMap::new())?.resolve()?;
    assert!(
        plain
            .server
            .auth
            .issuers
            .iter()
            .all(|issuer| issuer.verification.assurance().is_none())
    );
    Ok(())
}

/// A least level no value reaches is the crate's refusal of `[auth]`.
#[test]
fn a_least_level_no_value_reaches_is_refused() -> Result<(), Box<dyn Error>> {
    let refused = refusal(&assuring("minimum = \"high\""))?;
    assert!(
        matches!(&refused, Some(ConfigError::Smart { section, .. }) if section == "auth"),
        "{refused:?}"
    );
    Ok(())
}

/// Each per-issuer key a release before 0.0.18 removed is an unknown key
/// now, refused by the load naming it (#1787).
#[test]
fn each_removed_per_issuer_key_is_an_unknown_key() -> Result<(), Box<dyn Error>> {
    for (body, key) in [
        (
            "\n[trust.issuer.assurance]\nminimum = \"substantial\"\nsubstantial = [\"urn:example:loa:2\"]\n",
            "trust.issuer[0].assurance",
        ),
        (
            "\n[trust.issuer.requester]\nprofessional = \"uzi\"\nrole = \"role\"\norganisation = \"ura\"\norganisation_type = \"type\"\n",
            "trust.issuer[0].requester",
        ),
        (
            "client_tokens_act_for_professional = true\n",
            "trust.issuer[0].client_tokens_act_for_professional",
        ),
    ] {
        let text = format!("{}{body}", issuer());
        let refused = Config::from_sources(Some(&text), &BTreeMap::new())
            .err()
            .ok_or_else(|| format!("{key} is refused"))?;
        assert_eq!(
            Some(key),
            crate::support::refused_key(&refused),
            "{refused:?}"
        );
    }
    Ok(())
}

/// `config check` names each issuer that declares no assurance table, and
/// none that declares one.
#[test]
fn config_check_notes_an_issuer_without_an_assurance_table() -> Result<(), Box<dyn Error>> {
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
            "ferrofed: note: the issuer https://issuer.example.test declares no [auth.issuer.assurance]"
        ),
        "{stdout}"
    );
    let assured = binary(
        &["config", "check"],
        &assuring(
            "minimum = \"substantial\"\n[auth.issuer.assurance.levels]\n\"urn:example:loa:2\" = \"substantial\"\n",
        ),
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
