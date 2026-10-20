// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials and binding sections of a revision's configuration tree,
//! each read and written whole, and their secrets, which a revision holds
//! only as references to files.
//!
//! A secret of a resource is a key of [`SECRETS`] at any depth of it; a
//! revision holds its `_file` sibling alone. The console's settings of a
//! resource never name one: each secret is set by a value the gateway
//! writes to a new file in `config.secrets_dir`, by a reference, or cleared,
//! and a secret the change does not name keeps its reference. No
//! specification governs this: our own design.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use ferrofed_registry::resource::{BindingSection, Resource, SECRETS, SecretChange, SecretSlot};
use ferrofed_registry::secret::{SecretUrl, WriteOnly};

/// The suffix of a secret's reference to a file.
const FILE: &str = "_file";

/// Why the settings or the secrets of a resource are refused. It names
/// keys, never a value.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SectionFault {
    /// The settings are no TOML table.
    #[error("the settings are not TOML: the table's own keys, or one inline table")]
    Settings,
    /// The settings name a secret, which only a secret change sets.
    #[error("the settings name the secret {key}, which only a change of its secret sets")]
    SecretInSettings {
        /// The dotted key inside the resource.
        key: String,
    },
    /// A secret change names no secret the resource's settings hold a table
    /// for.
    #[error(
        "{key} is no secret of the resource: name a key of {} inside a table its settings hold",
        SECRETS.join(", ")
    )]
    Slot {
        /// The dotted key the change named.
        key: String,
    },
    /// A secret change gives other than exactly one of a value, a file and a
    /// clear.
    #[error("the change of {key} gives exactly one of a value, a file and a clear")]
    Change {
        /// The dotted key the change named.
        key: String,
    },
    /// A secret change names a file that is no absolute path.
    #[error("the file of {key} is no absolute path")]
    Reference {
        /// The dotted key the change named.
        key: String,
    },
    /// A secret value is sent while `config.secrets_dir` is unset.
    #[error(
        "the secret {key} is sent as a value, and config.secrets_dir is unset, so the gateway writes no secret file: set the secret by a reference to a file"
    )]
    NoSecretsDir {
        /// The dotted key the change named.
        key: String,
    },
    /// A secret value could not be written to its file.
    #[error("the secret {key} could not be written to a file in config.secrets_dir")]
    Write {
        /// The dotted key the change named.
        key: String,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
}

/// The value `tree` holds for `resource`.
#[must_use]
pub fn value_at<'a>(tree: &'a toml::Table, resource: &Resource) -> Option<&'a toml::Value> {
    let path = resource.path();
    let (last, parents) = path.split_last()?;
    let mut table = tree;
    for key in parents {
        table = table.get(*key)?.as_table()?;
    }
    table.get(*last)
}

/// Every credential and binding section `tree` sets, in resource order.
#[must_use]
pub fn present(tree: &toml::Table) -> Vec<Resource> {
    let credentials = tree
        .get("credentials")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|table| table.keys().cloned().map(Resource::Credentials));
    let bindings = BindingSection::ALL
        .into_iter()
        .map(Resource::Binding)
        .filter(|resource| value_at(tree, resource).is_some());
    credentials.chain(bindings).collect()
}

