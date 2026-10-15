// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration tree as a revision holds it.
//!
//! The bootstrap core is split off, every secret is held only as a
//! reference, and two revisions differ by dotted key. No specification
//! governs this: our own design.

use std::collections::BTreeMap;

use serde::Serialize;

/// The top-level keys of the bootstrap core, which only the file sets and a
/// revision never holds.
///
/// They are the profile, the client listener (`[server]`: its address, TLS,
/// base path and limits), the admin listener and its scrape authentication
/// (`[metrics]`), client authentication with the operator scope (`[auth]`),
/// the revision store itself (`[config]`), and the development
/// cross-reference (`[dev]`), which pairs patient identifiers with `ehr_id`s
/// and is never written to a store (§5.4.1, N33). The profile decides the
/// transport policy every outbound URL is held to.
pub const CORE: [&str; 6] = ["profile", "server", "metrics", "auth", "config", "dev"];

/// The keys of the resources the operator console owns, each a table and
/// the key inside it: only a revision holds them, and a file load carries
/// them over from the running revision ([`with_console_of`]).
pub const CONSOLE: [(&str, &str); 1] = [("registry", "members")];

/// The dotted keys of [`CONSOLE`] that `tree` sets, in [`CONSOLE`] order.
#[must_use]
pub fn console_keys(tree: &toml::Table) -> Vec<String> {
    CONSOLE
        .iter()
        .filter(|(table, key)| {
            tree.get(*table)
                .and_then(toml::Value::as_table)
                .is_some_and(|inner| inner.contains_key(*key))
        })
        .map(|(table, key)| format!("{table}.{key}"))
        .collect()
}

/// Returns `tree` without the keys of [`CONSOLE`]; a table left empty by
/// their removal is removed too.
#[must_use]
pub fn without_console(tree: &toml::Table) -> toml::Table {
    let mut tree = tree.clone();
    for (table, key) in CONSOLE {
        if let Some(toml::Value::Table(inner)) = tree.get_mut(table) {
            inner.remove(key);
            if inner.is_empty() {
                tree.remove(table);
            }
        }
    }
    tree
}

/// Returns `tree` with the keys of [`CONSOLE`] as `from` sets them, and
/// without those `from` does not set.
#[must_use]
pub fn with_console_of(tree: &toml::Table, from: &toml::Table) -> toml::Table {
    let mut tree = without_console(tree);
    for (table, key) in CONSOLE {
        let value = from
            .get(table)
            .and_then(toml::Value::as_table)
            .and_then(|inner| inner.get(key));
        if let Some(value) = value {
            let entry = tree
                .entry(table.to_owned())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if let toml::Value::Table(inner) = entry {
                inner.insert(key.to_owned(), value.clone());
            }
        }
    }
    tree
}

/// The keys whose value is a secret with a `_file` sibling, wherever they
/// sit: an inline value is refused, and the `_file` reference is kept.
const SECRETS: [&str; 7] = [
    "bearer_token",
    "password",
    "client_identity",
    "client_secret",
    "assertion",
    "feed_token",
    "scrape_token",
];

/// The dotted keys whose value is a secret connection string with a `_file`
/// sibling.
const SECRET_URLS: [&str; 1] = ["stored_queries.url"];

/// Splits `tree` into its bootstrap core ([`CORE`]) and the rest, which a
/// revision holds.
#[must_use]
pub fn split(tree: &toml::Table) -> (toml::Table, toml::Table) {
    tree.iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .partition(|(key, _)| CORE.contains(&key.as_str()))
}

/// Returns `core` with `revision` beside it, the whole tree a revision
/// configures; a key of `revision` in the core is left out.
#[must_use]
pub fn join(core: &toml::Table, revision: &toml::Table) -> toml::Table {
    let mut tree = core.clone();
    for (key, value) in revision {
        if !CORE.contains(&key.as_str()) {
            tree.insert(key.clone(), value.clone());
        }
    }
    tree
}

/// The top-level keys of `revision` that belong to the bootstrap core, in
/// key order.
#[must_use]
pub fn core_keys(revision: &toml::Table) -> Vec<String> {
    revision
        .keys()
        .filter(|key| CORE.contains(&key.as_str()))
        .cloned()
        .collect()
}

/// The dotted keys of `tree` that hold a secret inline, in key order: a key
/// of `SECRETS` anywhere, a key of `SECRET_URLS`, and any string that is
/// a URL carrying a password. Never a value.
#[must_use]
pub fn inline_secrets(tree: &toml::Table) -> Vec<String> {
    let mut found = Vec::new();
    secrets_in(tree, "", &mut found);
    found
}

/// Adds the keys of `table`, under `prefix`, that hold a secret inline to
/// `found`.
fn secrets_in(table: &toml::Table, prefix: &str, found: &mut Vec<String>) {
    for (key, value) in table {
        let dotted = dotted(prefix, key);
        match value {
            toml::Value::Table(inner) => secrets_in(inner, &dotted, found),
            toml::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    let indexed = format!("{dotted}[{index}]");
                    match item {
                        toml::Value::Table(inner) => secrets_in(inner, &indexed, found),
                        toml::Value::String(text) if carries_password(text) => {
                            found.push(indexed);
                        }
                        _ => {}
                    }
                }
            }
            toml::Value::String(text)
                if SECRETS.contains(&key.as_str())
                    || SECRET_URLS.contains(&dotted.as_str())
                    || carries_password(text) =>
            {
                found.push(dotted);
            }
            _ if SECRETS.contains(&key.as_str()) => found.push(dotted),
            _ => {}
        }
    }
}

