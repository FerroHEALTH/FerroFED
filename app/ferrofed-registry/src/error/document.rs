// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the TOML reader refused a document and why, with no text it quoted.
//!
//! A line of a registry document may hold a base URL whose query carries a
//! credential, and a line of a configuration a patient identifier. The
//! registry and the server's configuration both read their faults through
//! this one classification.

use std::fmt;

/// What kind of fault the TOML reader met, without any text it quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Problem {
    /// The text is not TOML.
    Syntax,
    /// A key the document's schema does not define.
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
        /// How the reader refused it.
        refused: Refusal,
        /// What the document expects there, in its schema's words.
        expected: String,
    },
    /// Any other refusal of the reader, such as an identifier that breaks
    /// its form.
    Other {
        /// The reader's message less every `"`-quoted run, when the
        /// [`Reading`] keeps messages.
        said: Option<String>,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax => f.write_str("the text is not TOML"),
            Self::UnknownKey => f.write_str("unknown field"),
            Self::MissingKey { key } => write!(f, "missing field `{key}`"),
            Self::DuplicateKey => f.write_str("duplicate key"),
            Self::InvalidValue { refused, expected } => {
                write!(f, "{}, expected {expected}", refused.as_str())
            }
            Self::Other { said: Some(said) } => f.write_str(said),
            Self::Other { said: None } => f.write_str("a value the reader refused"),
        }
    }
}

/// How the reader refused a value, in serde's own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// `invalid type`: a value of the wrong type.
    Type,
    /// `invalid value`: a value of the right type the key does not admit.
    Value,
    /// `invalid length`: a sequence or map of the wrong length.
    Length,
    /// `unknown variant`: a name outside the closed set the key admits.
    Variant,
}

impl Refusal {
    /// Returns serde's words for this refusal, which open its message.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Type => "invalid type",
            Self::Value => "invalid value",
            Self::Length => "invalid length",
            Self::Variant => "unknown variant",
        }
    }
}

/// How a key path is named in a [`DocumentFault`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyNames {
    /// A key path is named only when every segment reads as a key of the
    /// document's schema: lower-case ASCII letters, digits and `_`.
    Schema,
    /// A key path is named as it is written, quoted segments included.
    Written,
}

/// What a [`DocumentFault`] may name of the text it was found in.
#[derive(Debug, Clone, Copy)]
pub struct Reading<'a> {
    /// Whether the text is the document as its author wrote it.
    ///
    /// Then the fault is placed at its line and column, and a syntax fault
    /// is named. Otherwise the text is a rendering of a tree, which has no
    /// lines of its own and cannot break the TOML syntax.
    pub written: bool,
    /// How a key path is named.
    pub keys: KeyNames,
    /// The top-level tables whose rows are never described: a fault in one
    /// is named by the table alone.
    pub opaque: &'a [&'a str],
    /// Whether the reader's message for a refusal no other [`Problem`]
    /// names is kept, less every `"`-quoted run.
    pub messages: bool,
}

impl Default for Reading<'_> {
    fn default() -> Self {
        Self {
            written: true,
            keys: KeyNames::Schema,
            opaque: &[],
            messages: true,
        }
    }
}

