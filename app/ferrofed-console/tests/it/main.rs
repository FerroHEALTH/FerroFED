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
mod admin;
#[cfg(test)]
mod body_limit;
#[cfg(test)]
mod config;
#[cfg(test)]
mod configure;
#[cfg(test)]
mod configure_fhir;
#[cfg(test)]
mod configure_pages;
#[cfg(test)]
mod configure_refusals;
#[cfg(test)]
mod configure_revisions;
#[cfg(test)]
mod configure_sections;
#[cfg(test)]
mod configure_settings;
#[cfg(test)]
mod describe;
#[cfg(test)]
mod exchange;
#[cfg(test)]
mod gateway;
#[cfg(test)]
mod overview;
#[cfg(test)]
mod pages;
#[cfg(test)]
mod query;
#[cfg(test)]
mod refresh;
#[cfg(test)]
mod secrets;
#[cfg(test)]
mod server;
#[cfg(test)]
mod shell;
#[cfg(test)]
mod sign_in;
#[cfg(test)]
mod sign_out;
#[cfg(test)]
mod stored;
#[cfg(test)]
mod stub;
#[cfg(test)]
mod support;
#[cfg(test)]
mod views;
