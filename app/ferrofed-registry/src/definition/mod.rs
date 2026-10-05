// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query definitions of the federated registry (§12.7, N44): the
//! qualified query name, the semver version, the held definition, and the
//! store behind them.
//!
//! A gateway that offers the registry is authoritative for each definition
//! and versions it with ITS-REST's own semver path segment, and a stored
//! version is immutable (§12.7, N44). The names and versions follow the
//! ITS-REST Query API's "Qualified query name" rules: a name is
//! `[{namespace}::]{query-name}`, and a version is `major.minor.patch`, or,
//! where a version is looked up, a `{major}` or `{major}.{minor}` prefix that
//! selects the highest version it matches.
//!
//! ```
//! use ferrofed_registry::definition::{QueryName, QueryVersion, VersionPattern};
//!
//! let name: QueryName = "org.example::compositions".parse()?;
//! assert_eq!(Some("org.example"), name.namespace());
//! let version: QueryVersion = "1.10.0".parse()?;
//! assert!(version > "1.9.3".parse()?);
//! let pattern: VersionPattern = "1".parse()?;
//! assert!(pattern.matches(&version));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The definitions live behind [`store::DefinitionStore`], whose one write
//! is an insert that refuses a held name and version, and every read goes
//! through the in-memory [`store::Definitions`] (no specification governs
//! the storage: our own design).

pub mod store;

use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;

/// The ITS-REST separator between a query's namespace and its name.
const NAMESPACE_SEPARATOR: &str = "::";

/// The query name ITS-REST reserves for ad hoc AQL (`/query/aql`).
const RESERVED: &str = "aql";

/// A stored query's qualified name, `[{namespace}::]{query-name}` (ITS-REST
/// Query API, "Qualified query name").
///
/// The query name is one or more of `[a-zA-Z0-9_.-]` and is never `aql` in
/// any case, which ITS-REST reserves; the namespace, "a reverse domain name",
/// takes the same characters. The type has no `Display` of a refused value:
/// a refusal never quotes what was sent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryName(String);

/// Why a text is no qualified query name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum QueryNameError {
    /// The name, or its namespace, is empty.
    #[error("a stored query name and its namespace are not empty")]
    Empty,
    /// A character outside `[a-zA-Z0-9_.-]`, or more than one `::`.
    #[error(
        "a stored query name is [{{namespace}}::]{{query-name}} over the characters a-z, A-Z, 0-9, _, . and -"
    )]
    Malformed,
    /// The query name is `aql`, which ITS-REST reserves.
    #[error("the query name aql is reserved for ad hoc queries")]
    Reserved,
}

impl QueryName {
    /// Builds the name from its text.
    ///
    /// # Errors
    ///
    /// [`QueryNameError`] when `value` is not `[{namespace}::]{query-name}`
    /// over the ITS-REST characters, or names the reserved `aql`.
    pub fn new(value: &str) -> Result<Self, QueryNameError> {
        let (namespace, name) = match value.split_once(NAMESPACE_SEPARATOR) {
            Some((namespace, name)) => (Some(namespace), name),
            None => (None, value),
        };
        for part in namespace.into_iter().chain([name]) {
            if part.is_empty() {
                return Err(QueryNameError::Empty);
            }
            if !part.chars().all(is_name_char) {
                return Err(QueryNameError::Malformed);
            }
        }
        if name.eq_ignore_ascii_case(RESERVED) {
            return Err(QueryNameError::Reserved);
        }
        Ok(Self(value.to_owned()))
    }

    /// The name as ITS-REST spells it, namespace included.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The namespace, when the name has one.
    #[must_use]
    pub fn namespace(&self) -> Option<&str> {
        self.0
            .split_once(NAMESPACE_SEPARATOR)
            .map(|(namespace, _)| namespace)
    }
}

