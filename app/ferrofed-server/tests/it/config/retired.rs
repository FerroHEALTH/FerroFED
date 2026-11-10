// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A key 0.0.19 retired is refused at load naming its replacement, every one
//! the sources set at once (the family's one-release rule of 2026-11-09,
//! `ferrohealth_config::removed`; #1800, #1805, #1806). No specification
//! governs this: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error as StdError;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;

/// A configuration that sets one retired key of each kind: a unit in the
/// key name, a table moved under `endpoint`, and a node key moved into its
/// nested credentials table.
const RETIRED: &str = "[federation]\nper_node_timeout_ms = 1000\n\n\
     [emergency]\nwindow_s = 1\n\n\
     [audit.repository]\nhostname = \"gateway.example.org\"\nurl = \"https://arr.example.org/fhir\"\ntimeout_ms = 1000\nretry_max_ms = 60000\n\n\
     [eehrxf.terminology]\ntimeout_ms = 2000\n\n\
     [credentials.\"node-a\"]\nuser = \"gateway\"\npassword = \"synthetic\"\n";

/// Each retired key `RETIRED` sets and the key its refusal names.
const NAMED: [(&str, &str); 8] = [
    (
        "federation.per_node_timeout_ms",
        "federation.per_node_timeout",
    ),
    ("emergency.window_s", "emergency.window"),
    ("audit.repository.url", "audit.repository.endpoint.url"),
    (
        "audit.repository.timeout_ms",
        "audit.repository.endpoint.timeout",
    ),
    (
        "audit.repository.retry_max_ms",
        "audit.repository.endpoint.retry.max_backoff",
    ),
    (
        "eehrxf.terminology.timeout_ms",
        "eehrxf.terminology.endpoint.timeout",
    ),
    (
        "credentials.node-a.user",
        "credentials.\"<endpoint id>\".credentials.user",
    ),
    (
        "credentials.node-a.password",
        "credentials.\"<endpoint id>\".credentials.password",
    ),
];

#[test]
fn every_retired_key_is_refused_naming_its_replacement() -> Result<(), Box<dyn StdError>> {
    let refused = Config::from_sources(Some(RETIRED), &BTreeMap::new())
        .err()
        .ok_or("the retired keys are refused at load")?;
    let Error::Retired(retired) = &refused else {
        return Err(format!("a retired-key refusal: {refused:?}").into());
    };
    let named: Vec<(&str, &str)> = retired
        .removals()
        .iter()
        .map(|removal| (removal.key(), removal.replacement().unwrap_or_default()))
        .collect();
    for (key, replacement) in NAMED {
        assert!(
            named
                .iter()
                .any(|(set, named)| *set == key && named.contains(replacement)),
            "{key} names {replacement}: {named:?}"
        );
    }
    assert_eq!(
        NAMED.len(),
        named.len(),
        "each retired key set is refused once: {named:?}"
    );
    let shown = refused.to_string();
    assert!(!shown.contains("synthetic"), "no value is shown: {shown}");
    Ok(())
}
