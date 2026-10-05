// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a synthetic registry, a configuration shaped like a
//! server's, and a poll for futures that complete without waiting.
#![allow(clippy::panic, reason = "the helpers fail the test they serve")]

use std::pin::pin;
use std::task::{Context, Poll, Waker};

#[cfg(feature = "ihe")]
use ferrofed_identity::behalf::{Caller, OnBehalfOf, Purpose};
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

/// The synthetic caller's `sub`, unlike any other text.
#[cfg(feature = "ihe")]
pub(crate) const CALLER_SUBJECT: &str = "Qz7-caller-41";

/// The synthetic caller's `client_id`, unlike any other text.
#[cfg(feature = "ihe")]
pub(crate) const CALLER_CLIENT: &str = "Qz7-app-42";

/// The issuer of the synthetic caller's token.
#[cfg(feature = "ihe")]
pub(crate) const CALLER_ISSUER: &str = "https://issuer.example.test";

/// The audience the synthetic caller's token names the gateway by.
#[cfg(feature = "ihe")]
pub(crate) const CALLER_AUDIENCE: &str = "urn:example:gateway-under-test";

/// A synthetic caller the gateway verified, asking for the purpose of use
/// `TREAT`.
#[cfg(feature = "ihe")]
pub(crate) fn caller() -> OnBehalfOf {
    OnBehalfOf::Caller(
        Caller::new(
            CALLER_ISSUER.to_owned(),
            CALLER_SUBJECT.to_owned(),
            CALLER_CLIENT.to_owned(),
        )
        .with_audience(Some(CALLER_AUDIENCE.to_owned()))
        .with_purposes(vec![Purpose {
            system: Some("http://terminology.hl7.org/CodeSystem/v3-ActReason".to_owned()),
            code: "TREAT".to_owned(),
        }]),
    )
}

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
