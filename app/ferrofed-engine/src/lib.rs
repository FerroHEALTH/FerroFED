// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation engine: dispatch and fan-out to each node over ITS-REST, the
//! per-node and overall budgets, the completeness decision and follow-up
//! routing on the creating system id.
//!
//! [`dispatch`] holds the per-endpoint node client and the mapping from a
//! node's answer to its §11.1 endpoint status (#34). The fan-out and the
//! budgets over it land with FerroFED issue #37, following
//! `docs/architecture.md` sections 5 and 9.
#![doc(test(attr(deny(warnings))))]

pub mod dispatch;

/// The openEHR ITS-REST release the engine dispatches to each node.
///
/// The specification binds ITS-REST by release
/// (<https://specifications.openehr.org/releases/ITS-REST/Release-1.1.0/>).
pub const ITS_REST: &str = "1.1.0";

// TODO(#37): the fan-out over the node clients, with its budgets.
