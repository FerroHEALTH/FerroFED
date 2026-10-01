// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation wire additions of the Federation Tier with AQL
//! specification: `meta.federation`, the endpoint status vocabulary and the
//! federation headers, as typed carriers held to the published schemas.
//!
//! Version 0.0.0 holds the crate in the workspace; the implementation lands
//! with FerroFED issue #33, following `docs/architecture.md` section 11.
#![doc(test(attr(deny(warnings))))]

/// The Federation Tier with AQL specification version this crate implements.
///
/// The `OPTIONS {base}/` self-description reports it as `spec_version`
/// (§7a.2).
pub const FEDERATION_SPEC: &str = "0.9.0";

// TODO(#33): the implementation this crate holds the place for.
