// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `FerroHEALTH` installation report to `FerroPULSE`.
//!
//! One `start` report at each `serve` start and one `daily` report a day,
//! carrying the installation id, the version, the licence grant type, the
//! deployment kind, coarse host size, the member count as a range, the
//! configured identity and consent services, and latency aggregates per
//! route group with the node request counts. The wire contract is
//! `FerroPULSE`'s report envelope v2 (`docs/report-v2.md`, vendored under
//! `docs/specs/ferropulse/`), and every report the server builds is
//! validated against its schema in the tests. A report carries no patient
//! data, no AQL text, no request path, no node URL or name, and no
//! identifier other than the instance id: the payload types have no member
//! that could hold one, and [`window`] classifies a request by its matched
//! route template alone. It is on by default; `[usage_report] enabled =
//! false` stops it, a CI environment stops it, and so does the lack of a
//! writable state directory. One boot line says which
//! ([`reporter::announce`]). No specification governs any of this: our own
//! design.
//!
//! - [`config`]: the section, resolved, and the CI detection.
//! - [`payload`]: the typed report.
//! - [`window`]: the in-process metrics window and its rendering.
//! - [`state`]: the instance id and the last start time on disk.
//! - [`reporter`]: assembly, sending, scheduling.

pub mod config;
pub mod payload;
pub mod reporter;
pub mod state;
pub mod window;
