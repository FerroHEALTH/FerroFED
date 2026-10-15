// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic prescription and dispensation: their crosswalks to the
//! HL7 Europe MPD profiles, and the datasets read from and written as MPD
//! resources, over synthetic content in the `urn:oid:2.999` example arc.

mod crosswalk;
#[cfg(all(feature = "fhir-r4", feature = "dispensation"))]
mod dispensation;
#[cfg(all(feature = "fhir-r4", feature = "prescription"))]
mod extension;
#[cfg(feature = "fhir-r4")]
mod fixtures;
#[cfg(all(feature = "fhir-r4", feature = "prescription"))]
mod prescription;
