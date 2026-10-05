// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a configuration was refused: each refusal names the key, never a
//! value, so no secret reaches a message, a `Debug` rendering or a log.

use std::fmt;
use std::path::PathBuf;

/// Which text a [`ParseFault`] was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The configuration file as written: the position points into it.
    File,
    /// The tree after the environment overrides, which has no lines of its
    /// own: the fault is located by its key alone.
    Merged,
}

/// What kind of fault the TOML reader met, with no text it quoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The text is not TOML.
    Syntax,
    /// A key the configuration does not define.
    UnknownKey,
    /// A key is set twice.
    DuplicateKey,
    /// A value of the wrong type, or outside the values its key admits.
    InvalidValue,
    /// Any other refusal of the reader.
    Other,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Syntax => "the text is not TOML",
            Self::UnknownKey => "an unknown key",
            Self::DuplicateKey => "a key set twice",
            Self::InvalidValue => "a value its key does not admit",
            Self::Other => "a value the reader refused",
        })
    }
}

/// Where a configuration parse fault sits and what kind it is: a position,
/// a key name and a [`Problem`], never a value from the configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFault {
    /// The text the fault was found in.
    pub stage: Stage,
    /// The 1-based line and column, for [`Stage::File`].
    pub position: Option<(usize, usize)>,
    /// The dotted key path of the offending line, when it reads as one.
    pub key: Option<String>,
    /// What kind of fault it is.
    pub problem: Problem,
}

impl ParseFault {
    /// Reads `error`, which the TOML reader raised over `text`, keeping
    /// nothing the reader quoted.
    #[must_use]
    pub fn from_toml(error: &toml::de::Error, text: &str, stage: Stage) -> Self {
        let offset = error.span().map(|span| span.start);
        Self {
            stage,
            position: match stage {
                Stage::File => offset.and_then(|at| line_column(text, at)),
                Stage::Merged => None,
            },
            key: offset.and_then(|at| key_path(text, at)),
            problem: classify(error.message(), stage),
        }
    }
}

impl fmt::Display for ParseFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the configuration is not valid")?;
        if self.stage == Stage::Merged {
            f.write_str(" after the environment overrides")?;
        }
        write!(f, ": {}", self.problem)?;
        if let Some(key) = &self.key {
            write!(f, " at {key}")?;
        }
        if let Some((line, column)) = self.position {
            write!(f, " (line {line}, column {column})")?;
        }
        Ok(())
    }
}

/// Sorts the reader's message into a [`Problem`]; the message itself is
/// dropped.
fn classify(message: &str, stage: Stage) -> Problem {
    let first = message.lines().next().unwrap_or_default();
    if first.starts_with("unknown field") {
        Problem::UnknownKey
    } else if first.starts_with("duplicate key") || first.starts_with("duplicate field") {
        Problem::DuplicateKey
    } else if [
        "invalid type:",
        "invalid value:",
        "invalid length",
        "unknown variant",
        "missing field",
    ]
    .iter()
    .any(|prefix| first.starts_with(prefix))
    {
        Problem::InvalidValue
    } else if stage == Stage::File {
        Problem::Syntax
    } else {
        Problem::Other
    }
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
/// and the key before `=`, kept only when every part reads as a key the
/// configuration could define, so no stray text is ever echoed.
fn key_path(text: &str, offset: usize) -> Option<String> {
    let before = text.get(..offset)?;
    let start = before.rfind('\n').map_or(0, |at| at.saturating_add(1));
    let line = text.get(start..)?.lines().next().unwrap_or_default().trim();
    let path = if let Some(header) = line.strip_prefix('[') {
        header.trim_end_matches(']').trim().to_owned()
    } else {
        let (key, _value) = line.split_once('=')?;
        let table = text
            .get(..start)?
            .lines()
            .rev()
            .map(str::trim)
            .find_map(|candidate| candidate.strip_prefix('['))
            .map(|header| header.trim_end_matches(']').trim().to_owned());
        let key = key.trim().trim_matches('"');
        match table {
            Some(table) => format!("{table}.{key}"),
            None => key.to_owned(),
        }
    };
    let schema = !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
    schema.then_some(path)
}

/// A configuration the console will not start on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configuration file could not be read.
    #[error("cannot read the configuration file {}", path.display())]
    Read {
        /// The file named.
        path: PathBuf,
        /// Why it could not be read.
        #[source]
        source: std::io::Error,
    },
    /// The TOML does not read as the configuration tree.
    ///
    /// The fault holds a position, a key name and the kind of fault, never
    /// the reader's own message, which can quote a value.
    #[error("{fault}")]
    Parse {
        /// Where the reader refused the text, and why.
        fault: ParseFault,
    },
    /// The merged tree could not be written back for the second read.
    #[error("the configuration with its environment overrides cannot be assembled")]
    Assemble {
        /// What the TOML writer refused.
        #[source]
        source: toml::ser::Error,
    },
    /// An environment override names no section and key.
    #[error("the environment variable {name} names no configuration key")]
    EnvName {
        /// The variable's name.
        name: String,
    },
    /// An environment override names a key under a value that is not a table.
    #[error("the environment variable {name} names a key under a value that is not a table")]
    EnvShape {
        /// The variable's name.
        name: String,
    },
    /// A required key is absent or empty.
    #[error("{key} is required")]
    Missing {
        /// The key.
        key: String,
    },
    /// A secret is set inline and through its `_file` sibling.
    #[error("{key} and {key}_file are both set; set one")]
    Conflict {
        /// The key.
        key: String,
    },
    /// A secret file could not be read.
    #[error("cannot read {key} from {}", path.display())]
    Secret {
        /// The `_file` key.
        key: String,
        /// The file it names.
        path: PathBuf,
        /// Why it could not be read.
        #[source]
        source: std::io::Error,
    },
    /// A secret file holds nothing but white space.
    #[error("{key} names {}, which is empty", path.display())]
    EmptySecret {
        /// The `_file` key.
        key: String,
        /// The file it names.
        path: PathBuf,
    },
    /// A socket address does not parse.
    #[error("{key} is not a socket address")]
    Address {
        /// The key.
        key: String,
        /// What the parser refused.
        #[source]
        source: std::net::AddrParseError,
    },
    /// A URL does not parse.
    #[error("{key} is not a URL")]
    Url {
        /// The key.
        key: String,
        /// What the parser refused.
        #[source]
        source: url::ParseError,
    },
    /// A URL parses but is not one this key admits.
    #[error("{key} {reason}")]
    UrlShape {
        /// The key.
        key: String,
        /// What the URL lacks, as a predicate.
        reason: &'static str,
    },
    /// A number is out of its range.
    #[error("{key} must be at least 1")]
    Zero {
        /// The key.
        key: String,
    },
    /// The cookies are set without `Secure` on a console that is not on
    /// loopback.
    #[error(
        "{key} = false is admitted only when oidc.redirect_uri is http on a loopback host (localhost, 127.0.0.0/8 or ::1)"
    )]
    InsecureCookie {
        /// The key.
        key: String,
    },
}
