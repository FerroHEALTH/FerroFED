// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the configuration tree: the TOML file, then every `FERROFED__`
//! environment override applied over it. No specification governs the
//! configuration: our own design.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::error::Error;
use crate::config::error::parse::Stage;
use crate::config::{CONFIG_PATH_ENV, Config, ENV_PREFIX};

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
    use super::{apply_override, env_value};
    use crate::config::Config;
    use crate::config::error::Error;
    use std::collections::BTreeMap;

    #[test]
    fn an_absent_file_and_an_empty_environment_leave_the_defaults() {
        let config = Config::from_sources(None, &BTreeMap::new()).expect("the defaults parse");
        assert_eq!(Config::default(), config);
        assert_eq!("127.0.0.1:8080", config.server.listen);
        assert_eq!(1024 * 1024, config.server.body_limit_bytes);
        assert_eq!(0, config.server.drain_delay_ms);
        assert_eq!(None, config.server.shutdown_timeout_ms);
        assert!(config.credentials.is_empty());
        let settings = config.resolve().expect("the defaults resolve");
        assert_eq!(
            settings.server.request_timeout, settings.server.shutdown_timeout,
            "the drain defaults to the request timeout"
        );
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
}
