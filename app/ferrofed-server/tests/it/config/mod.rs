// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration contract: the file, the environment over it, the `_file`
//! secrets, and every refusal.

mod admin;
mod drain;
#[cfg(feature = "binding-ihe")]
mod redaction;
mod refusals;
mod renamed;
mod replica;
mod retired;
mod secrets;
mod sources;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::io::Write;

/// A file with every section set, so a test can override one key at a time,
/// its node grant's key the suite's signing key file.
fn full() -> String {
    FULL.replace(
        "@KEY@",
        &toml::Value::String(crate::support::signing_key_file().to_owned()).to_string(),
    )
}

/// [`full`] before the key file is named.
const FULL: &str = r#"
[server]
listen = "0.0.0.0:9000"
request_timeout = '1s'
shutdown_timeout = '2s'
body_limit = '4KiB'

[telemetry]
format = "json"
filter = "debug"

[credentials."hospital-a".credentials.oauth]
token_endpoint = "https://as.hospital-a.example.org/token"
client_id = "ferrofed"
client_auth = "private_key_jwt"
private_key_file = @KEY@

[credentials."clinic-b"]
credentials.user = "gateway"
credentials.password = "synthetic-password"
"#;

/// Returns the environment map a single override makes.
fn env(name: &str, value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(name.to_owned(), value.to_owned())])
}

/// Resolves `text` with no environment and returns the refusal.
fn refusal(text: &str) -> Result<Error, Box<dyn StdError>> {
    match Config::from_sources(Some(text), &BTreeMap::new()).and_then(|config| config.resolve()) {
        Ok(_) => Err("the configuration was accepted".into()),
        Err(error) => Ok(error),
    }
}

/// Returns a temporary file holding `content`.
fn secret_file(content: &str) -> Result<tempfile::NamedTempFile, Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    Ok(file)
}

/// Everything an error shows: its `Display`, its `Debug` and the `Display`
/// of every error in its source chain.
fn everything(error: &Error) -> String {
    let mut rendered = format!("{error}\n{error:?}");
    let mut cause = std::error::Error::source(error);
    while let Some(source) = cause {
        rendered.push('\n');
        rendered.push_str(&source.to_string());
        cause = source.source();
    }
    rendered
}

/// A malformed row of the removed `[dev]` table, as a configuration written
/// for an older release may carry, whose identifier is a sentinel, one row
/// per way a row can be broken.
const BROKEN_DEV_ROWS: [&str; 3] = [
    // An unterminated string.
    "profile = \"development\"\n[[dev.crossref]]\nnamespace = \"urn:oid:2.999.1\"\nvalue = \"SENTINEL-7319\nmember = \"node-a\"\n",
    // A value with no key.
    "profile = \"development\"\n[[dev.crossref]]\nnamespace = \"urn:oid:2.999.1\"\n\"SENTINEL-7319\"\n",
    // A key set twice.
    "profile = \"development\"\n[[dev.crossref]]\nvalue = \"SENTINEL-7319\"\nvalue = \"SENTINEL-7319\"\n",
];

/// A federating configuration with `request_ms` and `overall_ms`; the
/// registry document is only named, since resolving never reads it.
fn federating(request_ms: u64, overall_ms: u64) -> String {
    format!(
        "[server]\nrequest_timeout = '{request_ms}ms'\n\n[registry]\ndocument = \"/nonexistent/registry.toml\"\n\n[federation]\nper_node_timeout = '1s'\noverall_timeout = '{overall_ms}ms'\n{}",
        crate::support::audit_toml()
    )
}
