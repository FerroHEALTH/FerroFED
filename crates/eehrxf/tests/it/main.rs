// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The integration tests of `eehrxf`, one binary with a module per subject.
//!
//! The modules carry `#[cfg(test)]`, which an integration test always has, so
//! the test-scoped relaxations of `clippy.toml` reach their helpers too.

#[cfg(test)]
mod category;
#[cfg(test)]
#[cfg(feature = "patient-summary")]
mod crosswalk;
#[cfg(test)]
mod dataset;
#[cfg(test)]
#[cfg(all(feature = "openehr", feature = "patient-summary"))]
mod document;
#[cfg(test)]
#[cfg(feature = "openehr")]
mod mapping;
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
