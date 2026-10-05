// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identifiers the registry answers for, one type per namespace.
//!
//! `node_id`, `endpoint_id` and `system_id` are distinct namespaces, and the
//! registry resolves each unambiguously (N32, §12a.1). Each is its own type
//! with no conversion into another, so a `system_id` can never be accepted
//! where an `endpoint_id` is expected because the strings coincide:
//!
//! ```compile_fail,E0308
//! use ferrofed_registry::id::{EndpointId, NodeId};
//!
//! fn dispatch_to(_endpoint: EndpointId) {}
//!
//! let node: NodeId = "node-1".parse().unwrap();
//! dispatch_to(node); // a NodeId is never an EndpointId
//! ```
//!
//! ```compile_fail,E0277
//! use ferrofed_registry::id::{EndpointId, SystemId};
//!
//! let system: SystemId = "cdr1.example.org".parse().unwrap();
//! let _endpoint = EndpointId::from(system); // no conversion between namespaces
//! ```
//!
//! ```compile_fail,E0277
//! use ferrofed_registry::id::{NodeId, SystemId};
//!
//! let node: NodeId = "node-1".parse().unwrap();
//! let _system: SystemId = node.into(); // no conversion between namespaces
//! ```
//!
//! Each value is built from its own string form instead:
//!
//! ```
//! use ferrofed_registry::id::{EndpointId, NodeId, SystemId};
//!
//! fn dispatch_to(_endpoint: EndpointId) {}
//!
//! let endpoint: EndpointId = "node-1".parse()?;
//! let node: NodeId = "node-1".parse()?;
//! let system: SystemId = "node-1".parse()?;
//! dispatch_to(endpoint);
//! assert_eq!(node.as_str(), system.as_str());
//! # Ok::<(), ferrofed_registry::error::IdError>(())
//! ```

use std::fmt;
use std::str::FromStr;

use openehr_base::prelude::{HierObjectId, ObjectVersionId, Uid};
use openehr_base::v1_3::base_types::identification::lexical::composite_id_key;
use serde::Deserialize;

use crate::error::{IdError, IdKind};

/// The longest registry-owned identifier, in bytes.
pub const MAX_ID_LEN: usize = 64;

// NOTE: no specification governs the lexical form of `node_id`, `endpoint_id`
// or an organisation id (§ The four identifiers), so this rule is FerroFED's own.
fn validate_registry_id(kind: IdKind, value: &str) -> Result<(), IdError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(IdError::Empty { kind });
    };
    if value.len() > MAX_ID_LEN {
        return Err(IdError::TooLong {
            kind,
            found: value.to_owned(),
        });
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
    if !first.is_ascii_alphanumeric() || !chars.all(allowed) {
        return Err(IdError::Malformed {
            kind,
            found: value.to_owned(),
        });
    }
    Ok(())
}

/// The registry's handle for a member of the federation: one organisation's
/// CDR deployment as the unit of membership, trust and audit (§ The four
/// identifiers, §12b.1).
///
/// The form is 1 to 64 ASCII letters, digits, `.`, `-` and `_`, starting with
/// a letter or digit, because the value travels in headers and directives.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct NodeId(String);

/// The registry's handle for one reachable interface of a node: one base URL
/// with one connection type (N19, § The four identifiers).
///
/// It is the identifier of the `FROM ENDPOINT` directive, the endpoint header
/// and the `endpoint_id` row attribute (§8, §9.3), and re-addressing a node
/// never changes it. The form is the one [`NodeId`] has.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct EndpointId(String);

/// The registry's handle for an organisation: the one that operates a node,
/// or the managing organisation of an endpoint (N20).
///
/// The form is the one [`NodeId`] has.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct OrganisationId(String);

