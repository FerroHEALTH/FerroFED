// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration keys that were renamed, accepted under their old name for at
//! least one release before they are refused.
//!
//! The configuration refuses every key it does not know, so a key renamed
//! from one release to the next would stop a gateway that upgrades. A
//! rename is therefore listed in [`RENAMED`] for at least one release: the
//! old key is read as the new one, and `config check` and the start-up log
//! name it as deprecated, with the release that will refuse it. Setting both
//! the old and the new key is refused. No specification governs the
//! configuration: our own design.

use std::fmt;

use crate::config::error::Error;

/// A configuration key read under a new name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Renamed {
    /// The old dotted key, such as `server.old_key`.
    pub from: &'static str,
    /// The dotted key that replaces it.
    pub to: &'static str,
    /// The first release that refuses the old key.
    pub refused_in: &'static str,
}

/// The renamed keys this release still reads under their old names.
///
/// No key is renamed in this release.
pub const RENAMED: &[Renamed] = &[];

/// A deprecated key a configuration set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deprecated {
    /// The rename it falls under.
    pub renamed: Renamed,
}

impl fmt::Display for Deprecated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is deprecated and read as {}; rename it, because FerroFED {} refuses it",
            self.renamed.from, self.renamed.to, self.renamed.refused_in
        )
    }
}

/// Moves every key of `renamed` that `table` sets to its new name, and
/// returns the deprecated keys it found, in the order of `renamed`.
///
/// # Errors
///
/// [`Error::Renamed`] when `table` sets both the old and the new key.
pub fn apply(table: &mut toml::Table, renamed: &[Renamed]) -> Result<Vec<Deprecated>, Error> {
    let mut found = Vec::new();
    for rename in renamed {
        let Some(value) = take(table, rename.from) else {
            continue;
        };
        if !put(table, rename.to, value) {
            return Err(Error::Renamed {
                from: rename.from.to_owned(),
                to: rename.to.to_owned(),
            });
        }
        found.push(Deprecated { renamed: *rename });
    }
    Ok(found)
}

/// Removes and returns the value at the dotted `key` of `table`.
fn take(table: &mut toml::Table, key: &str) -> Option<toml::Value> {
    let (parents, last) = split(key)?;
    let mut cursor = table;
    for parent in parents {
        cursor = cursor.get_mut(parent)?.as_table_mut()?;
    }
    cursor.remove(last)
}

/// Puts `value` at the dotted `key` of `table`, creating the tables on the
/// way; returns `false`, changing nothing, when the key is already set or a
/// parent is no table.
fn put(table: &mut toml::Table, key: &str, value: toml::Value) -> bool {
    let Some((parents, last)) = split(key) else {
        return false;
    };
    let mut cursor = table;
    for parent in parents {
        let entry = cursor
            .entry(parent.to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let Some(next) = entry.as_table_mut() else {
            return false;
        };
        cursor = next;
    }
    if cursor.contains_key(last) {
        return false;
    }
    cursor.insert(last.to_owned(), value);
    true
}

/// The parent segments and the last segment of the dotted `key`.
fn split(key: &str) -> Option<(Vec<&str>, &str)> {
    let mut segments: Vec<&str> = key.split('.').collect();
    let last = segments.pop()?;
    Some((segments, last))
}

#[cfg(test)]
mod tests {
    use super::{Deprecated, Renamed, apply};
    use crate::config::error::Error;

    const RENAME: Renamed = Renamed {
        from: "server.old_limit",
        to: "server.body_limit_bytes",
        refused_in: "0.0.99",
    };

    fn table(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn an_old_key_is_read_as_the_new_one_and_named() {
        let mut read = table("[server]\nold_limit = 2048\n");
        let found = apply(&mut read, &[RENAME]).unwrap();
        assert_eq!(vec![Deprecated { renamed: RENAME }], found);
        assert_eq!(table("[server]\nbody_limit_bytes = 2048\n"), read);
        let line = found[0].to_string();
        assert!(line.contains("server.old_limit"), "{line}");
        assert!(line.contains("server.body_limit_bytes"), "{line}");
        assert!(line.contains("0.0.99"), "{line}");
    }

    #[test]
    fn a_configuration_without_the_old_key_is_left_alone() {
        let mut read = table("[server]\nbody_limit_bytes = 2048\n");
        assert!(apply(&mut read, &[RENAME]).unwrap().is_empty());
        assert_eq!(table("[server]\nbody_limit_bytes = 2048\n"), read);
    }

    #[test]
    fn the_old_and_the_new_key_together_are_refused() {
        let mut read = table("[server]\nold_limit = 1\nbody_limit_bytes = 2\n");
        let refused = apply(&mut read, &[RENAME]).unwrap_err();
        assert!(matches!(refused, Error::Renamed { .. }), "{refused:?}");
        let message = refused.to_string();
        assert!(message.contains("server.old_limit"), "{message}");
        assert!(message.contains("server.body_limit_bytes"), "{message}");
    }

    #[test]
    fn a_renamed_key_reaches_the_resolved_settings_as_a_warning() {
        let mut read = table("[server]\nold_limit = 2048\n");
        let found = apply(&mut read, &[RENAME]).unwrap();
        let mut config: crate::config::Config =
            toml::from_str(&toml::to_string(&read).unwrap()).unwrap();
        config.deprecated = found;
        let settings = config.resolve().unwrap();
        assert_eq!(2048, settings.server.body_limit);
        assert_eq!(vec![Deprecated { renamed: RENAME }], settings.deprecated);
    }
}
