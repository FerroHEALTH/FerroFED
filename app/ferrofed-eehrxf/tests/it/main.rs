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

mod crosswalk;
mod query;
mod template;
