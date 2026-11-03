// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the security events are logged: the tracing target every lane
//! writes them under, and the directive that keeps it on. No specification
//! governs this: our own design.

/// The tracing target of every security event, so an operator can route them
/// apart from the request log.
pub const TARGET: &str = "ferrofed::security";

/// The directive the console writes the security events through whatever
/// `telemetry.filter` says: [`TARGET`] at `info` and above.
pub const DIRECTIVE: &str = "ferrofed::security=info";
