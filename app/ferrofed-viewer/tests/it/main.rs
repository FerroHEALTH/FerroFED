// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator console's integration tests: its configuration, its HTTP
//! surface driven in-process, and its client of the gateway against a stub.
//!
//! The modules carry `#[cfg(test)]`, which an integration test always has, so
//! the test-scoped relaxations of `clippy.toml` reach their helpers too.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

#[cfg(test)]
mod config;
#[cfg(test)]
mod exchange;
#[cfg(test)]
mod gateway;
#[cfg(test)]
mod query;
#[cfg(test)]
mod secrets;
#[cfg(test)]
mod server;
#[cfg(test)]
mod sign_in;
#[cfg(test)]
mod sign_out;
#[cfg(test)]
mod support;
#[cfg(test)]
mod views;
