// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The European interoperability software component of an EHR system
//! (Regulation (EU) 2025/327 Art 2(2)(n), Art 15, Art 25(1)).
//!
//! The Regulation asks an EHR system to provide and receive personal
//! electronic health data of the priority categories of Art 14(1) in the
//! European electronic health record exchange format. Until the Art 15(1)
//! implementing act fixes that format, this crate holds the published proxies
//! for it:
//!
//! - [`dataset`]: the format-neutral dataset model, read as data from the
//!   Xt-EHR *EHDS Logical Information Models* package and keyed on its
//!   element paths, with the producer and consumer obligations of each
//!   element;
//! - [`category`]: the priority categories this build carries, one Cargo
//!   feature each, and the logical model and obligations profile of each;
//! - [`crosswalk`]: per category, the crosswalk from the Xt-EHR element
//!   paths to the eHealth Network element ids and to the elements and slices
//!   of the HL7 Europe profile, with the FHIRconnect contexts admitted to
//!   feed each, checked against the packages it names;
//! - `document` (features `patient-summary` and `fhir-r4`): the patient
//!   summary document, a FHIR R4 `Bundle` of the HL7 Europe Patient Summary
//!   profiles, assembled from the resources and sections a caller gives;
//! - `mapping` (feature `openehr`, which turns on `fhir-r4`): a FHIRconnect
//!   1.0.0 mapping, compiled once against an operational template and run in
//!   process over a canonical-JSON composition, answering FHIR R4 resources
//!   with their `Provenance`;
//! - `receive` (feature `fhir-r4`): a document received in the exchange
//!   format, read under the R4 document rules and checked against the
//!   profiles of its category; with `openehr`, mapped into one openEHR
//!   composition that keeps the original document beside the mapped content.
//!
//! The feature `fhir-r4` is the FHIR R4 serialisation and compiles no
//! openEHR crate; only `openehr` brings in the openEHR side the mapping reads.
//!
//! The crate depends on no logging component and on nothing of the system
//! that serves the documents, so the interoperability component stays
//! independent of the logging component (Art 2(2)(n), (o)).
//!
//! # Examples
//!
//! ```no_run
//! use eehrxf::dataset::DatasetModel;
//!
//! let package = std::fs::File::open("xtehr.eu.ehds.models-1.0.0.tgz")?;
//! let model = DatasetModel::read(package)?;
//! let summary = model
//!     .model("http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSPatientSummary")
//!     .ok_or("the package holds no patient summary model")?;
//! assert!(summary.element("EHDSPatientSummary.allergiesAndIntolerances").is_some());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![doc(test(attr(deny(warnings))))]

pub mod category;
pub mod crosswalk;
pub mod dataset;
#[cfg(all(feature = "patient-summary", feature = "fhir-r4"))]
pub mod document;
#[cfg(feature = "openehr")]
pub mod mapping;
#[cfg(feature = "fhir-r4")]
pub mod receive;
