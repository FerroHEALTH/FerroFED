// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the patient reference stays redacted, the static
//! development cross-reference resolves only under the development profile,
//! the PIXm resolver reads each member's `ehr_id` from its domain and fails
//! closed, and the resolution bindings stay in their session (§5.2, §5.4,
//! §12.5.1, N3, N6, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

#[cfg(feature = "ihe")]
mod atna;
mod localizer;
#[cfg(test)]
#[cfg(feature = "ihe")]
mod mcsd;
#[cfg(feature = "nl")]
mod mitz;
#[cfg(all(feature = "ihe", feature = "nl"))]
mod nvi_directory;
#[cfg(feature = "nl")]
mod nvi_localizer;
mod patient;
#[cfg(feature = "ihe")]
mod pdqm;
#[cfg(feature = "ihe")]
mod pixm;
#[cfg(feature = "ihe")]
mod pixm_identify;
#[cfg(feature = "ihe")]
mod pixm_localizer;
#[cfg(feature = "ihe")]
mod pmir;
mod session;
mod static_consent;
mod static_resolver;
mod support;
#[cfg(feature = "ihe")]
mod timing;
#[cfg(feature = "ihe")]
mod xcpd;
