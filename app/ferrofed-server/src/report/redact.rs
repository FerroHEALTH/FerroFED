// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The effective configuration with every value that could be a credential
//! or a patient identifier replaced, for the report bundle.
//!
//! Redaction is deny by default. Every key stays, so the bundle shows the
//! shape of the configuration. A value stays only when it is a number, a
//! boolean or a date, a path named by a `*_file` key or a registry
//! `document`, a URL with its userinfo, query and fragment removed, or a
//! string under one of the [`KEPT`] keys. Every other string, every value
//! under a credential-like key, and everything under `[dev]`, whose
//! cross-reference holds patient identifiers (§5.4.1, N33), is replaced by
//! [`REDACTED`]. A key added to the configuration later is therefore
//! redacted until it joins [`KEPT`]. No specification governs the report:
//! our own design.

use ferrofed_registry::secret::REDACTED;

/// The string-valued keys whose value is kept: enumerations, addresses,
/// paths, log filters, routing ids and namespaces, none of them a
/// credential or a patient identifier.
pub const KEPT: &[&str] = &[
    "audience",
    "backend",
    "base_path",
    "client_auth",
    "consent_disclosure",
    "default_namespace",
    "demographic_endpoint",
    "destination",
    "edge",
    "ehr_id_system",
    "filter",
    "format",
    "grant",
    "header",
    "id",
    "listen",
    "mode",
    "next_key_algorithm",
    "node_selection",
    "offset_strategy",
    "on_failure",
    "operator_scope",
    "organisation_type",
    "profile",
    "purpose_of_use",
    "registry_format",
    "role",
    "scope",
];

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

/// Returns `tree` with every value [this module](self) does not keep
/// replaced by [`REDACTED`].
#[must_use]
pub fn redact(tree: &toml::Table) -> toml::Table {
    tree.iter()
        .map(|(key, value)| {
            let kept = if key == DEVELOPMENT {
                replace_all(value)
            } else {
                redact_value(key, value)
            };
            (key.clone(), kept)
        })
        .collect()
}

/// Returns `value`, held under `key`, redacted.
fn redact_value(key: &str, value: &toml::Value) -> toml::Value {
    match value {
        toml::Value::Table(table) => toml::Value::Table(redact(table)),
        toml::Value::Array(items) => {
            toml::Value::Array(items.iter().map(|item| redact_value(key, item)).collect())
        }
        _ if path_key(key) => match value {
            toml::Value::String(_) => value.clone(),
            _ => redacted(),
        },
        _ if credential_key(key) => redacted(),
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

/// Returns `value` with every leaf replaced and every key kept.
fn replace_all(value: &toml::Value) -> toml::Value {
    match value {
        toml::Value::Table(table) => toml::Value::Table(
            table
                .iter()
                .map(|(key, value)| (key.clone(), replace_all(value)))
                .collect(),
        ),
        toml::Value::Array(items) => toml::Value::Array(items.iter().map(replace_all).collect()),
        _ => redacted(),
    }
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

/// Returns `text` as a URL with a host, with its userinfo, query and
/// fragment replaced by [`REDACTED`]; `None` when it is no such URL.
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
    out.push_str(url.path());
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

    /// A configuration with a secret in every form the gateway reads one,
    /// URL credentials, and a development cross-reference.
    const CONFIGURATION: &str = r#"
profile = "development"

[server]
listen = "0.0.0.0:8080"
base_path = "/fed"
request_timeout_ms = 30000

[telemetry]
format = "json"

[metrics]
otlp_endpoint = "https://collector-user:collector-pass@otel.example.org:4317/v1?api_key=QUERY-SECRET#frag"

[stored_queries]
backend = "postgres"
url = "postgres://db-user:db-pass@db.example.org:5432/ferrofed?sslpassword=QUERY-SECRET-2"

[credentials."node-a-query"]
bearer_token = "BEARER-SECRET-1"
bearer_token_file = "/run/secrets/node-a-token"

[credentials."node-b-query"]
user = "USER-NAME-1"
password = "PASSWORD-SECRET-1"
client_secret = "CLIENT-SECRET-1"
client_identity = "-----BEGIN PRIVATE KEY-----IDENTITY-SECRET"

[[dev.crossref]]
namespace = "urn:oid:2.999.1.1"
value = "PATIENT-ID-1"
ehr_id = "7d44b88c-4199-4bad-97dc-d78268e01398"
endpoint = "node-a-query"
count = 7
"#;

    #[test]
    fn every_secret_url_credential_and_development_value_is_redacted() {
        let tree: toml::Table = toml::from_str(CONFIGURATION).expect("the fixture parses");
        let written = toml::to_string(&redact(&tree)).expect("the redacted tree writes");
        for secret in [
            "collector-user",
            "collector-pass",
            "QUERY-SECRET",
            "frag",
            "db-user",
            "db-pass",
            "BEARER-SECRET-1",
            "USER-NAME-1",
            "PASSWORD-SECRET-1",
            "CLIENT-SECRET-1",
            "IDENTITY-SECRET",
            "PATIENT-ID-1",
            "urn:oid:2.999.1.1",
            "7d44b88c-4199-4bad-97dc-d78268e01398",
        ] {
            assert!(!written.contains(secret), "{secret} leaked: {written}");
        }
        let redacted: toml::Table = toml::from_str(&written).expect("the output reads back");
        let dev = &redacted["dev"]["crossref"][0];
        for key in ["namespace", "value", "ehr_id", "endpoint", "count"] {
            assert_eq!(
                "***",
                dev[key].as_str().expect("a string"),
                "dev.crossref.{key}"
            );
        }
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
            "https://***@otel.example.org:4317/v1?***#***",
            redacted["metrics"]["otlp_endpoint"]
                .as_str()
                .expect("stripped")
        );
        assert_eq!(
            "/run/secrets/node-a-token",
            redacted["credentials"]["node-a-query"]["bearer_token_file"]
                .as_str()
                .expect("a path stays")
        );
        assert_eq!(
            "***",
            redacted["credentials"]["node-b-query"]["user"]
                .as_str()
                .expect("an unlisted string is redacted")
        );
    }

    #[test]
    fn only_a_url_with_a_host_is_stripped() {
        assert_eq!(
            Some("https://idp.example.org/jwks".to_owned()),
            stripped_url("https://idp.example.org/jwks")
        );
        assert_eq!(None, stripped_url("urn:oid:2.999.1"));
        assert_eq!(None, stripped_url("ffd-test-0038"));
    }
}
