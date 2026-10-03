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
//! N37, N38). [`ehr`] creates and reads an EHR on one node for the admission
//! check (§12b.1, #79). [`forward`] passes one client request to one node once,
//! byte-identical (§7a.3, N22, N31). [`probe`] asks every member at once
//! whether it holds a path `ehr_id`, the read-only last step of §12.5.1.
//! A fan-out answer names the versions each endpoint's rows show it holding,
//! which the follow-up routing table learns from (§12.2, N21).
//! [`hygiene`] is the outbound gate every
//! request to a node passes before it is sent (#45), and [`declared`] holds
//! each header and query value a routed request forwards to the kind its
//! ITS-REST operation declares (§5.4.1, N33). [`outbound_id`] is the
//! correlation id the gateway mints for a node request, with the inventory of
//! every header a node request carries (§5.4.1, N33).
#![doc(test(attr(deny(warnings))))]

pub mod declared;
pub mod dispatch;
pub mod ehr;
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
