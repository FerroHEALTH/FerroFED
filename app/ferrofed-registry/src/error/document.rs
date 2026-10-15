// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the TOML reader refused a registry document and why, with no text
//! the reader quoted: a line of the document may hold a base URL whose
//! query carries a credential.

use std::fmt;

/// What kind of fault the TOML reader met, without any text it quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Problem {
    /// The text is not TOML.
    Syntax,
    /// A key the registry document does not define.
    UnknownKey,
    /// A key its table needs is absent.
    MissingKey {
        /// The key, as the document's schema names it.
        key: String,
    },
    /// A key is set twice.
    DuplicateKey,
    /// A value of the wrong type, or outside the values the key admits.
    InvalidValue {
        /// How the reader refused it: `invalid type`, `invalid value`,
        /// `invalid length` or `unknown variant`.
        refused: &'static str,
        /// What the document expects there, in its schema's words.
        expected: String,
    },
    /// Any other refusal of the reader, such as an identifier that breaks
    /// its form: its message with every quoted value left out.
    Other {
        /// The reader's message, less every `"`-quoted run.
        said: String,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax => f.write_str("the text is not TOML"),
            Self::UnknownKey => f.write_str("unknown field"),
            Self::MissingKey { key } => write!(f, "missing field `{key}`"),
            Self::DuplicateKey => f.write_str("duplicate key"),
            Self::InvalidValue { refused, expected } => write!(f, "{refused}, expected {expected}"),
            Self::Other { said } => f.write_str(said),
        }
    }
}

/// Where a registry document parse fault sits and what kind it is.
///
/// It holds a position, key names and the schema's own words only, never a
/// value from the document, so it is safe in a log, an error chain and
/// `Debug`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct DocumentFault {
    /// The 1-based line and column of the fault.
    pub position: Option<(usize, usize)>,
    /// The dotted key path of the offending line.
    pub key: Option<String>,
    /// What kind of fault it is.
    pub problem: Problem,
}

impl DocumentFault {
    /// Returns the fault `error` reports of the document `text`, keeping
    /// its position, its key path and its kind.
    #[must_use]
    pub fn from_toml(error: &toml::de::Error, text: &str) -> Self {
        let offset = error.span().map(|span| span.start);
        Self {
            position: offset.and_then(|at| line_column(text, at)),
            key: offset.and_then(|at| key_path(text, at)),
            problem: classify(error.message()),
        }
    }
}

impl fmt::Display for DocumentFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.problem)?;
        if let Some(key) = &self.key {
            write!(f, " at {key}")?;
        }
        if let Some((line, column)) = self.position {
            write!(f, " (line {line}, column {column})")?;
        }
        Ok(())
    }
}

/// Sorts the reader's message into a [`Problem`], keeping only the key a
/// missing-field refusal names and the schema text after `expected`.
fn classify(message: &str) -> Problem {
    let first = message.lines().next().unwrap_or_default();
    if let Some(rest) = first.strip_prefix("missing field `") {
        return match rest.split_once('`') {
            Some((key, _)) if is_schema_key(key) => Problem::MissingKey {
                key: key.to_owned(),
            },
            _ => Problem::Other {
                said: String::from("missing field"),
            },
        };
    }
    if first.starts_with("unknown field") {
        return Problem::UnknownKey;
    }
    if first.starts_with("duplicate key") || first.starts_with("duplicate field") {
        return Problem::DuplicateKey;
    }
    let refused = [
        "invalid type",
        "invalid value",
        "invalid length",
        "unknown variant",
    ]
    .into_iter()
    .find(|prefix| first.starts_with(prefix));
    if let Some(refused) = refused {
        // NOTE: serde writes the schema's expectation last, after any quoted
        // value, so the text after the last `, expected ` is the schema's.
        return Problem::InvalidValue {
            refused,
            expected: first
                .rsplit_once(", expected ")
                .map(|(_, expected)| unquoted(expected))
                .unwrap_or_default(),
        };
    }
    if is_syntax(first) {
        Problem::Syntax
    } else {
        Problem::Other {
            said: unquoted(first),
        }
    }
}

