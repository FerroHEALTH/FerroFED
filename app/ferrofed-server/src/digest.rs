// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The digests the gateway names content and patients by.
//!
//! The SHA-256 digest names the content of the configuration, the
//! conformance fixtures and the support report, and the pseudonym names a
//! patient in the security log. No specification governs where they live:
//! our own design.

use std::fmt::Write as _;
use std::sync::LazyLock;

use aws_lc_rs::hmac;
use aws_lc_rs::rand::SystemRandom;

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

/// The key the pseudonyms of one process are drawn with, generated at
/// start and never written anywhere.
static PSEUDONYM_KEY: LazyLock<Option<hmac::Key>> = LazyLock::new(|| {
    // NOTE: no specification governs this: our own design; with no random source the
    // record names no pseudonym, and never the identifier in its place.
    hmac::Key::generate(hmac::HMAC_SHA256, &SystemRandom::new()).ok()
});

/// The pseudonym of `identifier`: the first 16 bytes of its HMAC-SHA256
/// under this process's key, in hexadecimal.
pub(crate) fn pseudonym(identifier: &str) -> String {
    let Some(key) = PSEUDONYM_KEY.as_ref() else {
        return String::from("unavailable");
    };
    let tag = hmac::sign(key, identifier.as_bytes());
    tag.as_ref()
        .iter()
        .take(16)
        .fold(String::with_capacity(32), |mut hex, byte| {
            let _hex_written = write!(hex, "{byte:02x}");
            hex
        })
}
