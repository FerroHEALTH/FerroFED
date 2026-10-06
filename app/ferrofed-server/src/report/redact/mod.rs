// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The effective configuration with every key and value that is not known
//! to be safe replaced, for the report bundle.
//!
//! The redactor fails closed. It runs over the parsed tree, before anything
//! is serialized, so a comment or an inline table cannot pass it, and it
//! judges every key by its exact spelling, so a key in another casing or
//! with another separator is unknown. A key is shown only when it is one of
//! [`KNOWN_KEYS`] outside a table keyed by data ([`DATA_KEYED`]); any other
//! key becomes a numbered [`REDACTED`] placeholder. A value is kept only
//! under a known key that is no credential and no file path:
//!
//! - a boolean;
//! - a number under one of [`NUMBERS`], the timeouts, limits and sizes;
//! - the scheme, host and port of a URL under one of [`URLS`], its userinfo,
//!   path, query and fragment replaced;
//! - a string under one of [`KEPT`] that has the shape that key takes: a
//!   socket address, a path of plain segments, or a short token.
//!
//! Everything else is [`REDACTED`], and so is every value under `[dev]`,
//! whose cross-reference pairs patient identifiers with `ehr_id`s (§5.4.1,
//! N33). A key added to the configuration later is therefore redacted until
//! it joins these lists. No specification governs the report: our own
//! design.

use std::net::SocketAddr;

use ferrofed_registry::secret::REDACTED;

use crate::report::redact::keys::{DATA_KEYED, KEPT, KNOWN_KEYS, NUMBERS, URLS};

pub mod keys;

/// The words of a key that mark its value as a credential, in any casing
/// and between any separators.
const CREDENTIAL_WORDS: &[&str] = &[
    "assertion",
    "authorization",
    "credential",
    "credentials",
    "identity",
    "key",
    "passphrase",
    "password",
    "pin",
    "pwd",
    "secret",
    "token",
];

/// The section whose every value is replaced: the development
/// cross-reference, which pairs patient identifiers with `ehr_id`s.
const DEVELOPMENT: &str = "dev";

/// The longest token a [`KEPT`] key other than `listen` and `base_path`
/// keeps.
const TOKEN_LENGTH: usize = 40;

/// Returns `tree` with every key and value [this module](self) does not
/// know to be safe replaced by [`REDACTED`].
#[must_use]
pub fn redact(tree: &toml::Table) -> toml::Table {
    redact_table(tree, false, false)
}

/// Returns `table` redacted; `data_keyed` when its keys are data, and
/// `development` when it sits under `[dev]`.
fn redact_table(table: &toml::Table, data_keyed: bool, development: bool) -> toml::Table {
    table
        .iter()
        .enumerate()
        .map(|(position, (key, value))| {
            let shown = if data_keyed || !KNOWN_KEYS.contains(&key.as_str()) {
                format!("{REDACTED}{}", position.saturating_add(1))
            } else {
                key.clone()
            };
            let development = development || words(key) == [DEVELOPMENT];
            let value = if data_keyed {
                redact_value(REDACTED, value, development)
            } else {
                redact_value(key, value, development)
            };
            (shown, value)
        })
        .collect()
}

/// Returns `value`, held under `key`, redacted.
fn redact_value(key: &str, value: &toml::Value, development: bool) -> toml::Value {
    match value {
        toml::Value::Table(table) => {
            toml::Value::Table(redact_table(table, DATA_KEYED.contains(&key), development))
        }
        toml::Value::Array(items) => toml::Value::Array(
            items
                .iter()
                .map(|item| redact_value(key, item, development))
                .collect(),
        ),
        _ if development || !KNOWN_KEYS.contains(&key) || credential_key(key) || path_key(key) => {
            redacted()
        }
        toml::Value::Boolean(_) => value.clone(),
        toml::Value::Integer(_) | toml::Value::Float(_) if NUMBERS.contains(&key) => value.clone(),
        toml::Value::String(text) if URLS.contains(&key) => {
            stripped_url(text).map_or_else(redacted, toml::Value::String)
        }
        toml::Value::String(text) if KEPT.contains(&key) && shaped(key, text) => value.clone(),
        toml::Value::String(_)
        | toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Datetime(_) => redacted(),
    }
}

