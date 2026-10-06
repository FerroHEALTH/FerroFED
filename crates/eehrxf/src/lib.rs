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
//! - `mapping` (feature `openehr`, which turns on `fhir-r4`): a FHIRconnect
//!   1.0.0 mapping, compiled once against an operational template and run in
//!   process over a canonical-JSON composition, answering FHIR R4 resources
//!   with their `Provenance`.
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
#[cfg(feature = "openehr")]
pub mod mapping;
