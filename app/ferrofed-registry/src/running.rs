// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's running configuration, as its admin listener answers it.
//!
//! The operator console reads it: each deployment-wide setting with the
//! value the gateway runs, where that value comes from, its default, and
//! whether a change of it waits for the next start.
//!
//! A secret is never a value here: [`SecretSetting`] says whether one is
//! set and, when it is read from a file, that file's path. The body of the
//! save of settings ([`SettingsRequest`]) carries each value as the text an
//! operator typed, read as TOML syntax or as the string it is, the way an
//! environment override is read. No specification governs this: our own
//! design.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::revision::RevisionId;

/// The running configuration: how it is managed, the revision this replica
/// runs, and every setting the document lists, in the order a page shows
/// them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunningConfig {
    /// `config.ui`: `off` or `managed`.
    pub ui: String,
    /// The revision this replica runs, under `managed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<RevisionId>,
    /// The settings.
    pub settings: Vec<Setting>,
}

impl RunningConfig {
    /// The settings of `section`, in document order.
    pub fn of(&self, section: Section) -> impl Iterator<Item = &Setting> {
        self.settings
            .iter()
            .filter(move |setting| setting.section == section)
    }

    /// The setting whose dotted key is `key`.
    #[must_use]
    pub fn setting(&self, key: &str) -> Option<&Setting> {
        self.settings.iter().find(|setting| setting.key == key)
    }
}

/// The part of the deployment a setting belongs to, one tab of the
/// console's configuration page each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Section {
    /// How a query fans out to the members and when it gives up.
    Federation,
    /// How a query is paged, and what is sent on to the members.
    FanOut,
    /// What callers learn about consent.
    Consent,
    /// The store the stored queries are kept in.
    StoredQueries,
    /// How each access record is classified and kept.
    AccessLog,
    /// The FHIR face of the European exchange format.
    Fhir,
}

impl Section {
    /// Every section, in the order a page shows them.
    pub const ALL: [Self; 6] = [
        Self::Federation,
        Self::FanOut,
        Self::Consent,
        Self::StoredQueries,
        Self::AccessLog,
        Self::Fhir,
    ];

    /// The section as the document and a route name it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Federation => "federation",
            Self::FanOut => "fan-out",
            Self::Consent => "consent",
            Self::StoredQueries => "stored-queries",
            Self::AccessLog => "access-log",
            Self::Fhir => "fhir",
        }
    }

    /// The section named `name`, as [`Section::as_str`] writes it.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|section| section.as_str() == name)
    }
}

/// Where the value a setting runs with comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Source {
    /// The configuration file.
    File,
    /// A `FERROFED__` environment override.
    Environment,
    /// The configuration revision this replica runs, under `managed`.
    Revision,
    /// Nothing sets it: the gateway's default.
    Default,
}

/// One setting of the running configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setting {
    /// The dotted key, such as `federation.per_node_timeout_ms`.
    pub key: String,
    /// The section it belongs to.
    pub section: Section,
    /// The value the gateway runs with, as the configuration writes it;
    /// absent for a secret and for a setting nothing sets and that has no
    /// default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<toml::Value>,
    /// Where [`Setting::value`] comes from.
    pub source: Source,
    /// The value the gateway takes when nothing sets it, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<toml::Value>,
    /// Whether a change takes effect only at the next start of the replica.
    pub restart: bool,
    /// For a setting a change of which waits for the next start, the value
    /// that start takes when it differs from the running one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<toml::Value>,
    /// For a secret, whether it is set and where it is read from; never its
    /// value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretSetting>,
    /// Whether a save of settings ([`SettingsRequest`]) may change it.
    pub editable: bool,
}

/// Whether a secret setting is set, and the file it is read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretSetting {
    /// Whether the secret is set, inline or by a file.
    pub set: bool,
    /// The file the secret is read from, its `_file` reference, when it is
    /// read from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// The body of the save of settings: a revision on `base` with each
/// setting of `settings` changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsRequest {
    /// The revision the save is based on, which must be the newest.
    pub base: Option<RevisionId>,
    /// Why the change is made.
    pub reason: String,
    /// Each changed setting by its dotted key, its value as an operator
    /// typed it: TOML syntax, such as `3000` or `true`, or the string it is.
    pub settings: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::{RunningConfig, SecretSetting, Section, Setting, Source};

    #[test]
    fn a_section_reads_back_from_its_name() {
        for section in Section::ALL {
            assert_eq!(Some(section), Section::named(section.as_str()));
        }
        assert_eq!(None, Section::named("members"));
    }

    #[test]
    fn the_document_reads_back_from_the_json_it_is_written_as() {
        let document = RunningConfig {
            ui: String::from("off"),
            revision: None,
            settings: vec![
                Setting {
                    key: String::from("federation.per_node_timeout_ms"),
                    section: Section::Federation,
                    value: Some(toml::Value::Integer(2500)),
                    source: Source::File,
                    default: Some(toml::Value::Integer(10_000)),
                    restart: true,
                    next: Some(toml::Value::Integer(3000)),
                    secret: None,
                    editable: true,
                },
                Setting {
                    key: String::from("stored_queries.url"),
                    section: Section::StoredQueries,
                    value: None,
                    source: Source::File,
                    default: None,
                    restart: true,
                    next: None,
                    secret: Some(SecretSetting {
                        set: true,
                        reference: Some(String::from("/run/secrets/stored-queries-url")),
                    }),
                    editable: false,
                },
            ],
        };
        let text = serde_json::to_string(&document).expect("the document writes");
        let read: RunningConfig = serde_json::from_str(&text).expect("the document reads");
        assert_eq!(document, read);
        assert_eq!(1, read.of(Section::StoredQueries).count());
        assert!(read.setting("federation.per_node_timeout_ms").is_some());
    }
}
