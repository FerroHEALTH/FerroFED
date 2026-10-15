// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation registry: members, endpoints and system ids from a reviewed
//! document, the learned maps, integrity incidents and the stored-query
//! definition store.
//!
//! The registry is the operator's record of membership (§12b.1) and the
//! follow-up routing table (N21). Organisations, nodes with their openEHR
//! `system_id`, and endpoints with their base URLs come from a reviewed
//! bootstrap document, validated at load into an immutable
//! [`RegistrySnapshot`](snapshot::RegistrySnapshot) (§12b.1; the document
//! format is our own design, since no specification governs it). The document
//! is TOML, or FHIR `Organization` and `Endpoint` resources (N19) read into
//! the same [`Document`](document::Document) outside this crate. `node_id`,
//! `endpoint_id` and `system_id` are three types with no conversion between
//! them (N32, §12a.1, see [`id`]).
//!
//! ```
//! use ferrofed_registry::id::{EndpointId, NodeId, SystemId};
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
//!
//! let system_id: SystemId = "cdr-a.example.org".parse()?;
//! let node = registry.node_for_system_id(&system_id).map(|node| node.id());
//! assert_eq!(node, Some(&"node-a".parse::<NodeId>()?));
//! let endpoint: EndpointId = "node-a-pub".parse()?;
//! assert!(registry.endpoint(&endpoint).is_some());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! A member that serves an exchange-format category itself declares a second
//! endpoint of connection type `hl7-fhir-rest`, a
//! [`NativeEndpoint`](native::NativeEndpoint) the snapshot keeps apart from
//! every Query API endpoint, so no query or directive ever reaches it (N19,
//! §8).
//!
//! The follow-up routing table maps every observed `creating_system_id`, not
//! only the members' own `system_id`s (N21, §12.2): the document registers
//! more, a [`LearnedMap`](creating_system::LearnedMap) learns the rest from
//! answers, and a conflict raises an [`Incident`](incident::Incident). The
//! [`EhrIndex`](ehr_index::EhrIndex) learns which member holds an `ehr_id`,
//! the third step of path `ehr_id` routing (§12.5.1). [`operator`] holds
//! the reports the gateway's read-only operator surface answers with, and
//! [`health`] the body of the gateway's `GET {base}/operator/dependencies`. Each of them names
//! the process that produced it, a [`replica::Replica`].
//!
//! Every credential FerroFED is configured with, and every URL that may carry
//! one in its userinfo, is held in a [`Secret`](secret::Secret) or a
//! [`SecretUrl`](secret::SecretUrl), whose renderings never show it.
//!
//! [`console`] holds a member the operator console manages beside the
//! document's, owned whole, and the report of who owns each configured
//! resource.
//! [`resource`] holds the credentials and binding sections the console
//! manages whole, with their secrets as references.
//! [`revision`] holds the gateway's configuration revisions as its admin
//! listener writes them and the console reads them.
//!
//! [`manufacturer`] names the manufacturer of FerroFED, the one source the
//! gateway and the operator console both read it from.
#![doc(test(attr(deny(warnings))))]

pub mod console;
pub mod creating_system;
pub mod definition;
pub mod document;
pub mod ehr_index;
pub mod error;
pub mod export;
pub mod health;
pub mod id;
pub mod incident;
pub mod manufacturer;
pub mod native;
pub mod operator;
pub mod replica;
pub mod resource;
pub mod revision;
pub mod secret;
pub mod snapshot;