/// Whether `text` is a URL that carries a password in its userinfo.
fn carries_password(text: &str) -> bool {
    // NOTE: no specification governs this: our own design. A parse failure IS
    // the answer: text that is no URL carries no URL password.
    url::Url::parse(text).is_ok_and(|url| url.password().is_some())
}

/// `key` under `prefix`, dotted.
fn dotted(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    }
}

/// How one key differs between two revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Change {
    /// The key is set only in the later revision.
    Added,
    /// The key is set only in the earlier revision.
    Removed,
    /// The key is set in both, to different values.
    Changed,
}

/// One key that differs between two revisions, with its values as TOML
/// writes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Difference {
    /// The dotted key, an array of tables indexed as `table[0]`.
    pub key: String,
    /// How the key differs.
    pub change: Change,
    /// The value in the earlier revision, when it sets one.
    pub from: Option<String>,
    /// The value in the later revision, when it sets one.
    pub to: Option<String>,
}

/// The keys that differ from `from` to `to`, in key order. A revision holds
/// no secret value ([`inline_secrets`]), so the values are shown.
#[must_use]
pub fn diff(from: &toml::Table, to: &toml::Table) -> Vec<Difference> {
    let (before, after) = (leaves(from), leaves(to));
    let keys: std::collections::BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    keys.into_iter()
        .filter_map(|key| {
            let (was, now) = (before.get(key), after.get(key));
            let change = match (was, now) {
                (Some(was), Some(now)) if was == now => return None,
                (Some(_), Some(_)) => Change::Changed,
                (None, Some(_)) => Change::Added,
                (Some(_), None) => Change::Removed,
                (None, None) => return None,
            };
            Some(Difference {
                key: key.clone(),
                change,
                from: was.cloned(),
                to: now.cloned(),
            })
        })
        .collect()
}

/// Every leaf of `table` by dotted key, rendered as TOML writes the value.
/// A table nests, an array of tables is indexed, and any other array is one
/// leaf.
fn leaves(table: &toml::Table) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    leaves_in(table, "", &mut found);
    found
}

/// Adds the leaves of `table`, under `prefix`, to `found`.
fn leaves_in(table: &toml::Table, prefix: &str, found: &mut BTreeMap<String, String>) {
    for (key, value) in table {
        let dotted = dotted(prefix, key);
        match value {
            toml::Value::Table(inner) if !inner.is_empty() => leaves_in(inner, &dotted, found),
            toml::Value::Array(items) if items.iter().all(toml::Value::is_table) => {
                for (index, item) in items.iter().enumerate() {
                    if let toml::Value::Table(inner) = item {
                        leaves_in(inner, &format!("{dotted}[{index}]"), found);
                    }
                }
            }
            other => {
                found.insert(dotted, other.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Change, console_keys, core_keys, diff, inline_secrets, join, split, with_console_of,
        without_console,
    };

    #[test]
    fn the_console_keys_come_off_and_carry_over_whole() {
        let file = table("[registry]\ndocument = \"/r.toml\"\n");
        let running = table(
            "[registry]\ndocument = \"/old.toml\"\n[[registry.members]]\nnode = { id = \"node-c\" }\n",
        );
        assert!(console_keys(&file).is_empty());
        assert_eq!(vec!["registry.members"], console_keys(&running));
        let carried = with_console_of(&file, &running);
        assert_eq!(vec!["registry.members"], console_keys(&carried));
        assert_eq!(Some("/r.toml"), carried["registry"]["document"].as_str());
        assert_eq!(file, without_console(&carried));
        assert_eq!(file, with_console_of(&carried, &file));
        let only = table("[[registry.members]]\nnode = { id = \"node-c\" }\n");
        assert!(without_console(&only).is_empty());
    }

    fn table(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn the_core_splits_off_and_joins_back() {
        let tree = table(
            "profile = \"development\"\n[server]\nlisten = \"127.0.0.1:1\"\n[federation]\nbest_effort = false\n",
        );
        let (core, revision) = split(&tree);
        assert_eq!(vec!["profile", "server"], core.keys().collect::<Vec<_>>());
        assert_eq!(vec!["federation"], revision.keys().collect::<Vec<_>>());
        assert_eq!(tree, join(&core, &revision));
        assert_eq!(vec!["profile", "server"], core_keys(&tree));
    }

    #[test]
    fn an_inline_secret_is_named_by_key_and_a_reference_is_not() {
        let tree = table(
            "[credentials.\"node-a\"]\nbearer_token = \"synthetic-secret-1\"\n\
             [credentials.\"node-b\"]\nbearer_token_file = \"/run/secrets/b\"\n\
             [stored_queries]\nbackend = \"postgres\"\nurl = \"host=db\"\n\
             [telemetry]\notlp_endpoint = \"http://u:synthetic-pw@collector:4317\"\n",
        );
        assert_eq!(
            vec![
                "credentials.node-a.bearer_token",
                "stored_queries.url",
                "telemetry.otlp_endpoint"
            ],
            inline_secrets(&tree)
        );
    }

    #[test]
    fn the_diff_names_each_changed_leaf() {
        let from = table("[federation]\nbest_effort = true\nid = \"a\"\n[[x]]\ny = 1\n");
        let to = table("[federation]\nbest_effort = false\n[[x]]\ny = 2\n[fhir]\n");
        let changes = diff(&from, &to);
        let keys: Vec<(&str, Change)> = changes
            .iter()
            .map(|change| (change.key.as_str(), change.change))
            .collect();
        assert_eq!(
            vec![
                ("federation.best_effort", Change::Changed),
                ("federation.id", Change::Removed),
                ("fhir", Change::Added),
                ("x[0].y", Change::Changed),
            ],
            keys
        );
        assert_eq!(Some("true"), changes[0].from.as_deref());
        assert_eq!(Some("false"), changes[0].to.as_deref());
    }
}
