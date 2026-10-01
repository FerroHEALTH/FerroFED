// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a synthetic registry, a configuration shaped like a
//! server's, and a poll for futures that complete without waiting.
#![allow(clippy::panic, reason = "the helpers fail the test they serve")]

use std::pin::pin;
use std::task::{Context, Poll, Waker};

use ferrofed_identity::dev::{DevTable, Profile};
use ferrofed_registry::snapshot::RegistrySnapshot;
use serde::Deserialize;

/// Two synthetic members, `node-a` and `node-b`.
pub(crate) const REGISTRY: &str = r#"
[[organisation]]
id = "org-a"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[node]]
id = "node-b"
organisation = "org-a"
system_id = "cdr-b.example.org"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "https://cdr-a.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-b-pub"
node = "node-b"
url = "https://cdr-b.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"
"#;

/// The synthetic patient identifier of the fixtures.
pub(crate) const PATIENT_VALUE: &str = "12345";

/// The registry of [`REGISTRY`].
pub(crate) fn registry() -> RegistrySnapshot {
    match RegistrySnapshot::from_toml_str(REGISTRY) {
        Ok(registry) => registry,
        Err(error) => panic!("the fixture registry loads: {error}"),
    }
}

/// The part of a server configuration the cross-reference reads.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) profile: Profile,
    pub(crate) dev: Option<DevTable>,
}

/// A configuration with `profile` and one cross-reference row per
/// `(member, ehr_id)` for [`PATIENT_VALUE`] in namespace `2.999.1`.
pub(crate) fn config(profile: &str, rows: &[(&str, &str)]) -> String {
    let mut text = format!("profile = \"{profile}\"\n");
    for (member, ehr_id) in rows {
        let row = format!(
            "\n[[dev.crossref]]\nnamespace = \"2.999.1\"\nvalue = \"{PATIENT_VALUE}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        );
        text.push_str(&row);
    }
    text
}

/// Polls a future once and returns its output; the static resolver's futures
/// never wait.
pub(crate) fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the future was expected to complete without waiting"),
    }
}