/// Whether `text` has the shape the [`KEPT`] key `key` takes.
fn shaped(key: &str, text: &str) -> bool {
    match key {
        "listen" => text.parse::<SocketAddr>().is_ok(),
        "base_path" => {
            text.starts_with('/')
                && text.len() <= 128
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
        }
        _ => {
            !text.is_empty()
                && text.len() <= TOKEN_LENGTH
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        }
    }
}

/// Returns the lower-case words of `key`, split at every character that is
/// no letter or digit and at every change from lower to upper case.
fn words(key: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut lower = false;
    for c in key.chars() {
        if !c.is_alphanumeric() {
            words.push(std::mem::take(&mut current));
            lower = false;
            continue;
        }
        if c.is_uppercase() && lower {
            words.push(std::mem::take(&mut current));
        }
        lower = c.is_lowercase() || c.is_ascii_digit();
        current.extend(c.to_lowercase());
    }
    words.push(current);
    words.retain(|word| !word.is_empty());
    words
}

/// Whether `key` names a credential, in any casing and between any
/// separators: one of its words is a [`CREDENTIAL_WORDS`] entry, or its
/// words joined contain one of the longer entries (`Pass Word`).
fn credential_key(key: &str) -> bool {
    let words = words(key);
    let joined = words.concat();
    words
        .iter()
        .any(|word| CREDENTIAL_WORDS.contains(&word.as_str()))
        || CREDENTIAL_WORDS
            .iter()
            .any(|word| word.len() > 3 && joined.contains(word))
}

/// Whether `key` names a file path: a `*_file` key, or the registry
/// `document`.
fn path_key(key: &str) -> bool {
    matches!(
        words(key).last().map(String::as_str),
        Some("file" | "document")
    )
}

/// Returns `text` as a URL with a host, keeping its scheme, host and port
/// and replacing its userinfo, path, query and fragment by [`REDACTED`];
/// `None` when it is no such URL.
fn stripped_url(text: &str) -> Option<String> {
    // NOTE: no specification governs this: our own design. A parse failure IS
    // the answer: text that is no URL with a host is redacted whole.
    let url = url::Url::parse(text).ok().filter(url::Url::has_host)?;
    let host = url.host_str()?;
    let mut out = format!("{}://", url.scheme());
    if !url.username().is_empty() || url.password().is_some() {
        out.push_str(REDACTED);
        out.push('@');
    }
    out.push_str(host);
    if let Some(port) = url.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    if url.path() != "/" && !url.path().is_empty() {
        out.push('/');
        out.push_str(REDACTED);
    }
    if url.query().is_some() {
        out.push('?');
        out.push_str(REDACTED);
    }
    if url.fragment().is_some() {
        out.push('#');
        out.push_str(REDACTED);
    }
    Some(out)
}