impl FromStr for QueryName {
    type Err = QueryNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl fmt::Display for QueryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `c` may appear in a query name or a namespace (ITS-REST,
/// "Qualified query name": `[a-zA-Z0-9_.-]`).
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// A stored query's version, `major.minor.patch` (ITS-REST Query API,
/// "Qualified query name": "SEMVER style (i.e. `major.minor.patch`)").
///
/// Each part is a decimal number without a leading zero, as Semantic
/// Versioning 2.0.0 §2 (<https://semver.org/>)
/// writes it, so a version has one spelling. Versions order by major, then
/// minor, then patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

/// Why a text is no version, or no version prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum QueryVersionError {
    /// The text is not `major.minor.patch` over numbers without a leading
    /// zero.
    #[error("a stored query version is major.minor.patch, each a number without a leading zero")]
    Malformed,
    /// The text is not `{major}`, `{major}.{minor}` or
    /// `major.minor.patch`.
    #[error("a stored query version is {{major}}, {{major}}.{{minor}} or major.minor.patch")]
    Pattern,
}

impl QueryVersion {
    /// The version `major.minor.patch`.
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl FromStr for QueryVersion {
    type Err = QueryVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match numbers(value)
            .ok_or(QueryVersionError::Malformed)?
            .as_slice()
        {
            [major, minor, patch] => Ok(Self::new(*major, *minor, *patch)),
            _ => Err(QueryVersionError::Malformed),
        }
    }
}

impl fmt::Display for QueryVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The dot-separated decimal numbers of `value`, or `None` when a part is
/// empty, holds anything but digits, has a leading zero or exceeds `u64`.
fn numbers(value: &str) -> Option<Vec<u64>> {
    value
        .split('.')
        .map(|part| {
            let digits = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            let leading_zero = part.len() > 1 && part.starts_with('0');
            // NOTE: no specification governs this: our own design; a part past
            // `u64` is refused as malformed, the answer the parse failure gives.
            (digits && !leading_zero)
                .then(|| part.parse::<u64>().ok())
                .flatten()
        })
        .collect()
}

/// The version a stored query is looked up at: exact, or a `{major}` or
/// `{major}.{minor}` prefix that selects the highest version it matches
/// (ITS-REST Query API, the `version` path parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VersionPattern {
    /// `major.minor.patch`: that version only.
    Exact(QueryVersion),
    /// `{major}`: every version with that major number.
    Major(u64),
    /// `{major}.{minor}`: every version with those numbers.
    MajorMinor(u64, u64),
}

impl VersionPattern {
    /// Whether `version` is one this pattern selects.
    #[must_use]
    pub fn matches(&self, version: &QueryVersion) -> bool {
        match *self {
            Self::Exact(exact) => exact == *version,
            Self::Major(major) => version.major == major,
            Self::MajorMinor(major, minor) => version.major == major && version.minor == minor,
        }
    }
}

impl FromStr for VersionPattern {
    type Err = QueryVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match numbers(value).ok_or(QueryVersionError::Pattern)?.as_slice() {
            [major] => Ok(Self::Major(*major)),
            [major, minor] => Ok(Self::MajorMinor(*major, *minor)),
            [major, minor, patch] => Ok(Self::Exact(QueryVersion::new(*major, *minor, *patch))),
            _ => Err(QueryVersionError::Pattern),
        }
    }
}

/// One stored-query definition at one version, as the registry holds it.
///
/// The AQL is the text the registry admitted, which names any patient
/// through a `$parameter`, so a definition holds no patient identifier
/// (§5.4.1, N33).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDefinition {
    name: QueryName,
    version: QueryVersion,
    aql: String,
    saved: Timestamp,
}

impl StoredDefinition {
    /// The definition of `name` at `version`, holding `aql`, stored at
    /// `saved`.
    #[must_use]
    pub fn new(name: QueryName, version: QueryVersion, aql: String, saved: Timestamp) -> Self {
        Self {
            name,
            version,
            aql,
            saved,
        }
    }

    /// The qualified name.
    #[must_use]
    pub fn name(&self) -> &QueryName {
        &self.name
    }

    /// The version.
    #[must_use]
    pub fn version(&self) -> QueryVersion {
        self.version
    }

    /// The AQL text.
    #[must_use]
    pub fn aql(&self) -> &str {
        &self.aql
    }

    /// When the registry stored it, the ITS-REST `StoredQuery.saved`.
    #[must_use]
    pub fn saved(&self) -> Timestamp {
        self.saved
    }
}
