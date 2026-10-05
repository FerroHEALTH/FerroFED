// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation engine: every request the gateway sends to a node, how it
//! authenticates there, and the checks each request passes before it leaves.
//!
//! - [`dispatch`]: the per-endpoint node client over `openehr-its`'s
//!   `rest-client`, and the mapping from a node's answer to its §11.1
//!   endpoint status (N16, N40).
//! - [`fanout`]: one request per in-scope node under one deadline, the
//!   `meta.federation` envelope built from every outcome, and the
//!   all-or-nothing decision (§11.4, §11.5, N37, N38). A fan-out answer names
//!   the versions each endpoint's rows show it holding, which the follow-up
//!   routing table learns from (§12.2, N21).
//! - [`single_node`]: the calls that send one request to one node: EHR
//!   creation and retrieval for the admission check (`single_node::ehr`,
//!   §12b.1), one client request forwarded byte-identical
//!   (`single_node::forward`, §7a.3, N22, N31), and the read-only probe of
//!   §12.5.1 step 4 (`single_node::probe`).
//! - [`hygiene`]: the outbound gate every request to a node passes before it
//!   is sent (§5.4.1, N33).
//! - [`declared`]: each header and query value a routed request forwards,
//!   held to the kind its ITS-REST operation declares (§5.4.1, N33).
//! - [`outbound_id`]: the correlation id the gateway mints for a node
//!   request, with the inventory of every header a node request carries
//!   (§5.4.1, N33).
//! - [`onward`]: how the gateway authenticates to a node as itself (§13.1,
//!   N25): the grant kinds in `onward::grant` (client credentials, RFC 8693
//!   token exchange, FAPI 2.0, and under feature `nl` the Nuts grant of
//!   Annex B §B.4), the token request and the signed client assertion they
//!   share, the keys the gateway publishes as its JWK Set, and the two sender
//!   constraints, `DPoP` (RFC 9449) and mutual TLS (RFC 8705).
//! - [`conveyance`]: what a node is told about the caller, a token the
//!   gateway signs for that node and sends in a header of every request
//!   (§13.1, N24).
//! - [`trace_context`]: the span of each node request, and the W3C
//!   `traceparent` it carries when the gateway exports traces.
#![doc(test(attr(deny(warnings))))]

pub mod conveyance;
pub mod declared;
pub mod dispatch;
pub mod fanout;
pub mod hygiene;
pub mod onward;
pub mod outbound_id;
pub mod single_node;
pub mod trace_context;

/// The openEHR ITS-REST release the engine dispatches to each node.
///
/// The specification binds ITS-REST by release
/// (<https://specifications.openehr.org/releases/ITS-REST/Release-1.1.0/>).
pub const ITS_REST: &str = "1.1.0";
