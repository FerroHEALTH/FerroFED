// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the patient reference stays redacted, and the static
//! development cross-reference resolves only under the development profile
//! (§5.2, §5.4, N3, N6, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

mod patient;
mod static_resolver;
mod support;
