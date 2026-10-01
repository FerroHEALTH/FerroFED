// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration: an optional TOML file, then environment overrides, then one
//! typed [`Settings`] the run path holds.
//!
//! Every struct refuses an unknown key, every default lives inline in its own
//! `Default` impl, every secret is reachable through a `<key>_file` sibling
//! read at boot, and a bad value refuses to boot rather than falling back. No
//! specification governs the configuration: our own design.

use secrecy::SecretString;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::telemetry::{DEFAULT_FILTER, Format};

/// The prefix of every environment override.
///
/// The name after it is the dotted key with `__` between segments, upper or
/// lower case: `FERROFED__SERVER__LISTEN` sets `[server] listen`.
pub const ENV_PREFIX: &str = "FERROFED__";

/// The environment variable naming the configuration file.
pub const CONFIG_PATH_ENV: &str = "FERROFED_CONFIG";

/// The longest endpoint id a credentials section may be keyed by.
pub const MAX_ENDPOINT_ID_LENGTH: usize = 128;

/// The whole configuration tree, as a file and the environment state it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The HTTP surface.
    pub server: Server,
    /// The console.
    pub telemetry: Telemetry,
    /// The outbound credentials, one section per endpoint id
    /// (`[credentials."<endpoint id>"]`).
    ///
    /// The registry (#36) names the endpoints; the node dispatch (#34) hands
    /// each one its credentials. Both are read and checked at boot today.
    pub credentials: BTreeMap<String, Credentials>,
}

/// The HTTP surface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address to bind.
    pub listen: String,
    /// How long one request may take before the server answers `408`.
    pub request_timeout_ms: u64,
    /// How long the drain may take after the stop signal.
    pub shutdown_timeout_ms: u64,
    /// The largest request body the server reads before answering `413`.
    pub body_limit_bytes: usize,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:8080"),
            request_timeout_ms: 30_000,
            shutdown_timeout_ms: 10_000,
            body_limit_bytes: 1024 * 1024,
        }
    }
}

/// The console.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Telemetry {
    /// The rendering: `auto`, `json` or `pretty`.
    pub format: Format,
    /// The `tracing` filter directive.
    pub filter: String,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            format: Format::Auto,
            filter: String::from(DEFAULT_FILTER),
        }
    }
}

/// The credentials one endpoint expects.
///
/// Every secret is reachable inline or through its `_file` sibling; setting
/// both is a boot error, and so is naming two schemes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Credentials {
    /// An RFC 6750 bearer token.
    pub bearer_token: Option<String>,
    /// A file holding the bearer token, read at boot.
    pub bearer_token_file: Option<PathBuf>,
    /// The user name of RFC 7617 basic authentication.
    pub user: Option<String>,
    /// The password of RFC 7617 basic authentication.
    pub password: Option<String>,
    /// A file holding the password, read at boot.
    pub password_file: Option<PathBuf>,
}

/// A configuration the server refuses to start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The configuration file could not be read.
    #[error("the configuration file {} could not be read", path.display())]
    Read {
        /// The path that was tried.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The configuration does not parse, names an unknown key, or holds a
    /// value of the wrong type.
    #[error("the configuration is not valid")]
    Parse {
        /// What the TOML reader reported, with the offending key.
        #[source]
        source: toml::de::Error,
    },
    /// The merged configuration could not be written back for re-reading.
    #[error("the configuration could not be assembled")]
    Assemble {
        /// What the TOML writer reported.
        #[source]
        source: toml::ser::Error,
    },
    /// An environment override names no key under the prefix.
    #[error("{name} names no configuration key; use {ENV_PREFIX}<SECTION>__<KEY>")]
    EnvName {
        /// The variable that was read.
        name: String,
    },
    /// An environment override addresses a key under a value that is not a
    /// section.
    #[error("{name} addresses a key under a value that is not a section")]
    EnvShape {
        /// The variable that was read.
        name: String,
    },
    /// A value and its `_file` sibling are both set.
    #[error("{key} is set together with {key}_file; set one of them")]
    Conflict {
        /// The inline key.
        key: String,
    },
    /// A `_file` sibling could not be read.
    #[error("{key} names {}, which could not be read", path.display())]
    Secret {
        /// The `_file` key.
        key: String,
        /// The path it named.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A secret read from a `_file` sibling is empty.
    #[error("{key} names {}, which holds no secret", path.display())]
    EmptySecret {
        /// The `_file` key.
        key: String,
        /// The path it named.
        path: PathBuf,
    },
    /// A key a section needs is not set.
    #[error("{key} is not set, and its section needs it")]
    Missing {
        /// The key that carries no value.
        key: String,
    },
    /// A socket address does not parse.
    #[error("{key} is not a socket address")]
    Listen {
        /// The key that holds it.
        key: String,
        /// What the address parser reported.
        #[source]
        source: std::net::AddrParseError,
    },
    /// A duration or a size that must be positive is zero.
    #[error("{key} is zero; it must be positive")]
    Zero {
        /// The key that holds it.
        key: String,
    },
    /// The log filter does not parse.
    #[error("telemetry.filter is not a valid tracing filter")]
    Filter {
        /// What the filter parser reported.
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },
    /// A credentials section is keyed by something that is not an endpoint id.
    #[error(
        "credentials.{key:?} is not an endpoint id: one to {MAX_ENDPOINT_ID_LENGTH} printable ASCII characters with no space"
    )]
    EndpointId {
        /// The key that was given.
        key: String,
    },
    /// A credentials section names both a bearer token and a user.
    #[error("{section} names both a bearer token and a user; set one scheme")]
    Scheme {
        /// The credentials section.
        section: String,
    },
    /// A credentials section names no scheme at all.
    #[error("{section} names no credentials; remove the section or set one scheme")]
    NoScheme {
        /// The credentials section.
        section: String,
    },
}

