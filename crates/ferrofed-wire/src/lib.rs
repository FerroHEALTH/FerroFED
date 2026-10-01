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
//! # Examples
//!
//! ```
//! use ferrofed_wire::id::EndpointId;
//! use ferrofed_wire::meta::FederationMeta;
//! use ferrofed_wire::outcome::{EndpointOutcome, Outcome};
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

pub mod envelope;
pub mod error;
pub mod headers;
pub mod id;
pub mod meta;
pub mod options;
pub mod outcome;
pub mod status;

/// The Federation Tier with AQL specification release this crate implements.
///
/// The `OPTIONS {base}/` self-description reports its `major.minor` as
/// `spec_version` (§7a.2, [`options::SpecVersion::of_release`]).
pub const FEDERATION_SPEC: &str = "0.9.0";
