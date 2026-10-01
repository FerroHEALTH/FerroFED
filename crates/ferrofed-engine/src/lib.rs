// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation engine: dispatch and fan-out to each node over ITS-REST, the
//! per-node and overall budgets, the completeness decision and follow-up
//! routing on the creating system id.
//!
//! Version 0.0.0 holds the crate in the workspace; the implementation lands
//! with FerroFED issue #37, following `docs/architecture.md` section 11.
#![doc(test(attr(deny(warnings))))]

/// The openEHR ITS-REST release the engine dispatches to each node.
///
/// The specification binds ITS-REST by release
/// (<https://specifications.openehr.org/releases/ITS-REST/Release-1.1.0/>).
pub const ITS_REST: &str = "1.1.0";

// TODO(#37): the implementation this crate holds the place for.
