// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parse faults of a configuration: where the TOML reader refused it and
//! why, with no text the reader quoted (§5.4.1, N33), and the constructors
//! that carry one into an [`Error`].

use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::error::Error;

use ferrofed_registry::error::document::{DocumentFault, KeyNames, Reading};

/// Which text a [`ParseFault`] was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The configuration file as written: the position points into it.
    File,
    /// The tree after the environment overrides, which has no lines of its
    /// own: the fault is located by its key alone.
    Merged,
}

/// The top-level tables whose rows a fault never describes: a row of
/// `[dev]` holds a patient identifier.
const OPAQUE: &[&str] = &["dev"];

/// Where a configuration parse fault sits and what kind it is.
///
/// It holds a position, key names and the schema's own words only, never a
/// value from the configuration, so it is safe in a log, an error chain and
/// `Debug` (§5.4.1, N33).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFault {
    /// The file the fault is in, when the configuration came from one.
    pub file: Option<PathBuf>,
    /// The text the fault was found in.
    pub stage: Stage,
    /// Where the fault is and what kind it is: the line and column in the
    /// file for [`Stage::File`], and the dotted key path of the offending
    /// line, or `dev` alone for any line of the `[dev]` table, whose rows
    /// are not described.
    pub fault: DocumentFault,
}

impl ParseFault {
    pub(super) fn from_toml(error: &toml::de::Error, text: &str, stage: Stage) -> Self {
        let reading = Reading {
            written: stage == Stage::File,
            keys: KeyNames::Written,
            opaque: OPAQUE,
            messages: false,
        };
        Self {
            file: None,
            stage,
            fault: DocumentFault::read(error, text, &reading),
        }
    }

    /// Returns the dotted key path of the offending line, when it is known.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.fault.key.as_deref()
    }
}

impl fmt::Display for ParseFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the configuration")?;
        if let Some(file) = &self.file {
            write!(f, " file {}", file.display())?;
        }
        f.write_str(" is not valid")?;
        if self.stage == Stage::Merged {
            f.write_str(" after the environment overrides")?;
        }
        write!(f, ": {}", self.fault)
    }
}

impl Error {
    /// Builds an [`Error::Parse`] from what the TOML reader reported about
    /// `text`, keeping positions and key names and dropping the quoted source.
    pub(crate) fn parse(error: &toml::de::Error, text: &str, stage: Stage) -> Self {
        Self::Parse {
            fault: ParseFault::from_toml(error, text, stage),
        }
    }

    /// Names `path` as the file a parse fault sits in; any other error is
    /// returned unchanged.
    #[must_use]
    pub(crate) fn in_file(self, path: &Path) -> Self {
        match self {
            Self::Parse { mut fault } => {
                fault.file = Some(path.to_path_buf());
                Self::Parse { fault }
            }
            other => other,
        }
    }
}
