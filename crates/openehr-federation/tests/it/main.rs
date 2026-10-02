// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the wire types held to the vendored schemas and
//! examples of the Federation Tier with AQL specification, in three layers
//! (validation, drift, and the rules no schema states), and the crate's
//! pinned specification version against the pin matrix; with the `aql`
//! feature, the §7.1 rewrite; with the `merge` feature, the §11.6.1 merge.
//!
//! The modules carry `#[cfg(test)]`, which an integration test always has, so
//! the test-scoped relaxations of `clippy.toml` reach their helpers too.

#[cfg(test)]
mod aql;
#[cfg(test)]
mod drift;
#[cfg(test)]
mod envelope;
#[cfg(test)]
mod examples;
#[cfg(test)]
mod merge;
#[cfg(test)]
mod options;
#[cfg(test)]
mod outcomes;
#[cfg(test)]
mod pins;
#[cfg(test)]
mod properties;
#[cfg(test)]
mod support;
