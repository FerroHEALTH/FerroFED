// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The category map of the access log, `[access_log]`.
//!
//! ```toml
//! [access_log]
//! national_categories = ["nl-example"]
//!
//! [access_log.templates]
//! "Example Lab Report.v1" = ["medical-test-result"]
//! "Example Admin Note.v1" = "none"
//!
//! [access_log.archetypes]
//! "openEHR-EHR-OBSERVATION.laboratory_test_result.v1" = ["medical-test-result"]
//! ```
//!
//! Each template id and archetype id maps to the Art 14(1) categories the
//! data under it belong to (Regulation (EU) 2025/327), by the codes
//! `ehds_logging` names, or to a national category the deployment declares,
//! or to `none`. FerroFED ships no map, so every access is unclassified
//! until the operator writes one. The map's digest, a SHA-256 of its
//! canonical text, is named in every record classified under it. No
//! specification governs the table: our own design.

use std::collections::BTreeMap;

use ehds_logging::map::{CategoryMap, Declared};
use serde::Deserialize;

use crate::config::error::Error;

/// `[access_log]`, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccessLog {
    /// The codes of the categories national law adds (Art 14(1) third
    /// subparagraph).
    pub national_categories: Vec<String>,
    /// The categories of the data under each template id.
    pub templates: BTreeMap<String, Declared>,
    /// The categories of the data under each archetype id.
    pub archetypes: BTreeMap<String, Declared>,
}

/// Resolves `[access_log]` into the category map, named by its digest.
///
/// # Errors
///
/// [`Error::AccessLogMap`] for a map the logging component refuses.
pub(crate) fn resolve(table: &AccessLog) -> Result<CategoryMap, Error> {
    let map = CategoryMap::declare(
        &table.national_categories,
        &table.templates,
        &table.archetypes,
    )
    .map_err(Error::AccessLogMap)?;
    let digest = crate::conformance::fixture::sha256(map.canonical().as_bytes());
    Ok(map.with_digest(format!("sha256:{digest}")))
}
