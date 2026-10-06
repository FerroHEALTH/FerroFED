// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The category map, the retention and the emergency purposes of the access
//! log, `[access_log]`.
//!
//! ```toml
//! [access_log]
//! national_categories = ["https://example.org/fhir/CodeSystem/national-category|nl-example"]
//! patient_namespaces = ["urn:oid:2.999.1"]
//!
//! [access_log.templates]
//! "Example Lab Report.v1" = ["Laboratory-Reports"]
//! "Example Admin Note.v1" = "none"
//!
//! [access_log.archetypes]
//! "openEHR-EHR-OBSERVATION.laboratory_test_result.v1" = ["Laboratory-Reports"]
//!
//! [access_log.retention]
//! years = 5
//!
//! [access_log.retention.categories]
//! "Laboratory-Reports" = 20
//!
//! [access_log.retention.origins]
//! "node-a" = 15
//!
//! [[access_log.emergency_purpose]]
//! system = "http://terminology.hl7.org/CodeSystem/v3-ActReason"
//! code = "BTG"
//! ```
//!
//! Each template id and archetype id maps to the Art 14(1) categories the
//! data under it belong to (Regulation (EU) 2025/327), by the codes of HL7
//! Europe's `EEHRxFDocumentPriorityCategoryCS` that `ehds_logging` writes,
//! bare or as `<system>|<code>`, or to a national category the deployment
//! declares as `<system>|<code>`, or to `none`. FerroFED ships no map, so every access is unclassified
//! until the operator writes one. The map's digest, a SHA-256 of its
//! canonical text, is named in every record classified under it. No
//! specification governs the table: our own design.
//!
//! `[access_log.retention]` keeps every record `years`, the records of each
//! category and each origin (an endpoint of the registry) as long as their
//! tables say, each at least three years from the date of access (Art 9(2),
//! Annex II 3.4). Each record states its period for the Audit Record
//! Repository that holds it.
//!
//! Each `[[access_log.emergency_purpose]]` is a purpose of use, a code and
//! the system that defines it, that asserts an access in the vital interests
//! of the data subject (Art 11(5)). A record whose verified token declares
//! one is marked an emergency access. FerroFED declares none, so no access
//! is marked until the operator names the codes its issuers use (no
//! specification says which codes assert it: our own design).
//! `patient_namespaces` names the namespaces the identity binding is asked
//! to name the patient behind an `ehr_id` in, when a request named none, so
//! a search of the log by the patient's identifier finds the access (Art
//! 9(1)).

use std::collections::BTreeMap;

use ehds_logging::emergency::EmergencyPurposes;
use ehds_logging::map::{CategoryMap, Declared};
use ehds_logging::record::Purpose;
use ehds_logging::retention::{FLOOR_YEARS, RetentionPolicy};
use ferrofed_identity::role::patient::IdentifierNamespace;
use serde::Deserialize;

use crate::config::error::Error;

/// `[access_log]`, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccessLog {
    /// The categories national law adds (Art 14(1) third subparagraph),
    /// each `<system>|<code>`.
    pub national_categories: Vec<String>,
    /// The categories of the data under each template id.
    pub templates: BTreeMap<String, Declared>,
    /// The categories of the data under each archetype id.
    pub archetypes: BTreeMap<String, Declared>,
    /// How long each record is kept.
    pub retention: Retention,
    /// The purposes of use that mark an access an emergency access.
    pub emergency_purpose: Vec<EmergencyPurpose>,
    /// The namespaces the patient behind an `ehr_id` is named in, when the
    /// request named no patient: those an electronic health data access
    /// service searches the log by (Art 9(1), (2)).
    pub patient_namespaces: Vec<String>,
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

/// One `[[access_log.emergency_purpose]]`, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencyPurpose {
    /// The code system, such as the HL7 v3 `ActReason` system; absent, the
    /// entry matches only a purpose a token declares with no system.
    pub system: Option<String>,
    /// The code, such as `BTG`.
    pub code: String,
}

/// `[access_log]`, resolved: the category map, named by its digest, the
/// retention policy and the emergency purposes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessLogSettings {
    /// The category map every record is classified with.
    pub map: CategoryMap,
    /// How long every record is kept.
    pub retention: RetentionPolicy,
    /// The purposes of use that mark an access an emergency access.
    pub emergency: EmergencyPurposes,
    /// The namespaces the patient behind an `ehr_id` is named in.
    pub patient_namespaces: Vec<IdentifierNamespace>,
}

impl AccessLogSettings {
    /// What `config check` notes about the access log: that it declares no
    /// emergency purpose, so no access is marked as one (Regulation (EU)
    /// 2025/327 Art 11(5)).
    #[must_use]
    pub fn notes(&self) -> Vec<String> {
        if self.emergency.is_empty() {
            vec![String::from(
                "[access_log] declares no [[access_log.emergency_purpose]]: no access is marked as an emergency access (Regulation (EU) 2025/327 Art 11(5))",
            )]
        } else {
            Vec::new()
        }
    }
}

/// Resolves `[access_log]` into the category map, named by its digest, the
/// retention policy and the emergency purposes.
///
/// # Errors
///
/// [`Error::AccessLogMap`] for a map the logging component refuses,
/// [`Error::AccessLogRetention`] for a retention it refuses, and
/// [`Error::AccessLogEmergency`] for an emergency purpose it refuses, and
/// [`Error::AccessLogNamespace`] for an empty patient namespace.
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
    let purposes: Vec<Purpose> = table
        .emergency_purpose
        .iter()
        .map(|purpose| Purpose {
            system: purpose.system.clone(),
            code: purpose.code.clone(),
        })
        .collect();
    let emergency = EmergencyPurposes::declare(&purposes).map_err(Error::AccessLogEmergency)?;
    let patient_namespaces = table
        .patient_namespaces
        .iter()
        .map(|namespace| IdentifierNamespace::new(namespace.as_str()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(Error::AccessLogNamespace)?;
    Ok(AccessLogSettings {
        map,
        retention,
        emergency,
        patient_namespaces,
    })
}
