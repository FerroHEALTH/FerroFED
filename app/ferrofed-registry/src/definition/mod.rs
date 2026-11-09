// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query definitions of the federated registry (§12.7, N44): the
//! qualified query name, the semver version, the held definition, and the
//! store behind them.
//!
//! A gateway that offers the registry is authoritative for each definition
//! and versions it with ITS-REST's own semver path segment, and a stored
//! version is immutable (§12.7, N44). The names, versions and version
//! patterns are `openehr-its`'s ITS-REST Query API types
//! (`rest::stored_query`); the registry adds what is its own: a name it
//! stores is never the reserved `aql`, and the version it assigns a
//! definition stored with none.
//!
//! ```
//! use ferrofed_registry::definition::{assigned_after, query_name};
//! use openehr_its::rest::stored_query::{QueryVersion, VersionPattern};
//!
//! let name = query_name("org.example::compositions")?;
//! assert_eq!(Some("org.example"), name.namespace());
//! assert!(query_name("aql").is_err());
//! let version: QueryVersion = "1.10.0".parse()?;
//! assert!(version > "1.9.3".parse()?);
//! let pattern: VersionPattern = "1".parse()?;
//! assert!(pattern.matches(&version));
//! assert_eq!(Some(QueryVersion::new(1, 10, 1)), assigned_after(Some(version)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The definitions live behind [`store::DefinitionStore`], whose writes are
//! an insert that refuses a held name and version and an insert at the next
//! version the store assigns, and every read goes through the in-memory
//! [`store::Definitions`] (no specification governs the storage: our own
//! design).

pub mod store;

use jiff::Timestamp;
use openehr_its::rest::stored_query::{QualifiedQueryName, QueryNameError, QueryVersion};

/// Reads `text` as the qualified name of a query the registry stores or
/// addresses, `[{namespace}::]{query-name}` (ITS-REST Query API, "Qualified
/// query name").
///
/// The registry refuses the reserved `aql` in any case wherever a name
/// arrives, a read included, so a request that names it is answered as an
/// invalid name. A refusal never quotes what was sent.
///
/// # Errors
///
/// [`QueryNameError`] when `text` is not `[{namespace}::]{query-name}` over
/// the ITS-REST characters, is `.` or `..` without a namespace, or names the
/// reserved `aql`.
pub fn query_name(text: &str) -> Result<QualifiedQueryName, QueryNameError> {
    let name: QualifiedQueryName = text.parse()?;
    name.check_registrable()?;
    Ok(name)
}

/// Returns the version the registry assigns a definition stored with no
/// version (§12.7 stored-query-versioning).
///
/// With `newest` the newest version of its name the registry holds, that is
/// `1.0.0` for a name it holds no version of, else the next patch of
/// `newest`.
///
/// The newest is the highest by [`Ord`], which compares the numbers, so
/// `1.10.0` is newer than `1.9.0`. Returns `None` when the patch of `newest`
/// is the largest a version holds, so it has no next patch.
#[must_use]
pub fn assigned_after(newest: Option<QueryVersion>) -> Option<QueryVersion> {
    match newest {
        None => Some(QueryVersion::new(1, 0, 0)),
        Some(newest) => newest
            .patch()
            .checked_add(1)
            .map(|patch| QueryVersion::new(newest.major(), newest.minor(), patch)),
    }
}

/// One stored-query definition at one version, as the registry holds it.
///
/// The AQL is the text the registry admitted, which names any patient
/// through a `$parameter`, so a definition holds no patient identifier
/// (§5.4.1, N33).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDefinition {
    name: QualifiedQueryName,
    version: QueryVersion,
    aql: String,
    saved: Timestamp,
}

impl StoredDefinition {
    /// The definition of `name` at `version`, holding `aql`, stored at
    /// `saved`.
    #[must_use]
    pub fn new(
        name: QualifiedQueryName,
        version: QueryVersion,
        aql: String,
        saved: Timestamp,
    ) -> Self {
        Self {
            name,
            version,
            aql,
            saved,
        }
    }

    /// The qualified name.
    #[must_use]
    pub fn name(&self) -> &QualifiedQueryName {
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
