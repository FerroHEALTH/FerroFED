// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The deployment profile a server configuration declares.

use serde::Deserialize;

/// The deployment profile a server configuration declares.
///
/// [`Profile::Development`] relaxes the checks a deployment that serves real
/// requests must pass, such as cleartext to a loopback address; it admits
/// no patient index (no specification governs this: our own design).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// A deployment that serves real requests.
    Production,
    /// A deployment explicitly marked for development and testing.
    Development,
}
