// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The static development cross-reference: a fixed table from a synthetic
//! patient identifier to each member's `ehr_id`.
//!
//! It is FerroFED's own testing device and binds nothing: it is not an
//! identity binding of N3, and a PIXm resolver replaces it (#42). It is
//! accepted only in a configuration explicitly marked for development (no
//! specification governs this: our own design):
//!
//! ```toml
//! profile = "development"
//!
//! [[dev.crossref]]
//! namespace = "2.999.1"
//! value = "12345"
//! member = "node-a"
//! ehr_id = "6f2a51a4-1b8e-4f8b-9a4c-1f6c2b1d7e30"
//!
//! [[dev.consent_denied]]
//! namespace = "2.999.1"
//! value = "12345"
//! member = "node-b"
//! ```
//!
//! The optional `[[dev.consent_denied]]` rows are the static consent
//! pre-filter: the patient's consent denies asking each member a row names
//! (N27a). It is no consent binding either, and the Mitz adapter replaces it
//! (#87).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use thiserror::Error;

use crate::consent::{ConsentDecision, ConsentPrefilter, Requester};
use crate::localizer::{Localization, Localizer};
use crate::patient::{IdentifierNamespace, PatientRef};
use crate::resolver::{Resolution, Resolver};

/// The deployment profile a server configuration declares.
///
/// Only [`Profile::Development`] admits development-only devices such as the
/// static cross-reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// A deployment that serves real requests.
    Production,
    /// A deployment explicitly marked for development and testing.
    Development,
}

/// One row of the development cross-reference, as the configuration writes
/// it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryDoc {
    namespace: IdentifierNamespace,
    value: String,
    member: NodeId,
    ehr_id: EhrId,
}

/// The `[dev]` table of a server configuration.
///
/// Its `Debug` output counts the rows and shows none of them, because each
/// row carries a patient identifier value.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevTable {
    crossref: Vec<EntryDoc>,
    #[serde(default)]
    consent_denied: Vec<ConsentDoc>,
}

/// One row of the development consent pre-filter, as the configuration
/// writes it: the patient whose consent denies asking `member`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsentDoc {
    namespace: IdentifierNamespace,
    value: String,
    member: NodeId,
}

impl fmt::Debug for DevTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevTable")
            .field("crossref_rows", &self.crossref.len())
            .field("consent_denied_rows", &self.consent_denied.len())
            .finish()
    }
}

/// A development cross-reference that cannot be enabled.
///
/// The errors name the member or the namespace, never the identifier value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum DevCrossRefError {
    /// The configuration carries the table but is not marked for development.
    #[error("the static cross-reference is accepted only under profile = \"development\"")]
    NotDevelopment,
    /// A row's identifier value is empty.
    #[error("a cross-reference row in namespace {0} has an empty value")]
    EmptyValue(IdentifierNamespace),
    /// A row names a member the registry does not hold.
    #[error("a cross-reference row names member {0}, which is not in the registry")]
    UnknownMember(NodeId),
    /// Two rows map the same identifier at the same member.
    #[error(
        "two cross-reference rows map one identifier in namespace {namespace} at member {member}"
    )]
    DuplicateRow {
        /// The identifier's namespace.
        namespace: IdentifierNamespace,
        /// The member both rows name.
        member: NodeId,
    },
}

struct Row {
    namespace: IdentifierNamespace,
    value: SecretString,
    member: NodeId,
    ehr_id: EhrId,
}

/// The [`Resolver`] and the [`Localizer`] over the static development
/// cross-reference.
///
/// As a resolver it answers [`Resolution::Resolved`] for a row of the table
/// and [`Resolution::Unknown`] for every other member asked. As a localizer
/// it names the members that have a row for the patient, and answers
/// [`Localization::NoRecords`] when none has. It never answers that it is
/// unavailable.
pub struct StaticResolver {
    rows: Vec<Row>,
}

/// The warning logged when the static cross-reference is enabled.
pub const STATIC_CROSS_REFERENCE_WARNING: &str =
    "static cross-reference: development only, not an identity binding";

impl StaticResolver {
    /// Builds the static resolver a configuration asks for, if any.
    ///
    /// Returns `None` when the configuration has no `[dev]` table. The table is
    /// refused outside [`Profile::Development`], so a server in any other
    /// profile cannot start with it. Every row must name a registry member,
    /// and no two rows may map one identifier at one member. Enabling it
    /// logs [`STATIC_CROSS_REFERENCE_WARNING`] at `WARN`, with the row count
    /// and none of the rows.
    ///
    /// # Errors
    ///
    /// [`DevCrossRefError::NotDevelopment`] for a table outside the
    /// development profile, and the other variants for a row that breaks a
    /// rule above.
    pub fn from_config(
        profile: Profile,
        table: Option<DevTable>,
        registry: &RegistrySnapshot,
    ) -> Result<Option<Self>, DevCrossRefError> {
        let Some(table) = table else {
            return Ok(None);
        };
        if profile != Profile::Development {
            return Err(DevCrossRefError::NotDevelopment);
        }
        let mut rows: Vec<Row> = Vec::with_capacity(table.crossref.len());
        for entry in table.crossref {
            if entry.value.is_empty() {
                return Err(DevCrossRefError::EmptyValue(entry.namespace));
            }
            if registry.node(&entry.member).is_none() {
                return Err(DevCrossRefError::UnknownMember(entry.member));
            }
            let duplicate = rows.iter().any(|row| {
                row.namespace == entry.namespace
                    && row.member == entry.member
                    && row.value.expose_secret() == entry.value
            });
            if duplicate {
                return Err(DevCrossRefError::DuplicateRow {
                    namespace: entry.namespace,
                    member: entry.member,
                });
            }
            rows.push(Row {
                namespace: entry.namespace,
                value: entry.value.into(),
                member: entry.member,
                ehr_id: entry.ehr_id,
            });
        }
        tracing::warn!(rows = rows.len(), "{STATIC_CROSS_REFERENCE_WARNING}");
        Ok(Some(Self { rows }))
    }

