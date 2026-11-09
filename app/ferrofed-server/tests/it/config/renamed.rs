// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A key a release renamed is refused at load naming its replacement, in the
//! file and as a `FERROFED__` override, so the operator learns the step from
//! the refusal itself. Before 1.0 a key may be renamed in any release, with no
//! deprecation release first (the owner's decision on #1643). No
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
fn names_both(error: &Error, (old, new): (&str, &str)) -> Result<(), Box<dyn StdError>> {
    match error {
        Error::Renamed {
            old: refused,
            new: replacement,
        } if *refused == old && *replacement == new => {}
        other => return Err(format!("{old} is refused naming {new}: {other:?}").into()),
    }
    let shown = error.to_string();
    if !(shown.contains(old) && shown.contains(new)) {
        return Err(format!("the message names both keys: {shown}").into());
    }
    Ok(())
}

#[test]
fn a_key_renamed_in_the_file_is_refused_naming_its_replacement() -> Result<(), Box<dyn StdError>> {
    for (old, new, value, _) in RENAMED {
        let (table, key) = old.split_once('.').ok_or("a dotted key")?;
        names_both(
            &refusal(&format!("[{table}]\n{key} = {value}\n"))?,
            (old, new),
        )?;
    }
    Ok(())
}

#[test]
fn a_key_renamed_as_an_override_is_refused_naming_its_replacement() -> Result<(), Box<dyn StdError>>
{
    for (old, new, value, variable) in RENAMED {
        let environment =
            BTreeMap::from([(variable.to_owned(), value.trim_matches('"').to_owned())]);
        let error = Config::from_sources(None, &environment)
            .err()
            .ok_or_else(|| format!("{variable} is refused"))?;
        names_both(&error, (old, new))?;
    }
    Ok(())
}

#[test]
fn a_key_no_release_renamed_is_refused_as_an_unknown_key() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[server]\nlisten_port = 8080\n")?;
    if matches!(error, Error::Renamed { .. }) || !matches!(error, Error::Load { .. }) {
        return Err(format!("an unknown key stays unknown: {error:?}").into());
    }
    Ok(())
}