impl Config {
    /// Reads the configuration file `path` names, then the environment.
    ///
    /// `path` is what `--config` named; without it the file is the one
    /// `FERROFED_CONFIG` names, and without that the defaults stand.
    ///
    /// # Errors
    /// Returns [`Error::Read`] when a named file cannot be read and every
    /// other [`Error`] the merge and the parse produce.
    pub fn load(path: Option<&Path>) -> Result<Self, Error> {
        let named = path
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os(CONFIG_PATH_ENV).map(PathBuf::from));
        let text = match named {
            None => None,
            Some(path) => {
                let text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
                    path: path.clone(),
                    source,
                })?;
                Some(text)
            }
        };
        let environment: BTreeMap<String, String> = std::env::vars()
            .filter(|(name, _)| name.starts_with(ENV_PREFIX))
            .collect();
        Self::from_sources(text.as_deref(), &environment)
    }

    /// Reads `text` as TOML and applies `environment` over it.
    ///
    /// Reading the sources and reading the process environment are separate so
    /// a test can state both: setting an environment variable is `unsafe` in
    /// edition 2024 and this workspace forbids `unsafe`.
    ///
    /// # Errors
    /// Returns [`Error::Parse`] for TOML that does not read as this tree,
    /// [`Error::EnvName`] and [`Error::EnvShape`] for an override that names
    /// no key, and [`Error::Assemble`] when the merged tree cannot be written.
    pub fn from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        let mut table = match text {
            None => toml::Table::new(),
            Some(text) => toml::from_str(text).map_err(|source| Error::Parse { source })?,
        };
        for (name, raw) in environment {
            apply_override(&mut table, name, raw)?;
        }
        // The merged tree is written back and re-read so every refusal carries
        // the key and its position, which a `Table` alone cannot report.
        let merged = toml::to_string(&table).map_err(|source| Error::Assemble { source })?;
        toml::from_str(&merged).map_err(|source| Error::Parse { source })
    }

    /// Resolves this tree into the settings the run path holds.
    ///
    /// Every `_file` sibling is read here, so a secret reaches the process
    /// once, at boot, and never sits in the configuration tree.
    ///
    /// # Errors
    /// Returns [`Error::Conflict`] when a value and its `_file` sibling are
    /// both set, [`Error::Secret`] and [`Error::EmptySecret`] when a `_file`
    /// cannot be read or holds nothing, and the value errors
    /// ([`Error::Listen`], [`Error::Zero`], [`Error::Filter`],
    /// [`Error::EndpointId`], [`Error::Missing`], [`Error::Scheme`],
    /// [`Error::NoScheme`]), each naming the key that carries the fault.
    pub fn resolve(&self) -> Result<Settings, Error> {
        let listen = self
            .server
            .listen
            .parse::<SocketAddr>()
            .map_err(|source| Error::Listen {
                key: String::from("server.listen"),
                source,
            })?;
        let request_timeout =
            positive_ms("server.request_timeout_ms", self.server.request_timeout_ms)?;
        let shutdown_timeout = positive_ms(
            "server.shutdown_timeout_ms",
            self.server.shutdown_timeout_ms,
        )?;
        if self.server.body_limit_bytes == 0 {
            return Err(Error::Zero {
                key: String::from("server.body_limit_bytes"),
            });
        }
        tracing_subscriber::EnvFilter::try_new(&self.telemetry.filter)
            .map_err(|source| Error::Filter { source })?;
        let mut credentials = BTreeMap::new();
        for (endpoint, section) in &self.credentials {
            if !is_endpoint_id(endpoint) {
                return Err(Error::EndpointId {
                    key: endpoint.clone(),
                });
            }
            let scheme = resolve_credentials(&format!("credentials.{endpoint}"), section)?;
            credentials.insert(endpoint.clone(), scheme);
        }
        Ok(Settings {
            server: ServerSettings {
                listen,
                request_timeout,
                shutdown_timeout,
                body_limit: self.server.body_limit_bytes,
            },
            telemetry: TelemetrySettings {
                format: self.telemetry.format,
                filter: self.telemetry.filter.clone(),
            },
            credentials,
        })
    }
}