/// The value every redacted leaf becomes.
fn redacted() -> toml::Value {
    toml::Value::String(REDACTED.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{credential_key, redact, stripped_url, words};

    /// A configuration with a canary in every form the gateway reads a
    /// credential, a personal name, a namespace or a patient identifier: as
    /// a value, as a map key, inside a URL, in an inline table, in an array
    /// of tables and under `[dev]`.
    const CONFIGURATION: &str = r#"
profile = "development"

[server]
listen = "0.0.0.0:8080"
base_path = "/fed"
request_timeout_ms = 30000

[telemetry]
format = "json"
filter = "info,CANARY-FILTER=debug"

[federation]
id = "CANARY-FEDERATION"
default_namespace = "urn:oid:2.999.4242"

[metrics]
otlp_endpoint = "https://collector-user:collector-pass@otel.example.org:4317/CANARY-PATH?api_key=QUERY-SECRET#CANARY-FRAGMENT"

[stored_queries]
backend = "postgres"
url = "postgres://db-user:db-pass@db.example.org:5432/ferrofed?sslpassword=QUERY-SECRET-2"

[auth]
audience = "CANARY-AUDIENCE"

[[auth.issuer]]
issuer = "https://idp.example.org/realms/CANARY-REALM"
jwks = '{"keys":[{"kid":"CANARY-KID"}]}'
operator_scope = "CANARY-SCOPE"
patient = { header = "CANARY-HEADER", claim = "CANARY-CLAIM" }

[[pixm.manager]]
url = "https://pix-user:pix-pass@pix.example.org/fhir"

[pixm.manager.members]
"CANARY-MEMBER" = "urn:oid:2.999.7777"

[pixm.manager.namespaces]
"urn:oid:2.999.8888" = "CANARY-NAMESPACE-VALUE"

[credentials."CANARY-ENDPOINT"]
bearer_token = "BEARER-SECRET-1"
bearer_token_file = "/home/CANARY-PERSON/node-a-token"

[credentials."node-b-query"]
user = "Dr CANARY-PROFESSIONAL"
password = "PASSWORD-SECRET-1"
client_secret = "CLIENT-SECRET-1"
client_identity = "-----BEGIN PRIVATE KEY-----IDENTITY-SECRET"

[[dev.crossref]]
namespace = "urn:oid:2.999.1.1"
value = "PATIENT-ID-1"
ehr_id = "7d44b88c-4199-4bad-97dc-d78268e01398"
member = "node-a"
count = 7
"#;

    /// Every canary [`CONFIGURATION`] holds.
    const CANARIES: &[&str] = &[
        "collector-user",
        "collector-pass",
        "CANARY",
        "QUERY-SECRET",
        "db-user",
        "db-pass",
        "pix-user",
        "pix-pass",
        "2.999.4242",
        "2.999.7777",
        "2.999.8888",
        "BEARER-SECRET-1",
        "PASSWORD-SECRET-1",
        "CLIENT-SECRET-1",
        "IDENTITY-SECRET",
        "PATIENT-ID-1",
        "2.999.1.1",
        "7d44b88c-4199-4bad-97dc-d78268e01398",
        "node-a\"",
        "node-b-query",
        "ferrofed?",
        "/fhir",
    ];

    #[test]
    fn every_canary_value_and_key_is_redacted() {
        let tree: toml::Table = toml::from_str(CONFIGURATION).expect("the fixture parses");
        let written = toml::to_string(&redact(&tree)).expect("the redacted tree writes");
        for canary in CANARIES {
            assert!(!written.contains(canary), "{canary} leaked: {written}");
        }
        let redacted: toml::Table = toml::from_str(&written).expect("the output reads back");
        let dev = &redacted["dev"]["crossref"][0];
        assert!(
            dev.get("count").is_none(),
            "a key no list knows is never shown"
        );
        let dev = dev.as_table().expect("a table");
        for key in ["namespace", "value", "ehr_id", "member"] {
            assert!(dev.contains_key(key), "dev.crossref.{key} is shown");
        }
        for (key, value) in dev {
            assert_eq!(
                "***",
                value.as_str().expect("a string"),
                "dev.crossref.{key}"
            );
        }
        let credentials = redacted["credentials"].as_table().expect("a table");
        assert_eq!(
            vec!["***1", "***2"],
            credentials.keys().collect::<Vec<_>>(),
            "a table keyed by data shows no key"
        );
    }

    #[test]
    fn the_shape_and_the_safe_values_stay() {
        let tree: toml::Table = toml::from_str(CONFIGURATION).expect("the fixture parses");
        let redacted = redact(&tree);
        assert_eq!("development", redacted["profile"].as_str().expect("kept"));
        assert_eq!(
            "0.0.0.0:8080",
            redacted["server"]["listen"].as_str().expect("kept")
        );
        assert_eq!(
            "/fed",
            redacted["server"]["base_path"].as_str().expect("kept")
        );
        assert_eq!(
            30_000,
            redacted["server"]["request_timeout_ms"]
                .as_integer()
                .expect("kept")
        );
        assert_eq!(
            "postgres",
            redacted["stored_queries"]["backend"]
                .as_str()
                .expect("kept")
        );
        assert_eq!(
            "https://***@otel.example.org:4317/***?***#***",
            redacted["metrics"]["otlp_endpoint"]
                .as_str()
                .expect("stripped")
        );
        assert_eq!(
            "***",
            redacted["auth"]["issuer"][0]["patient"]["header"]
                .as_str()
                .expect("an inline table is redacted too")
        );
    }

    #[test]
    fn a_url_keeps_its_scheme_host_and_port_alone() {
        assert_eq!(
            Some("https://idp.example.org".to_owned()),
            stripped_url("https://idp.example.org/")
        );
        assert_eq!(
            Some("https://idp.example.org:8443/***".to_owned()),
            stripped_url("https://idp.example.org:8443/jwks")
        );
        assert_eq!(None, stripped_url("urn:oid:2.999.1"));
        assert_eq!(None, stripped_url("ffd-test-0038"));
    }

    /// Every shape a value could slip past a key-based redactor in, each
    /// carrying a canary: other casings and separators, unknown keys, deep
    /// nesting, arrays of arrays, inline tables, numbers, dates, URLs under
    /// keys that take none, and secret-shaped text under a kept key.
    const BYPASSES: &str = r#"
bearerToken = "CANARY-CAMEL"
"Bearer-Token" = "CANARY-KEBAB"
BEARER_TOKEN = 987654321
"client.secret" = "CANARY-DOTTED"
"Pass Word" = "CANARY-SPACED"
patient_number = 123456782
bsn = 999999990
birth = 1970-01-01
note = "https://canary-host.example.org/"
profile = "http://canary-user:canary-pass@h.example.org"
listen = "CANARY-NOT-AN-ADDRESS"
format = "CANARY WITH SPACES"
base_path = "/fed?CANARY-QUERY"
ehr_id = "7d44b88c-4199-4bad-97dc-d78268e01398"
header = "CANARY-HEADER-VALUE"
document = "/home/canary-person/registry.toml"
request_timeout_ms = "CANARY-STRING-IN-A-NUMBER"
nested = [["CANARY-ARRAY"], [{ password = "CANARY-INLINE" }]]
inline = { url = "https://canary-user2:canary-pass2@h.example.org/p?q#f" }

[a.b.c.d]
password = "CANARY-DEEP"
value = 111111110

[[DEV.crossref]]
value = 222222220
ehr_id = "c0ffee00-0000-4000-8000-000000000705"

[[auth.issuer]]
issuer = "https://idp.example.org"
client_secret = 333333330

[pixm.manager.members]
"node-a" = 444444440
"#;

    #[test]
    fn no_bypass_shape_carries_a_canary_through() {
        let tree: toml::Table = toml::from_str(BYPASSES).expect("the fixture parses");
        let written = toml::to_string(&redact(&tree)).expect("the redacted tree writes");
        for canary in [
            "CANARY",
            "canary",
            "987654321",
            "123456782",
            "999999990",
            "1970",
            "7d44b88c",
            "c0ffee00",
            "111111110",
            "222222220",
            "333333330",
            "444444440",
            "bearerToken",
            "Bearer-Token",
            "BEARER_TOKEN",
            "client.secret",
            "Pass Word",
            "patient_number",
            "bsn",
            "birth",
            "note",
            "DEV",
            "node-a",
        ] {
            assert!(!written.contains(canary), "{canary} leaked: {written}");
        }
        let redacted: toml::Table = toml::from_str(&written).expect("the output reads back");
        assert_eq!(
            "https://idp.example.org",
            redacted["auth"]["issuer"][0]["issuer"]
                .as_str()
                .expect("a URL under a URL key keeps its host")
        );
    }

    #[test]
    fn a_credential_word_is_found_in_any_casing_or_separator() {
        for key in [
            "bearer_token",
            "bearerToken",
            "Bearer-Token",
            "BEARER_TOKEN",
            "client.secret",
            "clientSecret",
            "Pass Word",
            "apiKey",
            "x-authorization",
        ] {
            assert!(credential_key(key), "{key}");
        }
        assert_eq!(vec!["client", "secret", "file"], words("clientSecret_FILE"));
        assert!(!credential_key("request_timeout_ms"));
    }
}
