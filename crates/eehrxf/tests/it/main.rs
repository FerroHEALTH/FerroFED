// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The integration tests of `eehrxf`, one binary with a module per subject.
//!
//! The modules carry `#[cfg(test)]`, which an integration test always has, so
//! the test-scoped relaxations of `clippy.toml` reach their helpers too.

#[cfg(test)]
mod category;
#[cfg(test)]
#[cfg(all(feature = "openehr", feature = "patient-summary"))]
mod claimed;
#[cfg(test)]
#[cfg(feature = "patient-summary")]
mod crosswalk;
#[cfg(test)]
mod dataset;
#[cfg(test)]
#[cfg(feature = "discharge")]
mod discharge;
#[cfg(test)]
#[cfg(all(feature = "discharge", feature = "fhir-r4"))]
mod discharge_document;
#[cfg(test)]
#[cfg(all(feature = "openehr", feature = "patient-summary"))]
mod document;
#[cfg(test)]
#[cfg(feature = "imaging")]
mod imaging;
#[cfg(test)]
#[cfg(all(feature = "imaging", feature = "fhir-r4"))]
mod imaging_document;
#[cfg(test)]
#[cfg(all(feature = "laboratory", feature = "fhir-r4"))]
mod lab_document;
#[cfg(test)]
#[cfg(feature = "laboratory")]
mod laboratory;
#[cfg(test)]
#[cfg(feature = "openehr")]
mod mapping;
#[cfg(test)]
#[cfg(any(feature = "prescription", feature = "dispensation"))]
mod medication;
#[cfg(test)]
#[cfg(all(feature = "openehr", feature = "patient-summary"))]
mod own;
#[cfg(test)]
mod profile;
#[cfg(test)]
mod property;
#[cfg(test)]
#[cfg(feature = "fhir-r4")]
mod receive;
#[cfg(test)]
mod refusals;
#[cfg(test)]
mod support;
#[cfg(test)]
#[cfg(feature = "fhir-r4")]
mod terminology;
#[cfg(test)]
#[cfg(all(feature = "openehr", feature = "patient-summary"))]
mod transformation;
