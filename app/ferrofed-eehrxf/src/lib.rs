// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation half of the European interoperability software component
//! (Regulation (EU) 2025/327 Art 2(2)(n)).
//!
//! The component itself, `eehrxf`, holds the format-neutral dataset, its
//! crosswalk and the mapping, and knows nothing of the gateway. This crate is
//! the gateway's side of it:
//!
//! - [`reserved`]: the namespace the gateway reserves for its own stored
//!   queries, which no `PUT` stores into and no store may hold;
//! - [`patient_summary`]: one stored query per patient summary section,
//!   selecting the whole compositions that feed it, each with its template
//!   id, from every member (Federation Tier §12.7, N44);
//! - [`summary`]: the patient summary document assembled from what the
//!   members answer to the section queries, each composition mapped by the
//!   FHIRconnect mappings the deployment supplies;
//! - [`face`]: the FHIR R4 face that serves it (Regulation (EU) 2025/327
//!   Annex II 2.1): the summary request, the `CapabilityStatement` and the
//!   `OperationOutcome`;
//! - [`list`]: the document list of that face, a `DocumentReference` search
//!   answering the summary written on request and each member's own one;
//! - [`lookup`]: the patient lookup of that face, a `Patient` search by
//!   identifier answering the `Patient` the identity binding holds;
//! - [`receive`]: a document received in the exchange format, checked and
//!   mapped for the one member the deployment declares for its category
//!   (Annex II 2.2 and 2.3), its required bindings held to the expansions of
//!   its package and its codings displayed;
//! - [`serving`]: the members the deployment declares to serve electronic
//!   prescriptions and dispensations on that face, each on its own endpoint;
//! - [`dispensations`]: a dispensation received on that face, read for the
//!   prescription it names and the member that owns it;
//! - [`prescriptions`]: the search of a patient's prescriptions on that
//!   face, each member's page read and every prescription passed through
//!   with the `Provenance` that names its member;
//! - [`terminology`]: the FHIR terminology server the component asks, over
//!   HTTP;
//! - [`transformation`]: the code systems a coded element is transcoded
//!   into, for the report of each semantic transformation the face applies.
//!
//! Each query names its patient through `$patient` and `$namespace` alone,
//! so it passes the stored-query admission and the rewrite scopes it to each
//! member's own `ehr_id`; no patient identifier reaches a node (§5.4.1, N33).
//!
//! ```
//! use ferrofed_eehrxf::patient_summary::Section;
//! use ferrofed_eehrxf::reserved;
//!
//! let held = Section::Problems.definition()?;
//! assert!(reserved::reserves(held.name()));
//! assert_eq!(reserved::VERSION, held.version());
//! assert!(held.aql().contains("openEHR-EHR-EVALUATION.problem_diagnosis.v1"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![doc(test(attr(deny(warnings))))]

pub mod dispensations;
pub mod face;
pub mod list;
pub mod lookup;
pub mod patient_summary;
pub mod prescriptions;
pub mod receive;
pub mod reserved;
pub mod serving;
pub mod summary;
pub mod terminology;
pub mod transformation;
