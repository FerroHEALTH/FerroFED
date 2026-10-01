// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration: an optional TOML file, then environment overrides, then one
//! typed [`Settings`] the run path holds.
//!
//! Every struct refuses an unknown key, every default lives inline in its own
//! `Default` impl, every secret is reachable through a `<key>_file` sibling
//! read at boot, and a bad value refuses to boot rather than falling back. No
//! specification governs the configuration: our own design.

use ferrofed_engine::fanout::Budget;
use ferrofed_identity::dev::{DevTable, Profile};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::error::{Error, Stage};
use crate::config::secrets::resolve_credentials;
use crate::config::settings::{FederationSettings, ServerSettings, Settings, TelemetrySettings};
use crate::telemetry::{DEFAULT_FILTER, Format};

pub mod error;
mod secrets;
pub mod settings;

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
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The deployment profile: `production`, or `development`, the only
    /// profile that admits the static cross-reference of `[dev]`.
    pub profile: Profile,
    /// The HTTP surface.
    pub server: Server,
    /// The console.
    pub telemetry: Telemetry,
    /// The federation's membership.
    pub registry: Registry,
    /// The federated query: the budgets and the default issuing namespace.
    pub federation: Federation,
    /// The outbound credentials, one section per endpoint id
    /// (`[credentials."<endpoint id>"]`).
    ///
    /// The registry names the endpoints, and the node dispatch hands each one
    /// its credentials.
    pub credentials: BTreeMap<String, Credentials>,
    /// The static development cross-reference (`[[dev.crossref]]`), accepted
    /// only under `profile = "development"`.
    pub dev: Option<DevSection>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Profile::Production,
            server: Server::default(),
            telemetry: Telemetry::default(),
            registry: Registry::default(),
            federation: Federation::default(),
            credentials: BTreeMap::new(),
            dev: None,
        }
    }
}

/// The federation's membership.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Registry {
    /// The reviewed registry document naming the organisations, nodes and
    /// endpoints (`docs/architecture.md` section 8). Without it the gateway
    /// federates nothing, and the ITS-REST surface stays unserved.
    pub document: Option<PathBuf>,
}

/// The federated query.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Federation {
    /// How long one node's request may take (§11.5, N38).
    pub per_node_timeout_ms: u64,
    /// How long the whole fan-out may take (§11.5, N38). It must be shorter
    /// than `server.request_timeout_ms`, so the gateway answers with its
    /// envelope before the request timeout cuts the connection.
    pub overall_timeout_ms: u64,
    /// The issuing namespace an unqualified patient identifier resolves in
    /// (decision A5). Without it, a query that names no namespace is a `400`.
    pub default_namespace: Option<String>,
}

impl Default for Federation {
    fn default() -> Self {
        Self {
            per_node_timeout_ms: 10_000,
            overall_timeout_ms: 25_000,
            default_namespace: None,
        }
    }
}

/// The `[dev]` table, held as written until the registry it refers to is
/// loaded.
///
/// Its rows carry patient identifier values, so `Debug` shows how many rows
/// there are and none of them.
#[derive(Clone, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct DevSection(toml::Table);

impl DevSection {
    /// Reads the table as the static cross-reference's configuration.
    ///
    /// # Errors
    /// Returns [`Error::DevTable`] when the table does not have the shape of
    /// `[[dev.crossref]]` rows. The error names the shape, never a value.
    pub fn table(&self) -> Result<DevTable, Error> {
        toml::Value::Table(self.0.clone())
            .try_into::<DevTable>()
            .map_err(|_shape| Error::DevTable)
    }
}

impl fmt::Debug for DevSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rows = self
            .0
            .get("crossref")
            .and_then(toml::Value::as_array)
            .map_or(0, Vec::len);
        f.debug_struct("DevSection")
            .field("crossref_rows", &rows)
            .finish()
    }
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
        let text = match &named {
            None => None,
            Some(path) => {
                let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
                    path: path.clone(),
                    source,
                })?;
                Some(text)
            }
        };
        let environment: BTreeMap<String, String> = std::env::vars()
            .filter(|(name, _)| name.starts_with(ENV_PREFIX))
            .collect();
        let loaded = Self::from_sources(text.as_deref(), &environment);
        match &named {
            Some(path) => loaded.map_err(|error| error.in_file(path)),
            None => loaded,
        }
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
            Some(text) => {
                toml::from_str(text).map_err(|error| Error::parse(&error, text, Stage::File))?
            }
        };
        for (name, raw) in environment {
            apply_override(&mut table, name, raw)?;
        }
        // The merged tree is written back and re-read so every refusal carries
        // the key and its position, which a `Table` alone cannot report.
        let merged = toml::to_string(&table).map_err(|source| Error::Assemble { source })?;
        toml::from_str(&merged).map_err(|error| {
            let fault = Error::parse(&error, &merged, Stage::Merged);
            text.and_then(|text| Self::in_the_file(text, &fault))
                .unwrap_or(fault)
        })
    }

    /// Reads the file alone again when the merged tree is refused, so a fault
    /// the file itself carries is reported at its line and column.
    ///
    /// The file's own refusal is taken only when it names the same key: a
    /// fault an override introduced has no line in the file.
    fn in_the_file(text: &str, merged: &Error) -> Option<Error> {
        let file = toml::from_str::<Self>(text)
            .err()
            .map(|error| Error::parse(&error, text, Stage::File))?;
        let same_key = match (merged, &file) {
            (Error::Parse { fault: merged }, Error::Parse { fault: file }) => {
                let unquoted =
                    |key: &Option<String>| key.as_deref().map(|key| key.replace('"', ""));
                unquoted(&merged.key) == unquoted(&file.key)
            }
            _ => false,
        };
        same_key.then_some(file)
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
    /// [`Error::NoScheme`], [`Error::Budget`]), each naming the key that
    /// carries the fault.
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
        let federation = self.resolve_federation(request_timeout)?;
        Ok(Settings {
            profile: self.profile,
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
            registry_document: self.registry.document.clone(),
            federation,
            credentials,
            dev: self.dev.clone(),
        })
    }

    /// Resolves `[federation]`: both budgets positive (§11.5), and the overall
    /// one ending before `request_timeout` when a registry is configured.
    fn resolve_federation(&self, request_timeout: Duration) -> Result<FederationSettings, Error> {
        let per_node = positive_ms(
            "federation.per_node_timeout_ms",
            self.federation.per_node_timeout_ms,
        )?;
        let overall = positive_ms(
            "federation.overall_timeout_ms",
            self.federation.overall_timeout_ms,
        )?;
        // NOTE: §11.4, the budget only bounds a fan-out, so it is held to the
        // request timeout only when the gateway federates.
        if self.registry.document.is_some() && overall >= request_timeout {
            return Err(Error::Budget {
                overall_ms: self.federation.overall_timeout_ms,
                request_ms: self.server.request_timeout_ms,
            });
        }
        let budget = Budget::new(per_node, overall).map_err(|_zero| Error::Zero {
            key: String::from("federation"),
        })?;
        if let Some(namespace) = &self.federation.default_namespace
            && namespace.is_empty()
        {
            return Err(Error::Missing {
                key: String::from("federation.default_namespace"),
            });
        }
        Ok(FederationSettings {
            budget,
            default_namespace: self.federation.default_namespace.clone(),
        })
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
