// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `config.confirm_ms`, `config.approval` and `config.secrets_dir`: each
//! read within its bounds under `config.ui = "managed"`, and a value outside
//! them refused at load. No specification governs this: our own design.

use std::collections::BTreeMap;

use ferrofed_server::config::Config;

use super::TestResult;

/// The `[config]` of a managed gateway, to which a test appends a key.
const MANAGED: &str = "[metrics]\nlisten = \"127.0.0.1:9\"\n\n[config]\nui = \"managed\"\npath = \"revisions.redb\"\n";

#[test]
fn a_window_a_rule_or_a_directory_out_of_bounds_is_refused() -> TestResult {
    for (key, expected) in [
        ("confirm_ms = 999", "confirm_ms"),
        ("confirm_ms = 3600001", "confirm_ms"),
        ("approval = [\"auth\"]", "auth"),
        ("secrets_dir = \"relative\"", "secrets_dir"),
        (
            "secrets_dir = \"/nonexistent/ferrofed/secrets\"",
            "secrets_dir",
        ),
    ] {
        let config = Config::from_sources(Some(&format!("{MANAGED}{key}\n")), &BTreeMap::new())?;
        let Err(error) = config.resolve() else {
            return Err(format!("{key} is refused").into());
        };
        assert!(error.to_string().contains(expected), "{key}: {error}");
    }
    Ok(())
}

#[test]
fn the_window_the_rules_and_the_directory_are_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = format!(
        "{MANAGED}confirm_ms = 1000\napproval = [\"onward-credentials\", \"credentials.node-a-pub\", \"federation.consent\"]\nsecrets_dir = {}\n",
        toml::Value::String(dir.path().display().to_string())
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        std::time::Duration::from_secs(1),
        settings.revision_policy.confirm
    );
    assert_eq!(3, settings.revision_policy.approval.len());
    assert_eq!(
        Some(dir.path()),
        settings.revision_policy.secrets_dir.as_deref()
    );
    let defaults = Config::from_sources(Some(MANAGED), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        std::time::Duration::from_secs(120),
        defaults.revision_policy.confirm
    );
    assert!(defaults.revision_policy.secrets_dir.is_none());
    Ok(())
}
