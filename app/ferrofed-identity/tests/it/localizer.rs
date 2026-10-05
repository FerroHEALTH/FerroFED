// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The localizer's failure policy: `closed` by default, `ask-all` only when
//! written, and no third value (§14.1, N4).

use std::error::Error;

use ferrofed_identity::role::localizer::OnFailure;
use serde::Deserialize;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug, Deserialize)]
struct Policy {
    on_failure: OnFailure,
}

#[test]
fn closed_is_the_default() {
    assert_eq!(
        OnFailure::Closed,
        OnFailure::default(),
        "§14.1: fail-closed"
    );
}

#[test]
fn both_values_read_and_render_as_the_specification_spells_them() -> TestResult {
    for (written, policy) in [
        ("closed", OnFailure::Closed),
        ("ask-all", OnFailure::AskAll),
    ] {
        let read: Policy = toml::from_str(&format!("on_failure = \"{written}\""))?;
        assert_eq!(policy, read.on_failure);
        assert_eq!(written, policy.as_str(), "OPTIONS declares it as written");
        assert_eq!(written, policy.to_string());
    }
    Ok(())
}

#[test]
fn a_value_the_specification_does_not_name_is_refused() {
    for written in ["open", "Closed", "ask_all", ""] {
        let read: Result<Policy, _> = toml::from_str(&format!("on_failure = \"{written}\""));
        assert!(
            read.is_err(),
            "§14.1 names closed and ask-all only: {written:?}"
        );
    }
}
