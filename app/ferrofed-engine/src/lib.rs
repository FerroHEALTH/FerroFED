// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation engine: dispatch and fan-out to each node over ITS-REST, the
//! per-node and overall budgets, the completeness decision and the outbound
//! identifier-hygiene gate.
//!
//! [`dispatch`] holds the per-endpoint node client and the mapping from a
//! node's answer to its §11.1 endpoint status (#34). [`fanout`] sends one
//! request per in-scope node under one deadline, builds `meta.federation` from
//! every outcome and applies the all-or-nothing decision (#37; §11.4, §11.5,
//! N37, N38). [`forward`] passes one client request to one node once,
//! byte-identical (§7a.3, N22, N31). [`probe`] asks every member at once
//! whether it holds a path `ehr_id`, the read-only last step of §12.5.1.
//! [`hygiene`] is the outbound gate every
//! request to a node passes before it is sent (#45). [`outbound_id`] is the
//! correlation id the gateway mints for a node request, with the inventory of
//! every header a node request carries (§5.4.1, N33).
#![doc(test(attr(deny(warnings))))]

// TODO(#64): follow-up reads routed on creating_system_id, then endpoint_id, then ask-all (§12.3).

pub mod dispatch;
pub mod fanout;
pub mod forward;
pub mod hygiene;
pub mod outbound_id;
pub mod probe;

/// The openEHR ITS-REST release the engine dispatches to each node.
///
/// The specification binds ITS-REST by release
/// (<https://specifications.openehr.org/releases/ITS-REST/Release-1.1.0/>).
pub const ITS_REST: &str = "1.1.0";
