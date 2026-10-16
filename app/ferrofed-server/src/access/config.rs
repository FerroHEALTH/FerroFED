// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The category map, the retention and the emergency purposes of the access
//! log, `[access_log]`.
//!
//! ```toml
//! [access_log]
//! national_categories = ["https://example.org/fhir/CodeSystem/national-category|nl-example"]
//! patient_namespaces = ["urn:oid:2.999.1"]
//! max_named_ehrs = 50
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
//! [access_log.labels]
//! origin_system = "https://example.org/fhir/CodeSystem/access-log-origin"
//!
//! [access_log.labels.categories]
//! "Laboratory-Reports" = [{ system = "https://example.org/fhir/CodeSystem/access-class", code = "lab" }]
//!
//! [access_log.labels.origins]
//! "node-a" = [{ system = "https://example.org/fhir/CodeSystem/access-class", code = "hospital-a" }]
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
//! `[access_log.labels]` gives each record a security label per category of
//! the data it reached and per origin of the data (Annex II 3.4): a category
//! carries the labels its table names or its own code, an origin the labels
//! its table names or its endpoint id as a code in `origin_system`, and an
//! unclassified access the labels of every category. The Audit Record
//! Repository limits who reads a record by them (no specification governs
//! the labels: our own design).
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
//! `max_named_ehrs`, 50 by default, is the most `ehr_id`s the record of a
//! query over many patients' data names from the `ehr_id` column its rows
//! deliver; above it, or at `0`, the record states the query's scope (Art
//! 9(1); no specification governs the bound: our own design).

use std::collections::BTreeMap;

use ehds_logging::emergency::EmergencyPurposes;
use ehds_logging::label::{DeclaredLabel, LabelPolicy};
use ehds_logging::map::{CategoryMap, Declared};
use ehds_logging::record::Purpose;
use ehds_logging::retention::{FLOOR_YEARS, RetentionPolicy};
use ferrofed_identity::role::patient::IdentifierNamespace;
use serde::Deserialize;

use crate::config::error::Error;

/// `[access_log]`, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
    /// The security labels of each record.
    pub labels: Labels,
    /// The purposes of use that mark an access an emergency access.
    pub emergency_purpose: Vec<EmergencyPurpose>,
    /// The namespaces the patient behind an `ehr_id` is named in, when the
    /// request named no patient: those an electronic health data access
    /// service searches the log by (Art 9(1), (2)).
    pub patient_namespaces: Vec<String>,
    /// The most `ehr_id`s the record of a query over many patients' data
    /// names, read from the delivered rows; above it, and at `0`, the record
    /// states the query's scope instead (Art 9(1)).
    pub max_named_ehrs: usize,
}

impl Default for AccessLog {
    fn default() -> Self {
        Self {
            national_categories: Vec::new(),
            templates: BTreeMap::new(),
            archetypes: BTreeMap::new(),
            retention: Retention::default(),
            labels: Labels::default(),
            emergency_purpose: Vec::new(),
            patient_namespaces: Vec::new(),
            max_named_ehrs: 50,
        }
    }
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

/// The FHIR R4 `CodeSystem` of the default [`Labels::origin_system`], as
/// JSON, which the documentation site serves at the system's URL.
///
/// Its codes are a deployment's endpoint ids, so its `content` is
/// `not-present` and it lists none.
pub const ORIGIN_SYSTEM_DEFINITION: &str = include_str!("CodeSystem-access-log-origin.json");

/// `[access_log.labels]`, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Labels {
    /// The code system an origin with no labels of its own is labelled in,
    /// its endpoint id the code: FerroFED's
    /// `https://ferrofed.eu/fhir/CodeSystem/access-log-origin` unless set.
    pub origin_system: String,
    /// The labels of each category, by its code.
    pub categories: BTreeMap<String, Vec<DeclaredLabel>>,
    /// The labels of each origin, by its endpoint id.
    pub origins: BTreeMap<String, Vec<DeclaredLabel>>,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            // NOTE: no specification governs an origin label: our own design, a code system
            // FerroFED owns whose codes are the deployment's endpoint ids.
            origin_system: "https://ferrofed.eu/fhir/CodeSystem/access-log-origin".to_owned(),
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
/// retention policy, the emergency purposes, the patient namespaces and the
/// most `ehr_id`s a population query's record names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessLogSettings {
    /// The category map every record is classified with.
    pub map: CategoryMap,
    /// How long every record is kept.
    pub retention: RetentionPolicy,
    /// The security labels of every record.
    pub labels: LabelPolicy,
    /// The purposes of use that mark an access an emergency access.
    pub emergency: EmergencyPurposes,
    /// The namespaces the patient behind an `ehr_id` is named in.
    pub patient_namespaces: Vec<IdentifierNamespace>,
    /// The most `ehr_id`s the record of a query over many patients' data
    /// names.
    pub max_named_ehrs: usize,
}

impl Default for AccessLogSettings {
    fn default() -> Self {
        Self {
            map: CategoryMap::default(),
            retention: RetentionPolicy::default(),
            labels: LabelPolicy::default(),
            emergency: EmergencyPurposes::default(),
            patient_namespaces: Vec::new(),
            max_named_ehrs: AccessLog::default().max_named_ehrs,
        }
    }
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
/// [`Error::AccessLogRetention`] for a retention it refuses,
/// [`Error::AccessLogLabels`] for labels it refuses,
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
    let labels = LabelPolicy::declare(
        Some(table.labels.origin_system.as_str()),
        &table.labels.categories,
        &table.labels.origins,
        &map,
    )
    .map_err(Error::AccessLogLabels)?;
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
        labels,
        emergency,
        patient_namespaces,
        max_named_ehrs: table.max_named_ehrs,
    })
}