macro_rules! registry_id {
    ($ty:ident, $kind:expr) => {
        impl $ty {
            /// Builds the identifier from its string form.
            ///
            /// # Errors
            ///
            /// [`IdError::Empty`], [`IdError::TooLong`] or
            /// [`IdError::Malformed`] when the value breaks the form above.
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                validate_registry_id($kind, &value)?;
                Ok(Self(value))
            }

            /// The identifier as written.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $ty {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::new(s)
            }
        }

        impl TryFrom<String> for $ty {
            type Error = IdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

registry_id!(NodeId, IdKind::Node);
registry_id!(EndpointId, IdKind::Endpoint);
registry_id!(OrganisationId, IdKind::Organisation);

/// A node's openEHR `system_id`: `EHR.system_id`, the logical EHR-management
/// system in which an EHR was created (§ The four identifiers).
///
/// The value is an openEHR `UID` (BASE `master05-identification_package.adoc`
/// §Syntaxes), the form a `creating_system_id` takes, so a node's `system_id`
/// compares directly with the middle segment of an `OBJECT_VERSION_ID` (§12a.1).
/// The value is stored as written, and two values that differ only in ASCII case
/// are the same identifier (master05 §"Composite Identifiers and Case").
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct SystemId {
    value: String,
    key: String,
}

impl SystemId {
    /// Builds a `system_id` from its string form.
    ///
    /// # Errors
    ///
    /// [`IdError::SystemId`] when the value is not an openEHR `uid`
    /// (`iso_oid | uuid | internet_id`).
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if let Err(source) = Uid::new(&value) {
            return Err(IdError::SystemId {
                found: value,
                source,
            });
        }
        let key = composite_id_key(&value);
        Ok(Self { value, key })
    }

    /// The `creating_system_id` of a version: the middle segment of its
    /// `OBJECT_VERSION_ID`, read through `openehr-base` as written (BASE
    /// `OBJECT_VERSION_ID.creating_system_id`, §12.2).
    ///
    /// # Errors
    ///
    /// [`IdError::SystemId`] when the segment is not an openEHR `uid`, which a
    /// version id built through [`ObjectVersionId::new`] never yields.
    pub fn creating_system_id_of(version: &ObjectVersionId) -> Result<Self, IdError> {
        Self::new(version.creating_system_id_str())
    }

    /// The `system_id` as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl PartialEq for SystemId {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for SystemId {}

impl PartialOrd for SystemId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SystemId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

impl std::hash::Hash for SystemId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl FromStr for SystemId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for SystemId {
    type Error = IdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for SystemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

/// A node-local `ehr_id`: the `HIER_OBJECT_ID` under which one node knows
/// one patient's EHR (§5.2).
///
/// An `ehr_id` is all a node is located by (N33). The value is stored as
/// written, and two values that differ only in ASCII case are the same
/// identifier (master05 §"Composite Identifiers and Case").
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct EhrId {
    id: HierObjectId,
    key: String,
}

impl EhrId {
    /// Builds an `ehr_id` from its string form.
    ///
    /// # Errors
    ///
    /// [`IdError::EhrId`] when the value is not a `HIER_OBJECT_ID`.
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        // NOTE: BASE 1.3.0 base_types §Syntaxes makes `extension` any string, the
        // empty one included, so `a::` is a HIER_OBJECT_ID with no extension.
        match HierObjectId::new(value.as_str()) {
            Ok(id) => {
                let key = composite_id_key(id.value());
                Ok(Self { id, key })
            }
            Err(source) => Err(IdError::EhrId {
                found: value,
                source,
            }),
        }
    }

    /// The `ehr_id` as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.id.value()
    }

    /// The openEHR `HIER_OBJECT_ID` this `ehr_id` is.
    #[must_use]
    pub fn hier_object_id(&self) -> &HierObjectId {
        &self.id
    }

    /// Whether this `ehr_id` is a bare UUID: its root is a `UUID` and it has
    /// no extension (BASE `UID_BASED_ID.root`, `UID_BASED_ID.extension`).
    ///
    /// ```
    /// use ferrofed_registry::id::EhrId;
    ///
    /// let minted: EhrId = "7d44b88c-4199-4bad-97dc-d78268e01398".parse()?;
    /// let oid: EhrId = "12345".parse()?;
    /// assert!(minted.is_uuid());
    /// assert!(!oid.is_uuid());
    /// # Ok::<(), ferrofed_registry::error::IdError>(())
    /// ```
    #[must_use]
    pub fn is_uuid(&self) -> bool {
        matches!(self.id.root(), Uid::Uuid(_)) && !self.id.has_extension()
    }
}

impl PartialEq for EhrId {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for EhrId {}

impl PartialOrd for EhrId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EhrId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

impl std::hash::Hash for EhrId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl FromStr for EhrId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for EhrId {
    type Error = IdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for EhrId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
