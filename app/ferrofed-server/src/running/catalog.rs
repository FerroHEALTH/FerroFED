// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings the running configuration lists, each with its section,
//! its default, the restart key that holds it to the next start, and
//! whether a save of settings may change it.
//!
//! A default is written as the configuration spells it, and the tests hold
//! each one to the value the configuration takes when the key is unset; a
//! restart key is one the reload names as waiting for the next start
//! ([`crate::reload::restart_keys`]), and the tests hold every setting
//! without one to being outside each of them. No specification governs
//! this: our own design.

use ferrofed_registry::running::Section;

/// What a setting holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// One value an operator types: a number, a boolean or a word.
    Scalar,
    /// A list or a table, changed by an import of the whole configuration.
    Whole,
    /// A secret: never a value, and its `_file` sibling the reference.
    Secret {
        /// The dotted key of the file it is read from.
        file: &'static str,
    },
}

/// One setting the running configuration lists.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry {
    /// The dotted key.
    pub(crate) key: &'static str,
    /// The section it belongs to.
    pub(crate) section: Section,
    /// The value it takes when unset, as TOML syntax.
    pub(crate) default: Option<&'static str>,
    /// The key a reload names when a change of it waits for the next start.
    pub(crate) restart: Option<&'static str>,
    /// What it holds.
    pub(crate) kind: Kind,
}

impl Entry {
    /// Whether a save of settings may change it.
    pub(crate) fn editable(&self) -> bool {
        self.kind == Kind::Scalar
    }
}

/// A setting of `section` that holds one value.
const fn scalar(
    key: &'static str,
    section: Section,
    default: Option<&'static str>,
    restart: Option<&'static str>,
) -> Entry {
    Entry {
        key,
        section,
        default,
        restart,
        kind: Kind::Scalar,
    }
}

/// A setting of `section` that holds a list or a table.
const fn whole(
    key: &'static str,
    section: Section,
    default: Option<&'static str>,
    restart: Option<&'static str>,
) -> Entry {
    Entry {
        key,
        section,
        default,
        restart,
        kind: Kind::Whole,
    }
}

/// The restart key of the two fan-out budgets.
const TIMEOUTS: Option<&str> = Some("federation.timeouts");

/// The restart key of the paging strategy and its window.
const OFFSET: Option<&str> = Some("federation.offset");

/// The restart key of the stored-query store.
const STORED: Option<&str> = Some("stored_queries");

/// The restart key of the FHIR face.
const FHIR: Option<&str> = Some("fhir");

