// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A key a release renamed is refused naming its replacement for that one
//! release, and is an unknown key from the next one on, in the file and as a
//! `FERROFED__` override (the owner's rule of 2026-11-09, #1787). No
//! specification governs this: our own design.

use std::collections::BTreeMap;
use std::error::Error as StdError;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;

use super::refusal;

/// Each key v0.0.16 renamed (#1490), its replacement, a value it takes, and
/// the override that sets it.
const RENAMED: [(&str, &str, &str, &str); 4] = [
    (
        "server.public_url",
        "gateway.public_url",
        "\"https://gw.example.org\"",
        "FERROFED__SERVER__PUBLIC_URL",
    ),
    (
        "server.replica",
        "gateway.replica",
        "\"replica-1\"",
        "FERROFED__SERVER__REPLICA",
    ),
    (
        "server.bindings_drain_timeout",
        "gateway.bindings_drain_timeout",
        "5000",
        "FERROFED__SERVER__BINDINGS_DRAIN_TIMEOUT",
    ),
    (
        "server.overload_retry_after_s",
        "server.retry_after",
        "5",
        "FERROFED__SERVER__OVERLOAD_RETRY_AFTER_S",
    ),
];

/// Holds `error` to the refusal of `old`, renamed to `new`.
/// Asserts `error` refuses `old` as an unknown key, naming no replacement.
fn unknown(error: &Error, (old, new): (&str, &str)) -> Result<(), Box<dyn StdError>> {
    if crate::support::refused_key(error) != Some(old) {
        return Err(format!("{old} is an unknown key: {error:?}").into());
    }
    if error.to_string().contains(new) {
        return Err(format!("the refusal names no replacement: {error}").into());
    }
    Ok(())
}

#[test]
fn a_key_renamed_before_0_0_18_is_unknown_in_the_file() -> Result<(), Box<dyn StdError>> {
    for (old, new, value, _) in RENAMED {
        let (table, key) = old.split_once('.').ok_or("a dotted key")?;
        unknown(
            &refusal(&format!("[{table}]\n{key} = {value}\n"))?,
            (old, new),
        )?;
    }
    Ok(())
}

#[test]
fn a_key_renamed_before_0_0_18_is_unknown_as_an_override() -> Result<(), Box<dyn StdError>> {
    for (old, new, value, variable) in RENAMED {
        let environment =
            BTreeMap::from([(variable.to_owned(), value.trim_matches('"').to_owned())]);
        let error = Config::from_sources(None, &environment)
            .err()
            .ok_or_else(|| format!("{variable} is refused"))?;
        unknown(&error, (old, new))?;
    }
    Ok(())
}

#[test]
fn a_key_no_release_renamed_is_refused_as_an_unknown_key() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[server]\nlisten_port = 8080\n")?;
    if !matches!(error, Error::Load { .. }) {
        return Err(format!("an unknown key stays unknown: {error:?}").into());
    }
    Ok(())
}
