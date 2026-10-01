// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation engine: dispatch and fan-out to each node over ITS-REST, the
//! per-node and overall budgets, the completeness decision and follow-up
//! routing on the creating system id.
//!
//! [`dispatch`] holds the per-endpoint node client and the mapping from a
//! node's answer to its §11.1 endpoint status (#34). [`fanout`] sends one
//! request per in-scope node under one deadline, builds `meta.federation` from
//! every outcome and applies the all-or-nothing decision (#37), following
//! `docs/architecture.md` sections 5 and 9.
#![doc(test(attr(deny(warnings))))]

pub mod dispatch;
pub mod fanout;

/// The openEHR ITS-REST release the engine dispatches to each node.
///
/// The specification binds ITS-REST by release
/// (<https://specifications.openehr.org/releases/ITS-REST/Release-1.1.0/>).
pub const ITS_REST: &str = "1.1.0";
