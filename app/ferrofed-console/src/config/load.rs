// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the configuration tree through `ferrohealth-config`.
//!
//! The defaults, the TOML file, every `FERROFED_CONSOLE__` environment
//! override and every `--set` pair applied over it. No specification governs
//! the configuration: our own design.

use std::collections::BTreeMap;
use std::path::Path;

use ferrohealth_config::loader::Loaded;

use crate::config::error::Error;
use crate::config::{Config, loader};

impl Config {
    /// Reads the configuration of the running process: the file `path`
    /// names, or the one `FERROFED_CONSOLE_CONFIG` names, the environment
    /// and the `--set` pairs `sets`, with where each key came from.
    ///
    /// # Errors
    /// Returns [`Error::Load`] when the file cannot be read, a key or a
    /// variable is unknown, or a value does not read as its key.
    pub fn load(path: Option<&Path>, sets: &[(String, String)]) -> Result<Loaded<Self>, Error> {
        loader()
            .and_then(|loader| loader.load_with(path, sets))
            .map_err(|source| Error::Load { source })
    }

    /// Reads `text` as TOML and applies `environment` over it.
    ///
    /// Reading the sources and reading the process environment are separate
    /// so a test can state both: setting an environment variable is `unsafe`
    /// in edition 2024 and this workspace forbids `unsafe`.
    ///
    /// # Errors
    /// Returns [`Error::Load`] for TOML that does not read as this tree and
    /// for an override that names no key.
    pub fn from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        loader()
            .and_then(|loader| loader.load_sources::<Self>(text, None, environment))
            .map(|loaded| loaded.into_parts().0)
            .map_err(|source| Error::Load { source })
    }
}
