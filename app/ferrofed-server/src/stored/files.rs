// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read-only store: stored-query definitions loaded from files when the
//! gateway starts, so several instances share them with no shared database.
//!
//! The directory mirrors the ITS-REST path of a definition: each definition
//! is one file `{path}/{qualified_query_name}/{version}.aql` holding its AQL
//! as UTF-8 text, as `PUT {base}/v1/definition/query/{qualified_query_name}/{version}`
//! would send it (ITS-REST Definition API, "Qualified query name"). A name and
//! a version each have one spelling, so the layout holds at most one file per
//! name and version. Each file is admitted as a `PUT` admits its body, and the
//! registry holds the canonical print; its `saved` instant is the file's
//! modification time. Anything else in the directory, an unreadable file, or
//! a definition the admission refuses refuses the store, naming the file and
//! never quoting its content. No specification governs the layout: our own
//! design.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::Utf8Error;

use ferrofed_registry::definition::store::{DefinitionStore, Insertion, StoreError};
use ferrofed_registry::definition::{
    QueryName, QueryNameError, QueryVersion, QueryVersionError, StoredDefinition,
};
use jiff::Timestamp;
use openehr_federation::aql::Context;
use openehr_federation::aql::definition::{Definition, SubjectOrigin};
use openehr_federation::aql::refusal::Refusal;

/// The extension of a definition file.
pub const EXTENSION: &str = "aql";

/// A definition directory that does not load, naming the file at fault.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FilesError {
    /// The directory, an entry or a file could not be read.
    #[error("{} could not be read", file.display())]
    Read {
        /// The path that was read.
        file: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// An entry is not a `{qualified_query_name}` directory holding
    /// `{version}.aql` files.
    #[error("{} is not a definition file: the layout is {{qualified_query_name}}/{{version}}.aql", file.display())]
    Layout {
        /// The entry.
        file: PathBuf,
    },
    /// A directory name is not a qualified query name.
    #[error("{} is not named for a stored query", file.display())]
    Name {
        /// The directory.
        file: PathBuf,
        /// Why the name is refused.
        #[source]
        source: QueryNameError,
    },
    /// A file name is not `major.minor.patch` with the extension.
    #[error("{} is not named for a stored-query version", file.display())]
    Version {
        /// The file.
        file: PathBuf,
        /// Why the version is refused.
        #[source]
        source: QueryVersionError,
    },
    /// A file is not UTF-8 text.
    #[error("{} is not UTF-8 text", file.display())]
    Text {
        /// The file.
        file: PathBuf,
        /// Where the text stops being UTF-8.
        #[source]
        source: Utf8Error,
    },
    /// The admission a `PUT` passes refuses the definition (§12.7, §7.1).
    #[error("{} is refused as a stored-query definition", file.display())]
    Refused {
        /// The file.
        file: PathBuf,
        /// The refusal, which quotes nothing of the text.
        #[source]
        source: Refusal,
    },
    /// The definition names its patient by a literal, which the registry
    /// never holds (§5.4.1, N33).
    #[error("{} names its patient by a literal; a stored query names it through a $parameter", file.display())]
    SubjectLiteral {
        /// The file.
        file: PathBuf,
    },
    /// The file's modification time is not an instant the registry reports.
    #[error("{} has no modification time the registry can report", file.display())]
    Saved {
        /// The file.
        file: PathBuf,
        /// What the time library reported.
        #[source]
        source: jiff::Error,
    },
}

/// The definitions of one directory, loaded once and never written.
#[derive(Debug)]
pub struct FilesStore {
    path: PathBuf,
    definitions: Vec<StoredDefinition>,
}

impl FilesStore {
    /// Loads every definition under `path`, each admitted under `context`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Corrupt`] carrying the [`FilesError`] of the first
    /// entry, in name order, that does not load.
    pub fn open(path: &Path, context: &Context) -> Result<Self, StoreError> {
        let definitions =
            load(path, context).map_err(|error| StoreError::Corrupt(Box::new(error)))?;
        Ok(Self {
            path: path.to_path_buf(),
            definitions,
        })
    }

    /// The directory the store was loaded from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl DefinitionStore for FilesStore {
    fn insert_if_absent(&self, _definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        Err(StoreError::ReadOnly)
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        Ok(self.definitions.clone())
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

/// Every definition under `path`, in name and version order.
fn load(path: &Path, context: &Context) -> Result<Vec<StoredDefinition>, FilesError> {
    let mut definitions = Vec::new();
    for directory in entries(path)? {
        if !is_directory(&directory)? {
            return Err(FilesError::Layout { file: directory });
        }
        let name = file_name(&directory)?;
        let name = QueryName::new(&name).map_err(|source| FilesError::Name {
            file: directory.clone(),
            source,
        })?;
        for file in entries(&directory)? {
            definitions.push(definition(&file, &name, context)?);
        }
    }
    Ok(definitions)
}

/// The entries of the directory `path`, sorted by name.
fn entries(path: &Path) -> Result<Vec<PathBuf>, FilesError> {
    let read = |source| FilesError::Read {
        file: path.to_path_buf(),
        source,
    };
    let mut entries = fs::read_dir(path)
        .map_err(read)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(read)?;
    entries.sort();
    Ok(entries)
}

/// Whether `path` is a directory, following a symbolic link.
fn is_directory(path: &Path) -> Result<bool, FilesError> {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .map_err(|source| FilesError::Read {
            file: path.to_path_buf(),
            source,
        })
}

/// The last component of `path` as text.
fn file_name(path: &Path) -> Result<String, FilesError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| FilesError::Layout {
            file: path.to_path_buf(),
        })
}

/// The definition of `name` the file `file` holds.
fn definition(
    file: &Path,
    name: &QueryName,
    context: &Context,
) -> Result<StoredDefinition, FilesError> {
    let layout = || FilesError::Layout {
        file: file.to_path_buf(),
    };
    let named = file_name(file)?;
    let stem = named
        .strip_suffix(EXTENSION)
        .and_then(|stem| stem.strip_suffix('.'))
        .ok_or_else(layout)?;
    let version = stem
        .parse::<QueryVersion>()
        .map_err(|source| FilesError::Version {
            file: file.to_path_buf(),
            source,
        })?;
    let read = |source| FilesError::Read {
        file: file.to_path_buf(),
        source,
    };
    let metadata = fs::metadata(file).map_err(read)?;
    if !metadata.is_file() {
        return Err(layout());
    }
    let modified = metadata.modified().map_err(read)?;
    let saved = Timestamp::try_from(modified).map_err(|source| FilesError::Saved {
        file: file.to_path_buf(),
        source,
    })?;
    let bytes = fs::read(file).map_err(read)?;
    let text = std::str::from_utf8(&bytes).map_err(|source| FilesError::Text {
        file: file.to_path_buf(),
        source,
    })?;
    let admitted = Definition::admit(text, context).map_err(|source| FilesError::Refused {
        file: file.to_path_buf(),
        source,
    })?;
    if let Some(SubjectOrigin::Literal { .. }) = admitted.subject() {
        return Err(FilesError::SubjectLiteral {
            file: file.to_path_buf(),
        });
    }
    Ok(StoredDefinition::new(
        name.clone(),
        version,
        admitted.aql().to_owned(),
        saved,
    ))
}
