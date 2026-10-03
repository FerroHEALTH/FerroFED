// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[stored_queries]` table: where the federated stored-query registry
//! keeps its definitions (§12.7, N44).
//!
//! Setting `path` offers the registry: the gateway holds each definition in
//! the embedded store file it names, which survives a restart, and declares
//! the registry in `OPTIONS {base}/` (§7a.2). Without it, no registry is
//! offered. No specification governs the store or its configuration: our
//! own design.

use std::path::PathBuf;

use serde::Deserialize;

use crate::config::Config;
use crate::config::error::Error;

/// The federated stored-query registry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StoredQueries {
    /// The store file the definitions are kept in, created when it does not
    /// exist. One gateway process opens it at a time.
    pub path: Option<PathBuf>,
}

/// Resolves `[stored_queries]` of `config`: the store file, when the registry
/// is offered.
///
/// # Errors
///
/// [`Error::Missing`] naming `stored_queries.path` when it is set empty, and
/// naming `registry.document` when the registry is offered without the
/// federation that executes its queries.
pub fn resolve(config: &Config) -> Result<Option<PathBuf>, Error> {
    let Some(path) = &config.stored_queries.path else {
        return Ok(None);
    };
    if path.as_os_str().is_empty() {
        return Err(Error::Missing {
            key: String::from("stored_queries.path"),
        });
    }
    if config.registry.document.is_none() {
        return Err(Error::Missing {
            key: String::from("registry.document"),
        });
    }
    Ok(Some(path.clone()))
}
