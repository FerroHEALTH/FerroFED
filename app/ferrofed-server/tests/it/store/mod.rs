// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroFED's own PostgreSQL store, `[store]`, as configured: needed by a
//! contact-point issuer and by the stored-query registry's `postgres`
//! backend and refused naming both when it is missing, never needed
//! otherwise, its connection string a secret that always requires TLS, and
//! its bounds checked ([`config`]). Against a real database the store is
//! the e2e suite's (`crate::e2e::store`). No specification governs the
//! store: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

mod config;
