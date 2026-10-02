// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the bootstrap document loads into a snapshot that
//! answers every lookup, refuses every document that breaks a membership rule,
//! and keeps `node_id`, `endpoint_id` and `system_id` apart (N19, N20, N21,
//! N32, §12a.1, §12b).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

mod creating_system;
mod fixture;
mod ids;
mod load;
mod namespaces;
mod refusal;
