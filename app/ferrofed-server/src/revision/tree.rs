// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration tree as a revision holds it.
//!
//! The bootstrap core is split off, every secret is held only as a
//! reference, and two revisions differ by dotted key. No specification
//! governs this: our own design.

use std::collections::BTreeMap;

use ferrofed_registry::resource::SECRETS;
use ferrofed_registry::revision::{Change, Difference};

/// The top-level keys of the bootstrap core, which only the file sets and a
/// revision never holds.
///
/// They are the profile, the client listener (`[server]`: its address, TLS,
/// base path and limits), the admin listener and its scrape authentication
/// (`[metrics]`), client authentication with the product scope (`[auth]`)
/// and its policy (`[trust]`), and the revision store itself (`[config]`).
/// The profile decides the transport policy every outbound URL is held to.
pub const CORE: [&str; 6] = ["profile", "server", "metrics", "auth", "trust", "config"];

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
/// of [`SECRETS`] anywhere, a key of `SECRET_URLS`, and any string that is
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
        assert_eq!(console_keys(&file), Vec::<String>::new());
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
            "[credentials.\"node-a\"]\npassword = \"synthetic-secret-1\"\n\
             [credentials.\"node-b\"]\npassword_file = \"/run/secrets/b\"\n\
             [stored_queries]\nbackend = \"postgres\"\nurl = \"host=db\"\n\
             [telemetry]\notlp_endpoint = \"http://u:synthetic-pw@collector:4317\"\n",
        );
        assert_eq!(
            vec![
                "credentials.node-a.password",
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

    /// Every configuration field of `text` that holds a secret of type
    /// `kind` and has a `_file` sibling the gateway reads it from.
    fn read_through_file<'a>(text: &'a str, kind: &str) -> Vec<&'a str> {
        let held = format!(": Option<{kind}>,");
        text.lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("pub ")?
                    .strip_suffix(held.as_str())
            })
            .filter(|name| text.contains(&format!("pub {name}_file: Option<PathBuf>,")))
            .collect()
    }

    /// Every `.rs` file under `dir`, with its path and its text.
    fn sources(dir: &std::path::Path, found: &mut Vec<(std::path::PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                found.push((path, text));
            }
        }
    }

    // A secret the gateway reads inline or through its `_file` sibling is
    // one the revision refuses inline and the console never shows, so a
    // configuration field of that shape missing from the list fails here.
    #[test]
    fn every_secret_the_gateway_reads_through_a_file_is_listed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files);
        let mut seen = std::collections::BTreeSet::new();
        for (path, text) in &files {
            for name in read_through_file(text, "Secret") {
                assert!(
                    super::SECRETS.contains(&name),
                    "{name} of {} is read through {name}_file and missing from SECRETS",
                    path.display()
                );
                seen.insert(name);
            }
            for name in read_through_file(text, "SecretUrl") {
                let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
                let listed = match relative.as_ref() {
                    "config/stored_queries.rs" => {
                        super::SECRET_URLS.contains(&format!("stored_queries.{name}").as_str())
                    }
                    "config/revisions.rs" => super::CORE.contains(&"config"),
                    _ => false,
                };
                assert!(
                    listed,
                    "{name} of {relative} is a secret URL read through {name}_file, in no list"
                );
            }
        }
        let unseen: Vec<&&str> = super::SECRETS
            .iter()
            .filter(|name| !seen.contains(**name))
            .collect();
        assert!(unseen.is_empty(), "listed but read nowhere: {unseen:?}");
    }
}