/// The settings the run path holds, with every secret already read.
#[derive(Debug)]
pub struct Settings {
    /// The HTTP surface.
    pub server: ServerSettings,
    /// The console.
    pub telemetry: TelemetrySettings,
    /// The outbound credentials, by endpoint id.
    pub credentials: BTreeMap<String, Scheme>,
}

/// The HTTP surface, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSettings {
    /// The socket address to bind.
    pub listen: SocketAddr,
    /// How long one request may take before the server answers `408`.
    pub request_timeout: Duration,
    /// How long the drain may take after the stop signal.
    pub shutdown_timeout: Duration,
    /// The largest request body the server reads before answering `413`.
    pub body_limit: usize,
}

/// The console, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetrySettings {
    /// The rendering.
    pub format: Format,
    /// The `tracing` filter directive, already known to parse.
    pub filter: String,
}

/// The authentication scheme a credentials section resolves to.
///
/// `Debug` redacts every secret, because [`SecretString`] does.
#[derive(Debug)]
#[non_exhaustive]
pub enum Scheme {
    /// An RFC 6750 bearer token.
    Bearer(SecretString),
    /// RFC 7617 basic authentication.
    Basic {
        /// The user name, which is not a secret.
        user: String,
        /// The password.
        password: SecretString,
    },
}

impl Settings {
    /// Logs what this process is configured to reach, never a value.
    ///
    /// The line names the endpoints that carry credentials and never the
    /// credentials, so a start-up log states what the process can reach
    /// without stating any of it.
    pub fn log_summary(&self) {
        let endpoints: Vec<&str> = self.credentials.keys().map(String::as_str).collect();
        tracing::info!(
            listen = %self.server.listen,
            credentials = endpoints.join(","),
            "configuration resolved"
        );
    }
}

/// Returns the duration `millis` names, refusing zero under `key`.
fn positive_ms(key: &str, millis: u64) -> Result<Duration, Error> {
    if millis == 0 {
        return Err(Error::Zero {
            key: key.to_owned(),
        });
    }
    Ok(Duration::from_millis(millis))
}

/// Returns whether `key` may name an endpoint.
///
/// The registry (#36) owns the endpoint id type; until it lands the rule is the
/// one a log line and a header can carry safely: printable ASCII with no space,
/// bounded in length.
fn is_endpoint_id(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_ENDPOINT_ID_LENGTH
        && key.chars().all(|c| c.is_ascii_graphic())
}

/// Applies one environment override onto `table`.
fn apply_override(table: &mut toml::Table, name: &str, raw: &str) -> Result<(), Error> {
    let Some(path) = name.strip_prefix(ENV_PREFIX) else {
        return Ok(());
    };
    let segments: Vec<String> = path.split("__").map(str::to_ascii_lowercase).collect();
    let Some((key, parents)) = segments.split_last() else {
        return Err(Error::EnvName {
            name: name.to_owned(),
        });
    };
    if key.is_empty() || parents.is_empty() || parents.iter().any(String::is_empty) {
        return Err(Error::EnvName {
            name: name.to_owned(),
        });
    }
    let mut cursor = table;
    for parent in parents {
        let entry = cursor
            .entry(parent.clone())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let toml::Value::Table(next) = entry else {
            return Err(Error::EnvShape {
                name: name.to_owned(),
            });
        };
        cursor = next;
    }
    cursor.insert(key.clone(), env_value(raw));
    Ok(())
}

