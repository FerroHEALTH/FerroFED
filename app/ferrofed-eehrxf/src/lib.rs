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
//! - [`receive`]: a document received in the exchange format, checked and
//!   mapped for the one member the deployment declares for its category
//!   (Annex II 2.2 and 2.3).
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

pub mod face;
pub mod patient_summary;
pub mod receive;
pub mod reserved;
pub mod summary;
