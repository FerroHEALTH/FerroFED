// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the harness records the Federation-Node profile of its CDR
//! products.
//!
//! The checks themselves are the gateway's, in
//! `ferrofed_server::conformance::node_profile`, which `conformance run
//! --node-profile` runs against a deployment's members; the harness runs the
//! same checks against FerroEHR and EHRbase and writes each product's
//! findings to [`findings_dir`], which `scripts/conformance/report.sh`
//! reports as the node class, apart from the gateway's points. No
//! specification governs the form of the record: our own design.

use std::path::{Path, PathBuf};

/// The environment variable naming the directory the harness writes its
/// findings to, in place of [`DEFAULT_FINDINGS_DIR`].
pub const FINDINGS_DIR_VARIABLE: &str = "FERROFED_NODE_PROFILE_DIR";

/// The directory the harness writes its findings to by default, relative to
/// the workspace root, beside the conformance report.
pub const DEFAULT_FINDINGS_DIR: &str = "target/conformance/node-profile";

/// Returns the directory findings are written to: the one
/// [`FINDINGS_DIR_VARIABLE`] names, else [`DEFAULT_FINDINGS_DIR`] under the
/// workspace root.
#[must_use]
pub fn findings_dir() -> PathBuf {
    std::env::var_os(FINDINGS_DIR_VARIABLE).map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(DEFAULT_FINDINGS_DIR)
        },
        PathBuf::from,
    )
}