/// Where a TOML document parse fault sits and what kind it is.
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
    /// its position, its key path and its kind under the default
    /// [`Reading`].
    #[must_use]
    pub fn from_toml(error: &toml::de::Error, text: &str) -> Self {
        Self::read(error, text, &Reading::default())
    }

    /// Returns the fault `error` reports of `text`, naming what `reading`
    /// admits.
    #[must_use]
    pub fn read(error: &toml::de::Error, text: &str, reading: &Reading<'_>) -> Self {
        let offset = error.span().map(|span| span.start);
        Self {
            position: if reading.written {
                offset.and_then(|at| line_column(text, at))
            } else {
                None
            },
            key: offset.and_then(|at| key_path(text, at, reading)),
            problem: classify(error.message(), reading),
        }
    }

    /// Returns the fault `error` reports of a TOML value that has no text,
    /// which has neither a position nor a key path.
    #[must_use]
    pub fn unlocated(error: &toml::de::Error) -> Self {
        let reading = Reading {
            written: false,
            ..Reading::default()
        };
        Self {
            position: None,
            key: None,
            problem: classify(error.message(), &reading),
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
fn classify(message: &str, reading: &Reading<'_>) -> Problem {
    let first = message.lines().next().unwrap_or_default();
    if let Some(rest) = first.strip_prefix("missing field `") {
        return match rest.split_once('`') {
            Some((key, _)) if is_schema_key(key) => Problem::MissingKey {
                key: key.to_owned(),
            },
            _ => Problem::Other {
                said: Some(String::from("missing field")),
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
        Refusal::Type,
        Refusal::Value,
        Refusal::Length,
        Refusal::Variant,
    ]
    .into_iter()
    .find(|refusal| first.starts_with(refusal.as_str()));
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
    if reading.written && is_syntax(first) {
        Problem::Syntax
    } else {
        Problem::Other {
            said: reading.messages.then(|| unquoted(first)),
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
/// and the key before `=`, as `reading` names them, or the table alone for
/// any line of an opaque table.
fn key_path(text: &str, offset: usize, reading: &Reading<'_>) -> Option<String> {
    let before = text.get(..offset)?;
    let start = before.rfind('\n').map_or(0, |at| at.saturating_add(1));
    let line = text
        .get(start..)?
        .split('\n')
        .next()
        .unwrap_or_default()
        .trim();
    let (header, key) = if line.starts_with('[') {
        (Some(header_name(line)), None)
    } else {
        let header = text
            .get(..start)?
            .lines()
            .rev()
            .map(str::trim)
            .find(|candidate| candidate.starts_with('['))
            .map(header_name);
        let key = line
            .split_once('=')
            .map(|(key, _)| key.trim().to_owned())
            .filter(|key| !key.is_empty());
        (header, key)
    };
    let root = header
        .as_deref()
        .or(key.as_deref())?
        .split('.')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches('"');
    if let Some(opaque) = reading.opaque.iter().find(|table| **table == root) {
        return Some((*opaque).to_owned());
    }
    let named = |path: String| match reading.keys {
        KeyNames::Written => Some(path),
        KeyNames::Schema => path.split('.').all(is_schema_key).then_some(path),
    };
    match (header.and_then(named), key.and_then(named)) {
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

    /// The error of parsing `text` as a table.
    fn refused(text: &str) -> toml::de::Error {
        toml::from_str::<toml::Table>(text).expect_err("a refused document")
    }

    /// The fault of parsing `text` as a table, rendered.
    fn fault(text: &str) -> String {
        DocumentFault::from_toml(&refused(text), text).to_string()
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

    #[test]
    fn a_written_reading_names_a_quoted_segment() {
        let text = "[credentials.\"a\"]\ntoken = \"synthetic\nother = 1\n";
        let reading = Reading {
            keys: KeyNames::Written,
            ..Reading::default()
        };
        let fault = DocumentFault::read(&refused(text), text, &reading);
        assert_eq!(Some("credentials.\"a\".token"), fault.key.as_deref());
        assert_eq!(Some(2), fault.position.map(|(line, _)| line));
    }

    #[test]
    fn a_line_of_an_opaque_table_is_named_by_the_table_alone() {
        let text = "[[dev.crossref]]\n\"synthetic-secret-7\" = \"x\nnext = 1\n";
        let reading = Reading {
            keys: KeyNames::Written,
            opaque: &["dev"],
            ..Reading::default()
        };
        let fault = DocumentFault::read(&refused(text), text, &reading);
        assert_eq!(Some("dev"), fault.key.as_deref());
        assert!(!fault.to_string().contains("synthetic-secret-7"));
    }

    #[test]
    fn a_rendered_text_has_no_position_and_no_syntax_fault() {
        let text = "a = \"synthetic\nb = 1\n";
        let reading = Reading {
            written: false,
            messages: false,
            ..Reading::default()
        };
        let fault = DocumentFault::read(&refused(text), text, &reading);
        assert_eq!(None, fault.position);
        assert_eq!(Problem::Other { said: None }, fault.problem);
    }
}
