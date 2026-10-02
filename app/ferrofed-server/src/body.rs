// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The JSON documents this server answers with on its own routes.
//!
//! Each is a typed struct serialized by `serde`, never a free-form JSON value.
//! The error bodies are [`crate::error`]'s. No specification governs these
//! documents: our own design.

use serde::Serialize;

/// The product name the root document reports.
pub const PRODUCT: &str = "FerroFED";

/// The product version the root document reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `GET /`: the product and its version.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Root {
    /// The product name.
    pub product: &'static str,
    /// The product version.
    pub version: &'static str,
}

impl Default for Root {
    fn default() -> Self {
        Self {
            product: PRODUCT,
            version: VERSION,
        }
    }
}

/// `GET /health`: the process is up.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Liveness {
    /// Always `up`: a process that can answer is live.
    pub state: crate::health::State,
}