/// Every setting the running configuration lists, in the order a page
/// shows them.
pub(crate) const ENTRIES: &[Entry] = &[
    scalar(
        "federation.per_node_timeout_ms",
        Section::Federation,
        Some("10000"),
        TIMEOUTS,
    ),
    scalar(
        "federation.overall_timeout_ms",
        Section::Federation,
        Some("25000"),
        TIMEOUTS,
    ),
    scalar(
        "federation.node_selection",
        Section::Federation,
        None,
        Some("federation.node_selection"),
    ),
    scalar(
        "federation.best_effort",
        Section::Federation,
        Some("true"),
        Some("federation.best_effort"),
    ),
    scalar(
        "federation.localization.on_failure",
        Section::Federation,
        Some("\"closed\""),
        None,
    ),
    scalar(
        "federation.localization.timeout_ms",
        Section::Federation,
        Some("5000"),
        None,
    ),
    scalar(
        "federation.fan_out_stored_queries",
        Section::FanOut,
        Some("false"),
        Some("federation.fan_out_stored_queries"),
    ),
    scalar(
        "federation.fan_out_template_upload",
        Section::FanOut,
        Some("false"),
        Some("federation.fan_out_template_upload"),
    ),
    scalar(
        "federation.offset_strategy",
        Section::FanOut,
        Some("\"bounded\""),
        OFFSET,
    ),
    scalar(
        "federation.max_offset_window",
        Section::FanOut,
        Some("1000"),
        OFFSET,
    ),
    scalar(
        "federation.max_node_rows",
        Section::FanOut,
        Some("9999"),
        Some("federation.max_node_rows"),
    ),
    scalar(
        "federation.consent.disclose",
        Section::Consent,
        Some("true"),
        None,
    ),
    scalar(
        "federation.consent.emergency",
        Section::Consent,
        Some("\"apply\""),
        None,
    ),
    scalar(
        "stored_queries.backend",
        Section::StoredQueries,
        None,
        STORED,
    ),
    scalar("stored_queries.path", Section::StoredQueries, None, STORED),
    Entry {
        key: "stored_queries.url",
        section: Section::StoredQueries,
        default: None,
        restart: STORED,
        kind: Kind::Secret {
            file: "stored_queries.url_file",
        },
    },
    scalar(
        "access_log.retention.years",
        Section::AccessLog,
        Some("3"),
        None,
    ),
    whole(
        "access_log.emergency_purpose",
        Section::AccessLog,
        Some("[]"),
        None,
    ),
    whole("access_log.templates", Section::AccessLog, Some("{}"), None),
    whole(
        "access_log.archetypes",
        Section::AccessLog,
        Some("{}"),
        None,
    ),
    scalar("fhir.base", Section::Fhir, None, FHIR),
    scalar(
        "fhir.document_url_lifetime_secs",
        Section::Fhir,
        Some("300"),
        FHIR,
    ),
    whole("fhir.receive", Section::Fhir, Some("[]"), FHIR),
];

/// The setting whose dotted key is `key`.
pub(crate) fn entry(key: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.key == key)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{ENTRIES, Kind};
    use crate::config::Config;

    /// `tree` with an empty table at every parent of the dotted `key`, and
    /// `value` at the key itself when given.
    fn with(mut tree: toml::Table, key: &str, value: Option<toml::Value>) -> toml::Table {
        let mut cursor = &mut tree;
        let segments: Vec<&str> = key.split('.').collect();
        let (last, parents) = segments.split_last().expect("a dotted key");
        for parent in parents {
            let entry = cursor
                .entry((*parent).to_owned())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            cursor = entry.as_table_mut().expect("a parent table");
        }
        if let Some(value) = value {
            cursor.insert((*last).to_owned(), value);
        }
        tree
    }

    /// The configuration `tree` writes.
    fn config(tree: &toml::Table) -> Config {
        let text = toml::to_string(tree).expect("the tree writes");
        Config::from_sources(Some(&text), &BTreeMap::new()).expect("the configuration reads")
    }

    #[test]
    fn each_default_is_the_value_the_configuration_takes_when_the_key_is_unset() {
        for entry in ENTRIES {
            let Some(default) = entry.default else {
                continue;
            };
            let value = crate::config::load::env_value(default);
            let unset = config(&with(toml::Table::new(), entry.key, None));
            let set = config(&with(toml::Table::new(), entry.key, Some(value)));
            assert_eq!(unset, set, "{}", entry.key);
        }
    }

    #[test]
    fn each_restart_key_is_one_a_reload_names_and_every_other_setting_reloads() {
        let restart = crate::reload::restart_keys();
        for entry in ENTRIES {
            if let Some(key) = entry.restart {
                assert!(restart.contains(&key), "{}: {key}", entry.key);
            } else {
                let held = restart
                    .iter()
                    .find(|key| entry.key == **key || entry.key.starts_with(&format!("{key}.")));
                assert_eq!(None, held, "{}", entry.key);
            }
        }
    }

    #[test]
    fn a_secret_is_never_editable_and_names_the_secret_keys_of_the_revisions() {
        for entry in ENTRIES {
            if let Kind::Secret { file } = entry.kind {
                assert!(!entry.editable(), "{}", entry.key);
                assert_eq!(format!("{}_file", entry.key), file);
            }
        }
    }

    #[test]
    fn every_key_is_listed_once() {
        let mut keys: Vec<&str> = ENTRIES.iter().map(|entry| entry.key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len());
    }
}
