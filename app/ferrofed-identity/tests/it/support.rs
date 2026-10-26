// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a synthetic registry and a synthetic caller.
#![allow(clippy::panic, reason = "the helpers fail the test they serve")]

#[cfg(feature = "ihe")]
use ferrofed_identity::role::behalf::{Caller, OnBehalfOf, Purpose};
use ferrofed_registry::snapshot::RegistrySnapshot;

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
