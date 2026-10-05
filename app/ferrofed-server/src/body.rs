// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The JSON documents this server answers with on its own routes.
//!
//! Each is a typed struct serialized by `serde`, never a free-form JSON value.
//! The error bodies are [`crate::error`]'s. No specification governs these
//! documents: our own design.

use ferrofed_registry::manufacturer::{MANUFACTURER, Manufacturer};
use serde::Serialize;

/// The product name the root document reports.
pub const PRODUCT: &str = "FerroFED";

/// The product version the root document reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `GET /`: the product, its version and its manufacturer.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Root {
    /// The product name.
    pub product: &'static str,
    /// The product version.
    pub version: &'static str,
    /// The manufacturer, named in the running system as Regulation (EU)
    /// 2025/327 Art 30(1)(g) asks.
    pub manufacturer: Manufacturer,
}

impl Default for Root {
    fn default() -> Self {
        Self {
            product: PRODUCT,
            version: VERSION,
            manufacturer: MANUFACTURER,
        }
    }
}

/// `GET /health`: the process is up.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Liveness {
    /// Always `up`: a process that can answer is live.
    pub state: crate::health::State,
}
