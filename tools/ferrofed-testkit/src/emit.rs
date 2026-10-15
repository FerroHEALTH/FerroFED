// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where a test writes an exchange-format document it has held to the
//! vendored profiles, so HL7's FHIR Validator can read it afterwards (#688).
//!
//! A test calls [`document`] after its own checks pass. With
//! [`EMIT_DIR_VARIABLE`] unset, the call does nothing, so an ordinary test
//! run writes no file; the CI lane sets it, runs the tests, and runs the
//! validator over the directory. No specification governs this: our own
//! design.

use std::io;
use std::path::PathBuf;

/// The environment variable naming the directory the tests write their
/// documents to.
pub const EMIT_DIR_VARIABLE: &str = "FERROFED_EMIT_DIR";

/// Writes `text` to `<dir>/<name>.json` when [`EMIT_DIR_VARIABLE`] names a
/// directory `dir`, creating it if needed, and returns the path written.
///
/// Returns `None`, and writes nothing, when the variable is unset. The name
/// is the document's stable identity across runs, so each call site passes
/// its own.
///
/// # Errors
///
/// Returns an [`io::Error`] of kind [`io::ErrorKind::InvalidInput`] when
/// `name` is empty or holds anything but ASCII letters, digits, `-` and
/// `_`, and the error of the file system when the directory or the file
/// cannot be written.
pub fn document(name: &str, text: &str) -> io::Result<Option<PathBuf>> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("an emitted document name is a plain file stem, not {name:?}"),
        ));
    }
    let Some(dir) = std::env::var_os(EMIT_DIR_VARIABLE) else {
        return Ok(None);
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.json"));
    std::fs::write(&path, text)?;
    Ok(Some(path))
}
