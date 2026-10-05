// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `conformance run` job: the Connectathon tracks of §16.3 driven
//! against a configured deployment over ITS-REST, and scored per track and
//! per point as §16.4 asks.
//!
//! The run reaches the gateway as a client does ([`client`]), seeds one
//! synthetic patient at the members its real cross-reference names, through
//! each node's own ITS-REST writes ([`fixture`]), runs every scenario of the
//! [`catalogue`] ([`scenarios`]), and writes the report the harness run
//! writes ([`report`]). With `--node-profile` it also runs the
//! Federation-Node profile checks against each active member
//! ([`node_profile`]). [`safety`] holds what the run refuses before it
//! writes anything, and [`run`] puts the parts together.
//!
//! The scenario checks are the ones the end-to-end suite runs against its
//! FerroEHR nodes: the suite calls the same functions and adds what only its
//! capturing proxies and fault injection can observe. A scenario a live
//! deployment cannot provide for (an injected fault, node-side wire
//! capture, a gateway configured for the one scenario) is reported
//! `not-run` with its reason, never `pass`. No specification governs the
//! form of the run: our own design.

pub mod catalogue;
pub mod client;
pub mod execute;
pub mod fixture;
pub mod node_profile;
pub mod report;
pub mod run;
pub mod safety;
pub mod scenarios;
pub mod seed;

use crate::conformance::client::GatewayError;

/// A scenario check that did not hold, or could not be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Failure {
    /// An expectation of the scenario did not hold.
    #[error("{0}")]
    Check(String),
    /// The gateway could not be asked.
    #[error("{step}: the gateway could not be asked")]
    Gateway {
        /// The request, as method and path.
        step: String,
        /// What the client reported.
        #[source]
        source: GatewayError,
    },
    /// An answer is not the document the scenario reads.
    #[error("{what} could not be read")]
    Read {
        /// The answer that was read.
        what: String,
        /// What the JSON reader reported.
        #[source]
        source: serde_json::Error,
    },
}

/// Returns `Ok` when `holds`, and the [`Failure::Check`] `what` describes
/// otherwise.
///
/// # Errors
///
/// Returns [`Failure::Check`] when `holds` is false.
pub fn ensure(holds: bool, what: impl FnOnce() -> String) -> Result<(), Failure> {
    if holds {
        Ok(())
    } else {
        Err(Failure::Check(what()))
    }
}

/// Returns `Ok` when `expected` equals `actual`, and the [`Failure::Check`]
/// naming both and `what` otherwise.
///
/// # Errors
///
/// Returns [`Failure::Check`] when the two differ.
pub fn ensure_eq<T: PartialEq + std::fmt::Debug>(
    expected: &T,
    actual: &T,
    what: &str,
) -> Result<(), Failure> {
    ensure(expected == actual, || {
        format!("{what}: expected {expected:?}, got {actual:?}")
    })
}
