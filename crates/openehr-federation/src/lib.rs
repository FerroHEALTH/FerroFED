// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The wire additions of the openEHR Federation Tier with AQL specification,
//! as typed carriers held to its two published JSON Schemas.
//!
//! A federation gateway answers an AQL query with an ordinary ITS-REST
//! `RESULT_SET` and adds one member to its open `meta` object,
//! `meta.federation` ([`meta::FederationMeta`]), carrying the per-endpoint
//! record of what happened ([`outcome::EndpointOutcome`], with the §11.1
//! status vocabulary of [`status::EndpointStatus`]). [`envelope`] puts that
//! member into the `openehr-its` `ResultSetMetadata` and reads it back.
//! [`options::OptionsRoot`] is the `OPTIONS {base}/` self-description, and
//! [`headers`] names the federation's HTTP headers.
//!
//! Every object in the schemas is open, so each type keeps the members it
//! does not model ([`object::Extra`]). The schemas' conditionals are
//! invariants of construction, and the readers refuse what the specification
//! rules out, including combinations a schema cannot state, such as a
//! `complete` that disagrees with the reported statuses.
//!
//! The wire format is JSON text: deserialize with `serde_json::from_str`,
//! `from_slice` or `from_reader`.
//!
//! The wire types are always present. Two features add the rest of the
//! specification's gateway logic: `aql`, the §7 rewrite of a client query into
//! one `ehr_id`-scoped query per node, and `merge`, the §9 to §11 merge of the
//! node answers. [`order::ResultOrder`] is the plain description of the Tier
//! order the rewrite produces and the merge reads, and
//! [`aggregate::Recombination`] the plain description of how the answers of an
//! aggregate query recombine across nodes. [`dedup::DedupMode`] names the
//! §10 deduplication a request selects.
//!
//! # Examples
//!
//! ```
//! use openehr_federation::id::EndpointId;
//! use openehr_federation::meta::FederationMeta;
//! use openehr_federation::outcome::{EndpointOutcome, Outcome};
//!
//! let answered = EndpointOutcome::new(EndpointId::new("node_1")?, Outcome::Active { latency_ms: 118 });
//! let excluded = EndpointOutcome::new(EndpointId::new("node_3")?, Outcome::Excluded { error: None });
//! let federation = FederationMeta::new(vec![answered, excluded])?;
//! assert!(federation.complete(), "an excluded endpoint was never in scope");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![doc(test(attr(deny(warnings))))]

#[macro_use]
pub mod object;

pub mod aggregate;
pub mod dedup;
pub mod envelope;
pub mod error;
pub mod headers;
pub mod id;
pub mod meta;
pub mod options;
pub mod order;
pub mod outcome;
pub mod status;

#[cfg(feature = "aql")]
pub mod aql;
#[cfg(feature = "merge")]
pub mod merge;

/// The openEHR AQL release the `aql` feature rewrites.
///
/// The specification binds AQL by release
/// (<https://specifications.openehr.org/releases/QUERY/Release-1.1.0/AQL.html>).
pub const AQL: &str = "1.1.0";

/// The Federation Tier with AQL specification release this crate implements.
///
/// The `OPTIONS {base}/` self-description reports its `major.minor` as
/// `spec_version` (§7a.2, [`options::SpecVersion::of_release`]).
pub const FEDERATION_SPEC: &str = "0.9.0";
