// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The effective configuration with every value, and every key, that could
//! carry a credential, a personal name or a patient identifier replaced, for
//! the report bundle.
//!
//! Redaction is deny by default and runs over the parsed tree, before
//! anything is serialized, so a comment or an inline table cannot pass it.
//! A key stays when it is a configuration field name (`snake_case`) outside
//! a table keyed by data ([`DATA_KEYED`], whose keys are endpoint ids,
//! member ids or namespaces); any other key becomes a numbered [`REDACTED`]
//! placeholder, so the shape stays visible. A value stays only when it is a
//! number, a boolean or a date, the scheme, host and port of a URL, or a
//! string under one of the [`KEPT`] keys. Everything else is [`REDACTED`]:
//! every other string, a file path (which can name a person's home), a URL's
//! userinfo, path, query and fragment, every value under a credential-like
//! key, and everything under `[dev]`, whose cross-reference holds patient
//! identifiers (§5.4.1, N33). A key added to the configuration later is
//! redacted until it joins [`KEPT`]. No specification governs the report:
//! our own design.

use ferrofed_registry::secret::REDACTED;

/// The string-valued keys whose value is kept: closed enumerations and
/// listen addresses, none of which can carry a credential, a personal name,
/// a namespace or a patient identifier.
pub const KEPT: &[&str] = &[
    "backend",
    "base_path",
    "client_auth",
    "consent_disclosure",
    "destination",
    "edge",
    "format",
    "grant",
    "listen",
    "mode",
    "next_key_algorithm",
    "node_selection",
    "offset_strategy",
    "on_failure",
    "profile",
    "registry_format",
];

/// The tables whose keys are data, never field names: the outbound
/// credentials by endpoint id, and the member, namespace and community maps
/// of the identity bindings.
pub const DATA_KEYED: &[&str] = &["communities", "credentials", "members", "namespaces"];

/// The parts of a key that mark its value as a credential, whatever its
/// type.
const CREDENTIAL_PARTS: &[&str] = &[
    "assertion",
    "identity",
    "key",
    "password",
    "secret",
    "token",
];

/// The section whose every value is replaced: the development
/// cross-reference, which pairs patient identifiers with `ehr_id`s.
const DEVELOPMENT: &str = "dev";

/// Returns `tree` with every key and value [this module](self) does not
/// keep replaced by [`REDACTED`].
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
            let shown = if data_keyed || !field_name(key) {
                format!("{REDACTED}{}", position.saturating_add(1))
            } else {
                key.clone()
            };
            let development = development || key == DEVELOPMENT;
            (shown, redact_value(key, value, development))
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
        _ if development || credential_key(key) || path_key(key) => redacted(),
        toml::Value::String(text) => redact_string(key, text),
        toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_)
        | toml::Value::Datetime(_) => value.clone(),
    }
}

/// Returns `text`, held under `key`, kept, stripped as a URL, or redacted.
fn redact_string(key: &str, text: &str) -> toml::Value {
    if let Some(stripped) = stripped_url(text) {
        return toml::Value::String(stripped);
    }
    if KEPT.contains(&key) {
        toml::Value::String(text.to_owned())
    } else {
        redacted()
    }
}

/// Whether `key` is a configuration field name: lower-case ASCII letters,
/// digits and underscores, opening with a letter.
fn field_name(key: &str) -> bool {
    key.chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Whether `key` names a file path: a `*_file` key, or the registry
/// `document`.
fn path_key(key: &str) -> bool {
    key.ends_with("_file") || key == "document"
}

/// Whether `key` names a credential.
fn credential_key(key: &str) -> bool {
    key.split('_').any(|part| CREDENTIAL_PARTS.contains(&part))
}

/// Returns `text` as a URL with a host, keeping its scheme, host and port
/// and replacing its userinfo, path, query and fragment by [`REDACTED`];
/// `None` when it is no such URL.
fn stripped_url(text: &str) -> Option<String> {
    // NOTE: no specification governs this: our own design. A parse failure IS
    // the answer: text that is no URL with a host is judged by its key instead.
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
    use super::{redact, stripped_url};

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
        for key in ["namespace", "value", "ehr_id", "member", "count"] {
            assert_eq!(
                "***",
                dev[key].as_str().expect("a string"),
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
}
