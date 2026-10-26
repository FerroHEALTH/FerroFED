// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The export of the routing state one gateway process learned, which a
//! system that replaces the gateway reads to route follow-ups as it did.
//!
//! It carries the `creating_system_id` mappings the process learned from
//! answers (§12.2, N21): `system_id`s, `endpoint_id`s and `node_id`s only.
//! It never carries a resolution binding, an `ehr_id`, the development
//! cross-reference or any other value a patient is known by, so no patient
//! identifier can be in it (§5.4.1, N33). The membership document and the
//! stored queries are left out because the operator already holds both.
//!
//! ```
//! use ferrofed_registry::creating_system::LearnedMap;
//! use ferrofed_registry::export::{FORMAT_VERSION, IdentityStateExport};
//! use ferrofed_registry::snapshot::RegistrySnapshot;
//!
//! let registry = RegistrySnapshot::from_toml_str(
//!     r#"
//!     [[organisation]]
//!     id = "org-a"
//!
//!     [[node]]
//!     id = "node-a"
//!     organisation = "org-a"
//!     system_id = "cdr-a.example.org"
//!
//!     [[endpoint]]
//!     id = "node-a-pub"
//!     node = "node-a"
//!     url = "https://cdr-a.example.org/openehr"
//!     connection_type = "openehr-rest-query"
//!     managing_organisation = "org-a"
//!     "#,
//! )?;
//! let (learned, at) = (LearnedMap::new(), jiff::Timestamp::UNIX_EPOCH);
//! let replica = "ferrofed-0".parse()?;
//! let export = IdentityStateExport::of("example-federation", &replica, &registry, &learned, at);
//! assert_eq!(FORMAT_VERSION, export.format_version);
//! assert!(export.learned_creating_systems.is_empty());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
// NOTE: Regulation (EU) 2025/327 Annex II 2.6; no specification governs the format:
// our own design, versioned so a reader refuses a version it does not know.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::creating_system::LearnedMap;
use crate::replica::Replica;
use crate::snapshot::RegistrySnapshot;

/// The version of the export format this crate writes.
///
/// A change that removes a field or changes a field's meaning raises it; a
/// reader refuses a version it does not know.
pub const FORMAT_VERSION: u32 = 1;

/// The body of the operator's export of this process's routing state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityStateExport {
    /// The version of the format, [`FORMAT_VERSION`] when this crate wrote it.
    pub format_version: u32,
    /// The federation's own identifier (§7a.2, N30).
    pub federation_id: String,
    /// When the process took the export, RFC 3339.
    pub exported_at: String,
    /// The replica whose state the export holds: each replica learns its
    /// own maps, so a deployment of several replicas is exported at each of
    /// them.
    pub replica: Replica,
    /// Every `creating_system_id` mapping the process learned from answers,
    /// ordered by `creating_system_id`.
    pub learned_creating_systems: Vec<LearnedCreatingSystem>,
    /// The state the process holds that the export leaves out, so a reader
    /// of the file alone knows it is not there.
    pub not_exported: Vec<NotExported>,
}

impl IdentityStateExport {
    /// The export of `learned`, read against `snapshot`, for the federation
    /// `federation_id`, taken `at` at `replica`.
    #[must_use]
    pub fn of(
        federation_id: &str,
        replica: &Replica,
        snapshot: &RegistrySnapshot,
        learned: &LearnedMap,
        at: Timestamp,
    ) -> Self {
        let learned_creating_systems = learned
            .entries()
            .map(|(creating_system_id, holder)| LearnedCreatingSystem {
                creating_system_id: creating_system_id.to_string(),
                status: if holder.is_some() {
                    LearnedStatus::Learned
                } else {
                    LearnedStatus::Withdrawn
                },
                node: holder.and_then(|endpoint| {
                    snapshot
                        .endpoint(endpoint)
                        .map(|declared| declared.node().to_string())
                }),
                endpoint: holder.map(ToString::to_string),
            })
            .collect();
        Self {
            format_version: FORMAT_VERSION,
            federation_id: federation_id.to_owned(),
            exported_at: at.to_string(),
            replica: replica.clone(),
            learned_creating_systems,
            not_exported: NotExported::ALL.to_vec(),
        }
    }
}

