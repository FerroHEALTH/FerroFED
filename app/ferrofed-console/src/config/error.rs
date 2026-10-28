// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a configuration was refused: each refusal names the key, never a
//! value, so no secret reaches a message, a `Debug` rendering or a log.

use std::fmt;
use std::path::PathBuf;

use ferrofed_registry::error::document::{self, DocumentFault, KeyNames, Reading};

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
    /// A value of the wrong type, outside the values its key admits, or
    /// absent where its table needs one.
    InvalidValue,
    /// Any other refusal of the reader.
    Other,
}

impl From<&document::Problem> for Problem {
    fn from(problem: &document::Problem) -> Self {
        match problem {
            document::Problem::Syntax => Self::Syntax,
            document::Problem::UnknownKey => Self::UnknownKey,
            document::Problem::DuplicateKey => Self::DuplicateKey,
            document::Problem::MissingKey { .. } | document::Problem::InvalidValue { .. } => {
                Self::InvalidValue
            }
            _ => Self::Other,
        }
    }
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
        let reading = Reading {
            written: stage == Stage::File,
            keys: KeyNames::Schema,
            opaque: &[],
            messages: false,
        };
        let fault = DocumentFault::read(error, text, &reading);
        Self {
            stage,
            position: fault.position,
            problem: Problem::from(&fault.problem),
            key: fault.key,
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
    /// A secret is set inline and through its `_file` sibling, or its file
    /// cannot be read or holds nothing.
    #[error(transparent)]
    SecretFile(#[from] ferrohealth_secret::file::SecretError),
    /// A socket address does not parse.
    #[error("{key} is not a socket address")]
    Address {
        /// The key.
        key: String,
        /// What the parser refused.
        #[source]
        source: std::net::AddrParseError,
    },
    /// A locale that is no BCP 47 language tag.
    #[error("{key} is not a language tag")]
    Locale {
        /// The key.
        key: String,
        /// What the parser refused.
        #[source]
        source: ferrohealth_ui::i18n::UnknownLanguage,
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
    /// A replica's name is not one the console admits.
    #[error("{key} {reason}")]
    ReplicaName {
        /// The key.
        key: String,
        /// What is wrong with it, as a predicate.
        reason: &'static str,
    },
    /// A scope list names a scope the console asks for through a setting of
    /// its own.
    #[error("{key} {reason}")]
    Scope {
        /// The key.
        key: String,
        /// What is wrong with it, as a predicate.
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
