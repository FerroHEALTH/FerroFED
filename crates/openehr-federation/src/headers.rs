// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HTTP header names and values the specification defines.
//!
//! HTTP field names are case-insensitive (RFC 9110 §5.1); these are the
//! spellings the specification uses.

/// The endpoint a request is targeted at (§8.4, N35), and on a response the
/// acting endpoint of a request routed to a single node (§7a.3, N31).
pub const ENDPOINT: &str = "openEHR-federation-endpoint";

/// The organisation a request is targeted at (§8.4).
pub const ORGANISATION: &str = "openEHR-federation-organisation";

/// The acting node's openEHR `system_id` on a routed response (§7a.3, N31).
pub const SYSTEM_ID: &str = "openEHR-federation-system-id";

/// The request header that selects the completion strategy (§11.4, N37).
pub const COMPLETENESS: &str = "openEHR-federation-completeness";

/// The request header that selects the dedup mode (§10, N15), the name
/// `OPTIONS {base}/` declares as `dedup.request_header` (§7a.2). It carries a
/// [`crate::dedup::DedupMode`] name.
pub const DEDUP: &str = "openEHR-federation-dedup";

/// The [`COMPLETENESS`] value that states the default all-or-nothing
/// completion explicitly (§11.4); a gateway accepts it even though it is the
/// default.
pub const COMPLETENESS_ALL: &str = "all";

/// The [`COMPLETENESS`] value that opts a request into best-effort
/// completion (§11.4).
pub const COMPLETENESS_PARTIAL: &str = "partial";

/// Every header name above, in the order this module declares them.
pub const ALL: [&str; 5] = [ENDPOINT, ORGANISATION, SYSTEM_ID, COMPLETENESS, DEDUP];
