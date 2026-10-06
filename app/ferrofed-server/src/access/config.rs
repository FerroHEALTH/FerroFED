// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The category map and the retention of the access log, `[access_log]`.
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
//!
//! [access_log.retention]
//! years = 5
//!
//! [access_log.retention.categories]
//! "medical-test-result" = 20
//!
//! [access_log.retention.origins]
//! "node-a" = 15
//! ```
//!
//! Each template id and archetype id maps to the Art 14(1) categories the
//! data under it belong to (Regulation (EU) 2025/327), by the codes
//! `ehds_logging` names, or to a national category the deployment declares,
//! or to `none`. FerroFED ships no map, so every access is unclassified
//! until the operator writes one. The map's digest, a SHA-256 of its
//! canonical text, is named in every record classified under it. No
//! specification governs the table: our own design.
//!
//! `[access_log.retention]` keeps every record `years`, the records of each
//! category and each origin (an endpoint of the registry) as long as their
//! tables say, each at least three years from the date of access (Art 9(2),
//! Annex II 3.4). Each record states its period for the Audit Record
//! Repository that holds it.

use std::collections::BTreeMap;

use ehds_logging::map::{CategoryMap, Declared};
use ehds_logging::retention::{FLOOR_YEARS, RetentionPolicy};
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
    /// How long each record is kept.
    pub retention: Retention,
}

/// `[access_log.retention]`, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Retention {
    /// The years every record is kept, at least three.
    pub years: u16,
    /// The years the records of each category are kept, by its code.
    pub categories: BTreeMap<String, u16>,
    /// The years the records of each origin are kept, by its endpoint id.
    pub origins: BTreeMap<String, u16>,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            years: FLOOR_YEARS,
            categories: BTreeMap::new(),
            origins: BTreeMap::new(),
        }
    }
}

/// `[access_log]`, resolved: the category map, named by its digest, and the
/// retention policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessLogSettings {
    /// The category map every record is classified with.
    pub map: CategoryMap,
    /// How long every record is kept.
    pub retention: RetentionPolicy,
}

/// Resolves `[access_log]` into the category map, named by its digest, and
/// the retention policy.
///
/// # Errors
///
/// [`Error::AccessLogMap`] for a map the logging component refuses, and
/// [`Error::AccessLogRetention`] for a retention it refuses.
pub(crate) fn resolve(table: &AccessLog) -> Result<AccessLogSettings, Error> {
    let map = CategoryMap::declare(
        &table.national_categories,
        &table.templates,
        &table.archetypes,
    )
    .map_err(Error::AccessLogMap)?;
    let digest = crate::conformance::fixture::sha256(map.canonical().as_bytes());
    let map = map.with_digest(format!("sha256:{digest}"));
    let retention = RetentionPolicy::declare(
        table.retention.years,
        &table.retention.categories,
        &table.retention.origins,
        &map,
    )
    .map_err(Error::AccessLogRetention)?;
    Ok(AccessLogSettings { map, retention })
}
