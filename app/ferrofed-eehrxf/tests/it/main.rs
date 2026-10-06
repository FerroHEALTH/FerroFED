// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: every patient summary section query is built through
//! `openehr-query`, names its patient by parameters alone, sits under the
//! reserved name at its immutable version, selects by the archetypes the
//! vendored International Patient Summary template names for its section,
//! and every crosswalk section has a query or a recorded reason (§12.7, N44,
//! §5.4.1, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the written document is read back as JSON"
)]

mod crosswalk;
mod query;
#[cfg(test)]
mod summary;
mod template;
