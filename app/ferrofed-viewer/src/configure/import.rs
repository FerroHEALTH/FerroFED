// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The import of a configuration as TOML: its limit. Its form and its
//! answer are rendered with the page's others (`render`).
//!
//! The operator pastes the TOML an export wrote, or one kept in version
//! control, and the first replica saves it as a dry-run revision on the
//! newest one: nothing is applied, and the page shows what it changes
//! against the running revision before the operator applies it with the
//! form every revision takes. No specification governs this: our own
//! design.

/// The largest TOML an import sends, in bytes, as the gateway takes it.
pub const IMPORT_LIMIT: usize = 1_048_576;