/// Sets `resource` in `tree` to `value`, making the tables above it.
pub fn set(tree: &mut toml::Table, resource: &Resource, value: toml::Value) {
    let path = resource.path();
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut table = tree;
    for key in parents {
        let entry = table
            .entry((*key).to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if !entry.is_table() {
            *entry = toml::Value::Table(toml::Table::new());
        }
        let toml::Value::Table(inner) = entry else {
            return;
        };
        table = inner;
    }
    table.insert((*last).to_owned(), value);
}

/// Removes `resource` from `tree`, and a table above it the removal leaves
/// empty, returning what it held.
pub fn remove(tree: &mut toml::Table, resource: &Resource) -> Option<toml::Value> {
    let path = resource.path();
    remove_in(tree, &path)
}

/// Removes the key at `path` under `table`, and each table left empty.
fn remove_in(table: &mut toml::Table, path: &[&str]) -> Option<toml::Value> {
    match path {
        [] => None,
        [last] => table.remove(*last),
        [first, rest @ ..] => {
            let inner = table.get_mut(*first)?.as_table_mut()?;
            let removed = remove_in(inner, rest);
            if inner.is_empty() {
                table.remove(*first);
            }
            removed
        }
    }
}

/// Every secret `value` references, by its dotted key inside the resource,
/// in key order.
#[must_use]
pub fn slots(value: &toml::Value) -> Vec<SecretSlot> {
    let mut found = Vec::new();
    if let toml::Value::Table(table) = value {
        slots_in(table, "", &mut found);
    }
    found
}

/// Adds the secrets `table` references, under `prefix`, to `found`.
fn slots_in(table: &toml::Table, prefix: &str, found: &mut Vec<SecretSlot>) {
    for (key, value) in table {
        let at = dotted(prefix, key);
        match value {
            toml::Value::Table(inner) => slots_in(inner, &at, found),
            toml::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    if let toml::Value::Table(inner) = item {
                        slots_in(inner, &format!("{at}[{index}]"), found);
                    }
                }
            }
            toml::Value::String(reference) => {
                if let Some(secret) = key.strip_suffix(FILE)
                    && SECRETS.contains(&secret)
                {
                    found.push(SecretSlot {
                        key: dotted(prefix, secret),
                        reference: reference.clone(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// `value` without its secrets and their references, and with every URL
/// that may carry a credential, in its userinfo or its query, redacted as
/// [`SecretUrl`] renders it.
#[must_use]
pub fn without_secrets(value: &toml::Value) -> toml::Value {
    match value {
        toml::Value::String(text) => toml::Value::String(shown(text)),
        toml::Value::Table(table) => toml::Value::Table(
            table
                .iter()
                .filter(|(key, _)| !is_secret_key(key))
                .map(|(key, inner)| (key.clone(), without_secrets(inner)))
                .collect(),
        ),
        toml::Value::Array(items) => {
            toml::Value::Array(items.iter().map(without_secrets).collect())
        }
        other => other.clone(),
    }
}

/// `text` as a page may show it: a URL redacted as [`SecretUrl`] renders it,
/// any other text as it is.
fn shown(text: &str) -> String {
    if text.contains("://") {
        SecretUrl::new(text).redacted()
    } else {
        text.to_owned()
    }
}

/// Puts back in `settings` each URL `held` holds at the same key whose
/// redaction, as [`without_secrets`] shows it, `settings` carries unchanged.
fn restore(settings: &mut toml::Value, held: &toml::Value) {
    match (settings, held) {
        (toml::Value::String(text), toml::Value::String(kept)) => {
            if *text != *kept && *text == shown(kept) {
                text.clone_from(kept);
            }
        }
        (toml::Value::Table(table), toml::Value::Table(kept)) => {
            for (key, value) in table.iter_mut() {
                if let Some(kept) = kept.get(key) {
                    restore(value, kept);
                }
            }
        }
        (toml::Value::Array(items), toml::Value::Array(kept)) => {
            for (value, kept) in items.iter_mut().zip(kept) {
                restore(value, kept);
            }
        }
        _ => {}
    }
}

/// Whether `key` is a secret or its reference.
fn is_secret_key(key: &str) -> bool {
    SECRETS.contains(&key)
        || key
            .strip_suffix(FILE)
            .is_some_and(|key| SECRETS.contains(&key))
}

/// The first secret or reference `table` names, by its dotted key.
#[must_use]
pub fn secret_named(table: &toml::Table) -> Option<String> {
    let mut found = Vec::new();
    secrets_named(table, "", &mut found);
    found.into_iter().next()
}

/// Adds the secrets and references `table` names, under `prefix`, to `found`.
fn secrets_named(table: &toml::Table, prefix: &str, found: &mut Vec<String>) {
    for (key, value) in table {
        let dotted = dotted(prefix, key);
        if is_secret_key(key) {
            found.push(dotted.clone());
        }
        match value {
            toml::Value::Table(inner) => secrets_named(inner, &dotted, found),
            toml::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    if let toml::Value::Table(inner) = item {
                        secrets_named(inner, &format!("{dotted}[{index}]"), found);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The settings `text` gives: the keys of a table, or one inline table.
///
/// # Errors
///
/// [`SectionFault::Settings`] for text that is neither; the parser's
/// message is not kept, since it may quote the text.
pub fn settings_of(text: &str) -> Result<toml::Table, SectionFault> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        let mut wrapped: toml::Table = toml::from_str(&format!("settings = {trimmed}"))
            .map_err(|_unread| SectionFault::Settings)?;
        return match wrapped.remove("settings") {
            Some(toml::Value::Table(table)) => Ok(table),
            _ => Err(SectionFault::Settings),
        };
    }
    toml::from_str(trimmed).map_err(|_unread| SectionFault::Settings)
}

/// `value` as one TOML inline table, as the console's settings field holds
/// it.
#[must_use]
pub fn inline(value: &toml::Value) -> String {
    value.to_string()
}

/// One secret change, read and checked.
enum Action<'a> {
    /// Write the value to a new file and reference it.
    Value(&'a WriteOnly),
    /// Reference the file.
    File(&'a str),
    /// Remove the secret.
    Clear,
}

/// The action `change` asks for.
fn action(change: &SecretChange) -> Result<Action<'_>, SectionFault> {
    match (&change.value, &change.file, change.clear) {
        (Some(value), None, false) => Ok(Action::Value(value)),
        (None, Some(file), false) => {
            if Path::new(file).is_absolute() {
                Ok(Action::File(file))
            } else {
                Err(SectionFault::Reference {
                    key: change.key.clone(),
                })
            }
        }
        (None, None, true) => Ok(Action::Clear),
        _ => Err(SectionFault::Change {
            key: change.key.clone(),
        }),
    }
}

/// The resource `settings` describe with its secrets: each reference
/// `held` holds that no change names, where its table remains, and each
/// change applied, a value written through `write`.
///
/// # Errors
///
/// [`SectionFault::SecretInSettings`] for settings that name a secret,
/// [`SectionFault::Slot`], [`SectionFault::Change`] and
/// [`SectionFault::Reference`] for a change that does not read, and what
/// `write` returns.
pub fn compose(
    settings: toml::Table,
    held: Option<&toml::Value>,
    changes: &[SecretChange],
    mut write: impl FnMut(&str, &WriteOnly) -> Result<PathBuf, SectionFault>,
) -> Result<toml::Table, SectionFault> {
    if let Some(key) = secret_named(&settings) {
        return Err(SectionFault::SecretInSettings { key });
    }
    let mut composed = settings;
    if let Some(held) = held {
        let mut value = toml::Value::Table(composed);
        restore(&mut value, held);
        composed = match value {
            toml::Value::Table(table) => table,
            _ => toml::Table::new(),
        };
    }
    let named = |key: &str| changes.iter().any(|change| change.key == key);
    for slot in held.map(slots).unwrap_or_default() {
        if !named(&slot.key) {
            // NOTE: no specification governs this: our own design; a reference whose
            // table the settings dropped goes with the table.
            let _placed = place(&mut composed, &slot.key, Some(slot.reference));
        }
    }
    for change in changes {
        let reference = match action(change)? {
            Action::Value(value) => Some(write(&change.key, value)?.to_string_lossy().into_owned()),
            Action::File(file) => Some(file.to_owned()),
            Action::Clear => None,
        };
        if !place(&mut composed, &change.key, reference) {
            return Err(SectionFault::Slot {
                key: change.key.clone(),
            });
        }
    }
    Ok(composed)
}

/// Sets the secret at dotted `key` of `table` to `reference`, or removes it
/// for none; `false` when `key` names no secret inside a table `table`
/// holds.
fn place(table: &mut toml::Table, key: &str, reference: Option<String>) -> bool {
    let segments: Vec<&str> = key.split('.').collect();
    let Some((last, parents)) = segments.split_last() else {
        return false;
    };
    if !SECRETS.contains(last) {
        return false;
    }
    let mut at = table;
    for segment in parents {
        let (name, index) = match segment.split_once('[') {
            Some((name, rest)) => match rest.strip_suffix(']').and_then(|n| n.parse().ok()) {
                Some(index) => (name, Some(index)),
                None => return false,
            },
            None => (*segment, None),
        };
        let next = match (at.get_mut(name), index) {
            (Some(toml::Value::Table(inner)), None) => inner,
            (Some(toml::Value::Array(items)), Some(index)) => match items.get_mut::<usize>(index) {
                Some(toml::Value::Table(inner)) => inner,
                _ => return false,
            },
            _ => return false,
        };
        at = next;
    }
    at.remove(*last);
    let reference_key = format!("{last}{FILE}");
    match reference {
        Some(reference) => {
            at.insert(reference_key, toml::Value::String(reference));
        }
        None => {
            at.remove(&reference_key);
        }
    }
    true
}

/// `key` under `prefix`, dotted.
fn dotted(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    }
}

/// Writes `value`, the secret `key` of `resource`, to a new file in `dir`
/// that only the gateway's user may read, and returns its path.
///
/// The file is never one that exists: each value gets a file of its own, so
/// a revision that references the previous one still reads it.
///
/// # Errors
///
/// [`SectionFault::Write`] when the file cannot be created or written.
pub fn write_secret(
    dir: &Path,
    (resource, key): (&Resource, &str),
    value: &WriteOnly,
) -> Result<PathBuf, SectionFault> {
    let slug = |text: &str| -> String {
        text.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    let name = format!(
        "{}.{}.{}.secret",
        slug(&resource.path().join(".")),
        slug(key),
        uuid::Uuid::new_v4().simple()
    );
    let path = dir.join(name);
    let failed = |source| SectionFault::Write {
        key: key.to_owned(),
        source,
    };
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&path).map_err(failed)?;
    let written = file
        .write_all(value.expose().as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(source) = written {
        drop(file);
        if let Err(removal) = std::fs::remove_file(&path) {
            tracing::warn!(
                error = %removal,
                "a secret file that could not be written could not be removed"
            );
        }
        return Err(failed(source));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ferrofed_registry::resource::{BindingSection, Resource, SecretChange};
    use ferrofed_registry::secret::WriteOnly;

    use super::{
        SectionFault, compose, present, remove, set, settings_of, slots, value_at, without_secrets,
        write_secret,
    };

    fn table(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    fn change(key: &str) -> SecretChange {
        SecretChange {
            key: key.to_owned(),
            value: None,
            file: None,
            clear: false,
        }
    }

    #[test]
    fn a_resource_is_read_set_and_removed_whole() {
        let mut tree = table("[federation]\nbest_effort = true\n[credentials.a]\nuser = \"u\"\n");
        let consent = Resource::Binding(BindingSection::Consent);
        assert_eq!(
            vec![Resource::Credentials(String::from("a"))],
            present(&tree)
        );
        set(
            &mut tree,
            &consent,
            toml::Value::Table(table("disclose = false")),
        );
        assert_eq!(
            Some(&toml::Value::Boolean(false)),
            value_at(&tree, &consent).and_then(|value| value.get("disclose"))
        );
        assert!(remove(&mut tree, &Resource::Credentials(String::from("a"))).is_some());
        assert!(!tree.contains_key("credentials"));
        assert!(remove(&mut tree, &consent).is_some());
        assert_eq!(table("[federation]\nbest_effort = true\n"), tree);
    }

    #[test]
    fn settings_read_as_keys_or_one_inline_table() {
        assert_eq!(table("user = \"u\""), settings_of("user = \"u\"").unwrap());
        assert_eq!(
            table("oauth2 = { scope = \"s\" }"),
            settings_of("{ oauth2 = { scope = \"s\" } }").unwrap()
        );
        assert!(matches!(
            settings_of("{ user ="),
            Err(SectionFault::Settings)
        ));
    }

    #[test]
    fn a_secret_is_held_only_by_reference_and_kept_unless_changed() {
        let held = toml::Value::Table(table(
            "user = \"u\"\npassword_file = \"/run/p\"\n[oauth2]\nclient_secret_file = \"/run/c\"\n",
        ));
        assert_eq!(
            vec!["oauth2.client_secret", "password"],
            slots(&held)
                .iter()
                .map(|slot| slot.key.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            table("user = \"u\"\n[oauth2]\n"),
            *without_secrets(&held).as_table().unwrap()
        );
        let mut cleared = change("password");
        cleared.clear = true;
        let composed = compose(
            table("user = \"v\"\n[oauth2]\nscope = \"s\"\n"),
            Some(&held),
            &[cleared],
            |_, _| Err(SectionFault::Settings),
        )
        .unwrap();
        assert_eq!(
            table("user = \"v\"\n[oauth2]\nscope = \"s\"\nclient_secret_file = \"/run/c\"\n"),
            composed
        );
    }

    #[test]
    fn a_url_credential_is_redacted_and_an_edit_that_keeps_it_keeps_the_url() {
        let held = toml::Value::Table(table(
            "url = \"https://u:synthetic-pw-1@pix.example.org/fhir?key=synthetic-q-2\"\nname = \"plain text\"\n",
        ));
        let shown = without_secrets(&held);
        let text = shown.to_string();
        assert!(!text.contains("synthetic-pw-1"), "{text}");
        assert!(!text.contains("synthetic-q-2"), "{text}");
        assert_eq!(
            Some("plain text"),
            shown.get("name").and_then(toml::Value::as_str)
        );
        let kept = compose(
            shown.as_table().cloned().unwrap(),
            Some(&held),
            &[],
            |_, _| Err(SectionFault::Settings),
        )
        .unwrap();
        assert_eq!(*held.as_table().unwrap(), kept);
        let changed = compose(
            table("url = \"https://pix.example.org/other\""),
            Some(&held),
            &[],
            |_, _| Err(SectionFault::Settings),
        )
        .unwrap();
        assert_eq!(table("url = \"https://pix.example.org/other\""), changed);
    }

    #[test]
    fn settings_that_name_a_secret_and_a_change_that_reads_badly_are_refused() {
        let refused = compose(table("password_file = \"/x\""), None, &[], |_, _| {
            Err(SectionFault::Settings)
        });
        assert!(matches!(
            refused,
            Err(SectionFault::SecretInSettings { .. })
        ));
        let mut both = change("password");
        both.file = Some(String::from("/x"));
        both.clear = true;
        assert!(matches!(
            compose(toml::Table::new(), None, &[both], |_, _| Err(
                SectionFault::Settings
            )),
            Err(SectionFault::Change { .. })
        ));
        let mut relative = change("password");
        relative.file = Some(String::from("x"));
        assert!(matches!(
            compose(toml::Table::new(), None, &[relative], |_, _| Err(
                SectionFault::Settings
            )),
            Err(SectionFault::Reference { .. })
        ));
        let mut elsewhere = change("oauth2.client_secret");
        elsewhere.file = Some(String::from("/x"));
        assert!(matches!(
            compose(toml::Table::new(), None, &[elsewhere], |_, _| Err(
                SectionFault::Settings
            )),
            Err(SectionFault::Slot { .. })
        ));
        let mut not_secret = change("user");
        not_secret.file = Some(String::from("/x"));
        assert!(matches!(
            compose(table("user = \"u\""), None, &[not_secret], |_, _| Err(
                SectionFault::Settings
            )),
            Err(SectionFault::Slot { .. })
        ));
    }

    #[test]
    fn a_value_is_written_by_the_callback_and_referenced() {
        let mut valued = change("password");
        valued.value = Some(WriteOnly::new("synthetic-token-91"));
        let composed = compose(toml::Table::new(), None, &[valued], |key, value| {
            assert_eq!("password", key);
            assert_eq!("synthetic-token-91", value.expose());
            Ok(PathBuf::from("/secrets/written"))
        })
        .unwrap();
        assert_eq!(table("password_file = \"/secrets/written\""), composed);
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "test assertions in a test that returns its setup errors"
    )]
    fn a_secret_file_is_new_and_readable_by_its_owner_alone()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let resource = Resource::Credentials(String::from("node-a.pub"));
        let value = WriteOnly::new("synthetic-secret-55");
        let one = write_secret(dir.path(), (&resource, "oauth2.client_secret"), &value)?;
        let two = write_secret(dir.path(), (&resource, "oauth2.client_secret"), &value)?;
        assert_ne!(one, two);
        assert_eq!("synthetic-secret-55", std::fs::read_to_string(&one)?);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(0o600, std::fs::metadata(&one)?.permissions().mode() & 0o777);
        }
        Ok(())
    }
}
