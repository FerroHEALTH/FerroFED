// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The AQL rewrite of the Federation Tier: the patient carrier consumed at the
//! gateway and each node query rewritten to its own `ehr_id`, as
//! transformations of the openEHR AQL syntax tree with no I/O.
//!
//! Version 0.0.0 holds the crate in the workspace; the implementation lands
//! with FerroFED issue #35, following `docs/architecture.md` section 11.
#![doc(test(attr(deny(warnings))))]

/// The openEHR AQL release this crate rewrites.
///
/// The specification binds AQL by release
/// (<https://specifications.openehr.org/releases/QUERY/Release-1.1.0/AQL.html>).
pub const AQL: &str = "1.1.0";

// TODO(#35): the implementation this crate holds the place for.