    fn lookup(&self, patient: &PatientRef, member: &NodeId) -> Option<&EhrId> {
        self.rows
            .iter()
            .find(|row| {
                row.member == *member
                    && row.namespace == *patient.namespace()
                    && row.value.expose_secret() == patient.value()
            })
            .map(|row| &row.ehr_id)
    }
}

impl fmt::Debug for StaticResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticResolver")
            .field("rows", &self.rows.len())
            .finish()
    }
}

#[async_trait]
impl Resolver for StaticResolver {
    async fn resolve(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        _deadline: Instant,
    ) -> BTreeMap<NodeId, Resolution> {
        members
            .iter()
            .map(|member| {
                let resolution = match self.lookup(patient, member) {
                    Some(ehr_id) => Resolution::Resolved(ehr_id.clone()),
                    None => Resolution::Unknown,
                };
                (member.clone(), resolution)
            })
            .collect()
    }
}

#[async_trait]
impl Localizer for StaticResolver {
    async fn localize(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        _deadline: Instant,
    ) -> Localization {
        let candidates: BTreeSet<NodeId> = members
            .iter()
            .filter(|member| self.lookup(patient, member).is_some())
            .cloned()
            .collect();
        if candidates.is_empty() {
            Localization::NoRecords
        } else {
            Localization::Candidates(candidates)
        }
    }
}

/// One patient whose consent denies asking one member.
struct Denial {
    namespace: IdentifierNamespace,
    value: SecretString,
    member: NodeId,
}

/// The name `OPTIONS {base}/` declares the static consent pre-filter under.
pub const STATIC_CONSENT_MODE: &str = "development-static";

/// The warning logged when the static consent pre-filter is enabled.
pub const STATIC_CONSENT_WARNING: &str =
    "static consent pre-filter: development only, not a consent binding";

/// The [`ConsentPrefilter`] over the `[[dev.consent_denied]]` rows of the
/// development table (N27a).
///
/// It answers [`ConsentDecision::Denied`] with the candidates a row names for
/// the patient, and [`ConsentDecision::NoSignal`] when no row names one; it
/// never answers [`ConsentDecision::Unavailable`]. A candidate it does not
/// deny is still asked, and its node checks consent itself (N27).
pub struct StaticConsentPrefilter {
    denials: Vec<Denial>,
}

impl StaticConsentPrefilter {
    /// Builds the static pre-filter the development table asks for, if any.
    ///
    /// Returns `None` when the table has no `[[dev.consent_denied]]` row. The
    /// rows are refused outside [`Profile::Development`]; each must name a
    /// registry member, and no two rows may deny one member for one
    /// identifier. Enabling it logs [`STATIC_CONSENT_WARNING`] at `WARN`, with
    /// the row count and none of the rows.
    ///
    /// # Errors
    ///
    /// [`DevCrossRefError::NotDevelopment`] outside the development profile,
    /// and the other variants for a row that breaks a rule above.
    pub fn from_config(
        profile: Profile,
        table: &DevTable,
        registry: &RegistrySnapshot,
    ) -> Result<Option<Self>, DevCrossRefError> {
        if table.consent_denied.is_empty() {
            return Ok(None);
        }
        if profile != Profile::Development {
            return Err(DevCrossRefError::NotDevelopment);
        }
        let mut denials: Vec<Denial> = Vec::with_capacity(table.consent_denied.len());
        for row in &table.consent_denied {
            if row.value.is_empty() {
                return Err(DevCrossRefError::EmptyValue(row.namespace.clone()));
            }
            if registry.node(&row.member).is_none() {
                return Err(DevCrossRefError::UnknownMember(row.member.clone()));
            }
            let duplicate = denials.iter().any(|denial| {
                denial.namespace == row.namespace
                    && denial.member == row.member
                    && denial.value.expose_secret() == row.value
            });
            if duplicate {
                return Err(DevCrossRefError::DuplicateRow {
                    namespace: row.namespace.clone(),
                    member: row.member.clone(),
                });
            }
            denials.push(Denial {
                namespace: row.namespace.clone(),
                value: row.value.clone().into(),
                member: row.member.clone(),
            });
        }
        tracing::warn!(rows = denials.len(), "{STATIC_CONSENT_WARNING}");
        Ok(Some(Self { denials }))
    }

    fn denies(&self, patient: &PatientRef, member: &NodeId) -> bool {
        self.denials.iter().any(|denial| {
            denial.member == *member
                && denial.namespace == *patient.namespace()
                && denial.value.expose_secret() == patient.value()
        })
    }
}

impl fmt::Debug for StaticConsentPrefilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticConsentPrefilter")
            .field("rows", &self.denials.len())
            .finish()
    }
}

#[async_trait]
impl ConsentPrefilter for StaticConsentPrefilter {
    async fn prefilter(
        &self,
        patient: &PatientRef,
        _requester: Option<&Requester>,
        candidates: &[NodeId],
        _deadline: Instant,
    ) -> ConsentDecision {
        let denied: BTreeSet<NodeId> = candidates
            .iter()
            .filter(|member| self.denies(patient, member))
            .cloned()
            .collect();
        if denied.is_empty() {
            ConsentDecision::NoSignal
        } else {
            ConsentDecision::Denied(denied)
        }
    }

    fn mode(&self) -> &'static str {
        STATIC_CONSENT_MODE
    }
}
