// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SHA-256 digest the configuration, the conformance fixtures and the
//! support report name their content by. No specification governs where it
//! lives: our own design.

use std::fmt::Write as _;

/// Returns the hexadecimal SHA-256 digest of `bytes` (FIPS 180-4).
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _hex_written = write!(hex, "{byte:02x}");
            hex
        })
}