/// Returns `text` with every `"`-quoted run, its escapes included, left out.
fn unquoted(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut quoted = false;
    let mut chars = text.chars();
    while let Some(next) = chars.next() {
        match next {
            '"' => quoted = !quoted,
            '\\' if quoted => {
                chars.next();
            }
            _ if quoted => {}
            other => kept.push(other),
        }
    }
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether the reader's first message line reads as a TOML syntax fault,
/// which never comes from the document's own schema.
fn is_syntax(first: &str) -> bool {
    first.starts_with("invalid")
        || first.starts_with("expected")
        || first.starts_with("unexpected")
        || first.starts_with("unclosed")
        || first.starts_with("key with no value")
        || first.starts_with("missing")
        || first.starts_with("extra")
}

/// Whether `key` reads as a key the document's schema defines.
fn is_schema_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The 1-based line and column of byte `offset` in `text`.
fn line_column(text: &str, offset: usize) -> Option<(usize, usize)> {
    let before = text.get(..offset)?;
    let line = before.matches('\n').count().saturating_add(1);
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        .saturating_add(1);
    Some((line, column))
}

/// The dotted key path of the line holding byte `offset`: its table header
/// and the key before `=`, when the key reads as one of the schema's.
fn key_path(text: &str, offset: usize) -> Option<String> {
    let before = text.get(..offset)?;
    let start = before.rfind('\n').map_or(0, |at| at.saturating_add(1));
    let line = text
        .get(start..)?
        .split('\n')
        .next()
        .unwrap_or_default()
        .trim();
    let table = |line: &str| {
        let name = header_name(line);
        name.split('.').all(is_schema_key).then_some(name)
    };
    if line.starts_with('[') {
        return table(line);
    }
    let header = text
        .get(..start)?
        .lines()
        .rev()
        .map(str::trim)
        .find(|candidate| candidate.starts_with('['))
        .and_then(table);
    let key = line
        .split_once('=')
        .map(|(key, _)| key.trim().to_owned())
        .filter(|key| is_schema_key(key));
    match (header, key) {
        (Some(header), Some(key)) => Some(format!("{header}.{key}")),
        (Some(header), None) => Some(header),
        (None, Some(key)) => Some(key),
        (None, None) => None,
    }
}

/// The name a `[table]` or `[[array.of.tables]]` header line declares.
fn header_name(line: &str) -> String {
    line.trim_start_matches('[')
        .split(']')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fault of parsing `text` as a table, rendered.
    fn fault(text: &str) -> String {
        let error = toml::from_str::<toml::Table>(text).expect_err("a refused document");
        DocumentFault::from_toml(&error, text).to_string()
    }

    #[test]
    fn a_syntax_fault_names_its_position_and_never_the_line() {
        let text = "[[endpoint]]\nid = \"node-a-pub\"\nurl = \"https://cdr.example.org/x?key=synthetic-secret-7\nnode = \"node-a\"\n";
        let shown = fault(text);
        assert!(!shown.contains("synthetic-secret-7"), "{shown}");
        assert!(shown.contains("line 3"), "{shown}");
        assert!(shown.contains("endpoint.url"), "{shown}");
    }

    #[test]
    fn a_quoted_value_is_left_out_of_a_message() {
        assert_eq!(
            "system_id is not an openEHR uid",
            unquoted(r#"system_id "a \"synthetic-secret-7\" b" is not an openEHR uid"#)
        );
        assert_eq!(
            "invalid type: string , expected u64",
            unquoted(r#"invalid type: string "synthetic-secret-7", expected u64"#)
        );
    }

    #[test]
    fn a_key_that_is_not_the_schemas_is_not_named() {
        let shown = fault("[[endpoint]]\nsynthetic-secret-7 x\n");
        assert!(!shown.contains("synthetic-secret-7"), "{shown}");
    }
}