/// Reads `raw` as a TOML value, or as the string it is.
///
/// An environment variable carries text, so a number, a boolean and an array
/// are spelled in TOML syntax and everything else is the string itself.
fn env_value(raw: &str) -> toml::Value {
    // NOTE: no specification governs this: our own design. A parse failure IS
    // the answer here, because text that is not TOML syntax is a plain string.
    let parsed = toml::from_str::<toml::Table>(&format!("value = {raw}"))
        .ok()
        .and_then(|table| table.get("value").cloned());
    parsed.unwrap_or_else(|| toml::Value::String(raw.to_owned()))
}

/// Returns the scheme `credentials` describes.
fn resolve_credentials(section: &str, credentials: &Credentials) -> Result<Scheme, Error> {
    let token = secret(
        &format!("{section}.bearer_token"),
        credentials.bearer_token.as_deref(),
        credentials.bearer_token_file.as_deref(),
    )?;
    let password = secret(
        &format!("{section}.password"),
        credentials.password.as_deref(),
        credentials.password_file.as_deref(),
    )?;
    match (token, credentials.user.as_deref(), password) {
        (Some(_), Some(_), _) | (Some(_), None, Some(_)) => Err(Error::Scheme {
            section: section.to_owned(),
        }),
        (Some(token), None, None) => Ok(Scheme::Bearer(token)),
        (None, Some(user), Some(password)) => Ok(Scheme::Basic {
            user: user.to_owned(),
            password,
        }),
        (None, Some(_), None) => Err(Error::Missing {
            key: format!("{section}.password"),
        }),
        (None, None, Some(_)) => Err(Error::Missing {
            key: format!("{section}.user"),
        }),
        (None, None, None) => Err(Error::NoScheme {
            section: section.to_owned(),
        }),
    }
}

/// Returns the secret `key` names, inline or from its `_file` sibling.
fn secret(
    key: &str,
    inline: Option<&str>,
    file: Option<&Path>,
) -> Result<Option<SecretString>, Error> {
    match (inline, file) {
        (Some(_), Some(_)) => Err(Error::Conflict {
            key: key.to_owned(),
        }),
        (Some(value), None) => Ok(Some(SecretString::from(value))),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: format!("{key}_file"),
                path: path.to_path_buf(),
                source,
            })?;
            let value = text.trim();
            if value.is_empty() {
                return Err(Error::EmptySecret {
                    key: format!("{key}_file"),
                    path: path.to_path_buf(),
                });
            }
            Ok(Some(SecretString::from(value)))
        }
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Error, apply_override, env_value, is_endpoint_id};
    use std::collections::BTreeMap;

    #[test]
    fn an_absent_file_and_an_empty_environment_leave_the_defaults() {
        let config = Config::from_sources(None, &BTreeMap::new()).expect("the defaults parse");
        assert_eq!(Config::default(), config);
        assert_eq!("127.0.0.1:8080", config.server.listen);
        assert_eq!(1024 * 1024, config.server.body_limit_bytes);
        assert_eq!(10_000, config.server.shutdown_timeout_ms);
        assert!(config.credentials.is_empty());
        config.resolve().expect("the defaults resolve");
    }

    #[test]
    fn an_environment_value_reads_as_toml_syntax_or_as_the_string_it_is() {
        assert_eq!(toml::Value::Integer(5), env_value("5"));
        assert_eq!(toml::Value::Boolean(true), env_value("true"));
        assert_eq!(
            toml::Value::String(String::from("0.0.0.0:8080")),
            env_value("0.0.0.0:8080")
        );
    }

    #[test]
    fn an_override_name_without_a_section_and_a_key_is_refused() {
        let mut table = toml::Table::new();
        let error = apply_override(&mut table, "FERROFED__LISTEN", "x")
            .expect_err("a section and a key are both required");
        assert!(matches!(error, Error::EnvName { .. }), "{error:?}");
    }

    #[test]
    fn an_endpoint_id_is_bounded_printable_ascii_with_no_space() {
        assert!(is_endpoint_id("hospital-a.query"));
        assert!(!is_endpoint_id(""));
        assert!(!is_endpoint_id("node a"));
        assert!(!is_endpoint_id("node\u{e9}"));
        assert!(!is_endpoint_id(
            &"a".repeat(super::MAX_ENDPOINT_ID_LENGTH + 1)
        ));
    }
}
