// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The integration tests of `eehrxf`, one binary with a module per subject.
//!
//! The modules carry `#[cfg(test)]`, which an integration test always has, so
//! the test-scoped relaxations of `clippy.toml` reach their helpers too.

#[cfg(test)]
mod category;
#[cfg(test)]
mod dataset;
#[cfg(test)]
#[cfg(feature = "fhir-r4")]
mod mapping;
#[cfg(test)]
mod property;
#[cfg(test)]
mod refusals;
#[cfg(test)]
mod support;