/// Whether a learned mapping routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LearnedStatus {
    /// The process saw versions of the `creating_system_id` at one node
    /// only, and routes its follow-up reads there.
    Learned,
    /// An integrity incident withdrew the mapping: versions were seen at
    /// two nodes, or at a node the membership document contradicts, and
    /// the process routes nothing by it.
    Withdrawn,
}

/// One `creating_system_id` mapping the process learned from answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnedCreatingSystem {
    /// The `creating_system_id`, as the version uid spelled it.
    pub creating_system_id: String,
    /// Whether the mapping routes.
    pub status: LearnedStatus,
    /// The node the endpoint belongs to, absent for a withdrawn mapping.
    pub node: Option<String>,
    /// The endpoint the versions were seen at, absent for a withdrawn
    /// mapping.
    pub endpoint: Option<String>,
}

/// State the process holds that an export leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotExported {
    /// The resolution bindings of each client session: scoped to the
    /// session and bounded by a time to live, and a replacement asks the
    /// identifier cross-reference itself (§12.5.1 step 2).
    ResolutionBindings,
    /// The `ehr_id` index: one entry per patient's EHR, a hint that the
    /// ask-all probe of §12.5.1 step 4 rebuilds.
    EhrIdIndex,
    /// The membership document, which the operator reviews and holds.
    Membership,
    /// The stored queries, which their backend holds and the operator
    /// surface reads.
    StoredQueries,
}

impl NotExported {
    /// Every kind of state an export leaves out, in the order it lists them.
    pub const ALL: [Self; 4] = [
        Self::ResolutionBindings,
        Self::EhrIdIndex,
        Self::Membership,
        Self::StoredQueries,
    ];
}

#[cfg(test)]
mod tests {
    use super::{IdentityStateExport, LearnedStatus, NotExported};
    use crate::creating_system::LearnedMap;
    use crate::snapshot::RegistrySnapshot;

    const REGISTRY: &str = r#"
        [[organisation]]
        id = "org-a"

        [[node]]
        id = "node-a"
        organisation = "org-a"
        system_id = "cdr-a.example.org"

        [[node]]
        id = "node-b"
        organisation = "org-a"
        system_id = "cdr-b.example.org"

        [[endpoint]]
        id = "node-a-pub"
        node = "node-a"
        url = "https://cdr-a.example.org/openehr"
        connection_type = "openehr-rest-query"
        managing_organisation = "org-a"

        [[endpoint]]
        id = "node-b-pub"
        node = "node-b"
        url = "https://cdr-b.example.org/openehr"
        connection_type = "openehr-rest-query"
        managing_organisation = "org-a"
    "#;

    #[test]
    fn learned_and_withdrawn_mappings_are_exported_in_order() {
        let snapshot = RegistrySnapshot::from_toml_str(REGISTRY).expect("a registry");
        let mut learned = LearnedMap::new();
        let version = |system: &str| {
            format!("8849182c-82ad-4088-a07f-48ead4180515::{system}::1")
                .parse()
                .expect("a version uid")
        };
        let at = |endpoint: &str| endpoint.parse().expect("an endpoint");
        learned
            .observe(
                &snapshot,
                &version("legacy-z.example.org"),
                &at("node-b-pub"),
            )
            .expect("learned");
        learned
            .observe(
                &snapshot,
                &version("legacy-y.example.org"),
                &at("node-a-pub"),
            )
            .expect("learned");
        learned
            .observe(
                &snapshot,
                &version("legacy-y.example.org"),
                &at("node-b-pub"),
            )
            .expect("withdrawn");
        let replica = "ferrofed-0".parse().expect("a replica");
        let export = IdentityStateExport::of(
            "example-federation",
            &replica,
            &snapshot,
            &learned,
            jiff::Timestamp::UNIX_EPOCH,
        );
        let rows: Vec<_> = export
            .learned_creating_systems
            .iter()
            .map(|row| {
                (
                    row.creating_system_id.as_str(),
                    row.status,
                    row.node.as_deref(),
                    row.endpoint.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            vec![
                ("legacy-y.example.org", LearnedStatus::Withdrawn, None, None),
                (
                    "legacy-z.example.org",
                    LearnedStatus::Learned,
                    Some("node-b"),
                    Some("node-b-pub")
                ),
            ],
            rows
        );
        assert_eq!(NotExported::ALL.to_vec(), export.not_exported);
        assert_eq!("1970-01-01T00:00:00Z", export.exported_at);
        assert_eq!("ferrofed-0", export.replica.as_str());
    }
}
